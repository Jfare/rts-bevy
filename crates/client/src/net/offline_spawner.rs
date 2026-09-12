use bevy::prelude::*;
use shared::components::*;
use shared::economy::PlayerEconomy;

/// Spawns a local offline game scene for Solo vs AI skirmish when running disconnected from server
#[allow(dead_code)]
pub fn spawn_standalone_offline_match(
    commands: &mut Commands,
    economy: &mut ResMut<PlayerEconomy>,
    mut wave_ai_opt: Option<&mut bot_ai::WaveAiState>,
    camera_query: &mut Query<&mut Transform, (With<Camera2d>, Without<NetEntity>, Without<Unit>, Without<Building>, Without<ResourceNode>)>,
    cleanup_query: &Query<Entity, Or<(With<NetEntity>, With<Unit>, With<Building>, With<ResourceNode>)>>,
) {
    info!("🤖 [Offline] Initializing standalone local Solo vs AI match");
    for ent in cleanup_query.iter() {
        commands.entity(ent).despawn_recursive();
    }

    // Reset economy
    let cur_min = economy.get_minerals(Faction::Player1);
    if cur_min != 200 {
        if cur_min < 200 {
            economy.add_minerals(Faction::Player1, 200 - cur_min);
        } else {
            economy.spend_minerals(Faction::Player1, cur_min - 200);
        }
    }
    economy.set_supply(Faction::Player1, 2, 10);

    // Setup wave AI
    if let Some(ref mut wave_ai) = wave_ai_opt {
        wave_ai.is_active = true;
        wave_ai.current_wave = 0;
        wave_ai.time_until_next_wave = 40.0;
        wave_ai.ai_spawn_pos = shared::map::P2_BASE_POS;
        wave_ai.target_player_pos = shared::map::P1_BASE_POS;
    }

    // Center camera
    for mut cam_tf in camera_query.iter_mut() {
        cam_tf.translation.x = shared::map::P1_BASE_POS.x;
        cam_tf.translation.y = shared::map::P1_BASE_POS.y;
    }

    let p1_pos = shared::map::P1_BASE_POS;

    // Spawn P1 Base HQ
    commands.spawn((
        Building::new("Base HQ", Vec2::new(110.0, 110.0), 5.0, true),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
        ProductionBuilding {
            queue: Vec::new(),
            current_timer: 0.0,
            max_queue_size: 5,
            rally_point: p1_pos + Vec2::new(0.0, 100.0),
        },
        Health::new(1500.0),
        Faction::Player1,
        Selectable::default(),
        Radius(55.0),
        NetEntity { net_id: 1, owner_peer_id: 1 },
        Transform::from_xyz(p1_pos.x, p1_pos.y, 1.0),
    ));

    // Spawn P1 Minerals
    let mut p1_primary_mineral_e = None;
    for (i, &min_pos) in shared::map::P1_MAIN_MINERALS.iter().enumerate() {
        let e = commands.spawn((
            ResourceNode::new(2000),
            Faction::Neutral,
            Selectable::default(),
            Radius(36.0),
            NetEntity { net_id: 2 + i as u32, owner_peer_id: 0 },
            Transform::from_xyz(min_pos.x, min_pos.y, 0.5),
        )).id();
        if p1_primary_mineral_e.is_none() {
            p1_primary_mineral_e = Some(e);
        }
    }

    // Spawn P1 Workers (2 workers auto-harvesting at start)
    for (i, &pos) in shared::map::P1_STARTER_WORKERS.iter().enumerate() {
        commands.spawn((
            Unit { name: "Worker".to_string(), supply_cost: 1 },
            Worker {
                state: WorkerState::MovingToResource,
                target_node: p1_primary_mineral_e,
                ..default()
            },
            Health::new(80.0),
            Faction::Player1,
            Selectable::default(),
            Radius(14.0),
            MoveSpeed(190.0),
            Velocity::default(),
            NetEntity { net_id: 10 + i as u32, owner_peer_id: 1 },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));
    }

    // Spawn Hostile AI Base (North)
    let ai_pos = shared::map::P2_BASE_POS;
    commands.spawn((
        Building::new("Hostile Base HQ", Vec2::new(110.0, 110.0), 5.0, true),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
        Health::new(1500.0),
        Faction::HostileAi,
        Selectable::default(),
        Radius(55.0),
        NetEntity { net_id: 100, owner_peer_id: 2 },
        Transform::from_xyz(ai_pos.x, ai_pos.y, 1.0),
    ));

    // Spawn Hostile AI Minerals
    let mut ai_primary_mineral_e = None;
    for (i, &min_pos) in shared::map::P2_MAIN_MINERALS.iter().enumerate() {
        let e = commands.spawn((
            ResourceNode::new(2000),
            Faction::Neutral,
            Selectable::default(),
            Radius(36.0),
            NetEntity { net_id: 101 + i as u32, owner_peer_id: 0 },
            Transform::from_xyz(min_pos.x, min_pos.y, 0.5),
        )).id();
        if ai_primary_mineral_e.is_none() {
            ai_primary_mineral_e = Some(e);
        }
    }

    // Spawn Hostile AI Workers (2 workers auto-harvesting at start)
    for (i, &pos) in shared::map::P2_STARTER_WORKERS.iter().enumerate() {
        commands.spawn((
            Unit { name: "Worker".to_string(), supply_cost: 1 },
            Worker {
                state: WorkerState::MovingToResource,
                target_node: ai_primary_mineral_e,
                ..default()
            },
            Health::new(80.0),
            Faction::HostileAi,
            Selectable::default(),
            Radius(14.0),
            MoveSpeed(190.0),
            Velocity::default(),
            NetEntity { net_id: 110 + i as u32, owner_peer_id: 2 },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));
    }

    // Spawn Expansion Minerals
    let all_expansions = [
        &shared::map::P1_NATURAL_EXPANSION_MINERALS[..],
        &shared::map::P2_NATURAL_EXPANSION_MINERALS[..],
        &shared::map::CONTESTED_WEST_MINERALS[..],
        &shared::map::CONTESTED_EAST_MINERALS[..],
    ];

    let mut net_id_counter = 200;
    for exp_cluster in all_expansions {
        for &exp_pos in exp_cluster {
            commands.spawn((
                ResourceNode::new(2000),
                Faction::Neutral,
                Selectable::default(),
                Radius(36.0),
                NetEntity { net_id: net_id_counter, owner_peer_id: 0 },
                Transform::from_xyz(exp_pos.x, exp_pos.y, 0.5),
            ));
            net_id_counter += 1;
        }
    }
}

