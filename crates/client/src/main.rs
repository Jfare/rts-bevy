mod audio_sfx;
mod camera;
mod chat;
mod combat;
mod command_marker;
mod fog_of_war;
mod mining;
mod minimap;
mod net;
mod particles;
mod pings;
mod placement;
mod production;
mod render_units;
mod selection;
mod stats;
mod ui;
mod unit_movement;
mod world_grid;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use audio_sfx::AudioSfxPlugin;
use camera::{RtsCamera, RtsCameraPlugin};
use chat::ChatPlugin;
use combat::CombatPlugin;
use command_marker::CommandMarkerPlugin;
use fog_of_war::FogOfWarPlugin;
use mining::MiningPlugin;
use minimap::MinimapPlugin;
use net::NetClientPlugin;
use particles::ParticlesPlugin;
use pings::TacticalPingPlugin;
use placement::PlacementPlugin;
use production::ProductionPlugin;
use render_units::RenderUnitsPlugin;
use selection::SelectionPlugin;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::BuildingKind;
use stats::StatsPlugin;
use ui::RtsUiPlugin;
use unit_movement::UnitMovementPlugin;
use world_grid::WorldGridPlugin;

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let mut app = App::new();

    let mut economy = PlayerEconomy::new();
    // Initialize starting supply for Player 1 (2 SCVs @ 1 = 2, 3 Marines @ 2 = 6, 1 Tank @ 3 = 3 -> Total 11 / 20)
    economy.register_supply(Faction::Player1, 11);
    // Initialize starting supply for Hostile AI (2 Marines @ 2 = 4 -> Total 4 / 10)
    economy.register_supply(Faction::HostileAi, 4);

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Mini-RTS (Bevy 0.15)".to_string(),
                    canvas: Some("#bevy-canvas".to_string()),
                    resolution: WindowResolution::new(1280.0, 720.0),
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: false,
                    ..default()
                }),
                ..default()
            })
            .set(ImagePlugin::default_nearest()),
    )
    .insert_resource(ClearColor(Color::srgb(0.04, 0.06, 0.08)))
    .insert_resource(economy)
    .add_plugins((
        WorldGridPlugin,
        RtsCameraPlugin,
        SelectionPlugin,
        CommandMarkerPlugin,
        UnitMovementPlugin,
        RenderUnitsPlugin,
        MiningPlugin,
        ProductionPlugin,
    ))
    .add_plugins((
        PlacementPlugin,
        CombatPlugin,
        AudioSfxPlugin,
        ParticlesPlugin,
        StatsPlugin,
        MinimapPlugin,
        FogOfWarPlugin,
        RtsUiPlugin,
        ChatPlugin,
        TacticalPingPlugin,
        NetClientPlugin,
        bot_ai::WaveAiPlugin,
    ));


    // Enable Bevy Remote Protocol (BRP) for live MCP debugging on native desktop
    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(bevy::remote::RemotePlugin::default());

    app.add_systems(Startup, setup_demo_scene);

    app.run();
}

/// Spawns the camera and initial playable units, buildings, and mineral nodes
fn setup_demo_scene(mut commands: Commands) {
    // 1. RTS 2D Camera centered on Player 1 Base
    commands.spawn((
        Camera2d,
        RtsCamera::default(),
        Transform::from_xyz(-600.0, 250.0, 0.0),
    ));

    // ─────────────────────────────────────────────────────────────────────────
    // PLAYER 1 BASE & STARTING UNITS
    // ─────────────────────────────────────────────────────────────────────────
    let p1_base_pos = Vec2::new(-700.0, 250.0);

    // Player 1 Base HQ
    commands.spawn((
        Building::new(
            BuildingKind::BaseHQ.name(),
            BuildingKind::BaseHQ.size(),
            BuildingKind::BaseHQ.build_duration(),
            true,
        ),
        BaseHQ {
            supply_provided: 20,
            dropoff_radius: 70.0,
        },
        ProductionBuilding {
            queue: Vec::new(),
            current_timer: 0.0,
            max_queue_size: 5,
            rally_point: p1_base_pos + Vec2::new(0.0, -100.0),
        },
        Health::new(BuildingKind::BaseHQ.max_health()),
        Faction::Player1,
        Selectable::default(),
        Radius(55.0),
        Transform::from_xyz(p1_base_pos.x, p1_base_pos.y, 1.0),
    ));

    // Player 1 Gun Turret
    let turret_pos = p1_base_pos + Vec2::new(0.0, 110.0);
    commands.spawn((
        Building::new(
            BuildingKind::Turret.name(),
            BuildingKind::Turret.size(),
            BuildingKind::Turret.build_duration(),
            true,
        ),
        GunTurret::default(),
        Health::new(BuildingKind::Turret.max_health()),
        Faction::Player1,
        Selectable::default(),
        Radius(28.0),
        Transform::from_xyz(turret_pos.x, turret_pos.y, 1.0),
    ));

    // Player 1 Starting Mineral Field
    let mineral_pos = p1_base_pos + Vec2::new(180.0, -40.0);
    let mineral_e = commands.spawn((
        ResourceNode::new(1500),
        Radius(32.0),
        Selectable::default(),
        Transform::from_xyz(mineral_pos.x, mineral_pos.y, 1.0),
    )).id();

    // Player 1 SCV Workers (Automatically harvesting starting mineral patch)
    let worker_offsets = [Vec2::new(-60.0, -80.0), Vec2::new(60.0, -80.0)];
    for (i, offset) in worker_offsets.iter().enumerate() {
        let pos = p1_base_pos + *offset;
        commands.spawn((
            Unit {
                name: "SCV Worker".to_string(),
                supply_cost: 1,
            },
            Worker {
                state: WorkerState::MovingToResource,
                target_node: Some(mineral_e),
                ..default()
            },
            TacticalStance::default(),
            Health::new(80.0),
            Radius(14.0),
            MoveSpeed(190.0),
            Velocity::default(),
            Faction::Player1,
            Selectable::default(),
            NetEntity {
                net_id: 100 + i as u32,
                owner_peer_id: 1,
            },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));
    }

    // Player 1 Marine Soldiers
    let soldier_offsets = [
        Vec2::new(-120.0, 0.0),
        Vec2::new(-120.0, 40.0),
        Vec2::new(-120.0, -40.0),
    ];
    for (i, offset) in soldier_offsets.iter().enumerate() {
        let pos = p1_base_pos + *offset;
        commands.spawn((
            Unit {
                name: "Marine Soldier".to_string(),
                supply_cost: 2,
            },
            Soldier {
                state: SoldierState::Idle,
                attack_range: 150.0,
                aggro_radius: 240.0,
                attack_damage: 15.0,
                attack_cooldown: 0.85,
                ..default()
            },
            Stimpack::default(),
            TacticalStance::default(),
            Health::new(120.0),
            Radius(16.0),
            MoveSpeed(180.0),
            Velocity::default(),
            Faction::Player1,
            Selectable::default(),
            NetEntity {
                net_id: 200 + i as u32,
                owner_peer_id: 1,
            },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));
    }

    // Player 1 Siege Tank
    let tank_pos = p1_base_pos + Vec2::new(-160.0, 80.0);
    commands.spawn((
        Unit {
            name: "Siege Tank".to_string(),
            supply_cost: 3,
        },
        SiegeTank::default(),
        TacticalStance::default(),
        Health::new(220.0),
        Radius(22.0),
        MoveSpeed(140.0),
        Velocity::default(),
        Faction::Player1,
        Selectable::default(),
        NetEntity {
            net_id: 250,
            owner_peer_id: 1,
        },
        Transform::from_xyz(tank_pos.x, tank_pos.y, 2.0),
    ));


    // ─────────────────────────────────────────────────────────────────────────
    // HOSTILE AI BASE & DEFENDERS
    // ─────────────────────────────────────────────────────────────────────────
    let ai_base_pos = Vec2::new(700.0, -250.0);

    // AI Base HQ
    commands.spawn((
        Building::new(
            BuildingKind::BaseHQ.name(),
            BuildingKind::BaseHQ.size(),
            BuildingKind::BaseHQ.build_duration(),
            true,
        ),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
        Health::new(BuildingKind::BaseHQ.max_health()),
        Faction::HostileAi,
        Selectable::default(),
        Radius(55.0),
        Transform::from_xyz(ai_base_pos.x, ai_base_pos.y, 1.0),
    ));

    // AI Patrol Soldiers
    let ai_soldier_offsets = [Vec2::new(-80.0, 0.0), Vec2::new(-80.0, -40.0)];
    for (i, offset) in ai_soldier_offsets.iter().enumerate() {
        let pos = ai_base_pos + *offset;
        commands.spawn((
            Unit {
                name: "Hostile Marine".to_string(),
                supply_cost: 2,
            },
            Soldier {
                state: SoldierState::Idle,
                attack_range: 150.0,
                aggro_radius: 240.0,
                attack_damage: 14.0,
                attack_cooldown: 0.9,
                ..default()
            },
            Stimpack::default(),
            TacticalStance::default(),
            Health::new(120.0),
            Radius(16.0),
            MoveSpeed(175.0),
            Velocity::default(),
            Faction::HostileAi,
            Selectable::default(),
            NetEntity {
                net_id: 300 + i as u32,
                owner_peer_id: 2,
            },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));
    }

    // AI Starting Mineral Field
    let ai_mineral_pos = ai_base_pos + Vec2::new(-180.0, 40.0);
    commands.spawn((
        ResourceNode::new(1500),
        Radius(32.0),
        Selectable::default(),
        Transform::from_xyz(ai_mineral_pos.x, ai_mineral_pos.y, 1.0),
    ));
}
