use bevy::prelude::*;
use shared::components::*;
use shared::grid::BuildingKind;
use shared::protocol::{EntityKind, EntityState, GameMode, UnitKind};

use crate::session::Matchmaker;

/// Spawns the standard initial RTS base layout for Player 1 and Player 2 (or AI)
pub fn spawn_match_entities(
    commands: &mut Commands,
    matchmaker: &mut Matchmaker,
    room_id: u32,
    mode: GameMode,
    p1_peer: u64,
    p2_peer: Option<u64>,
) -> Vec<EntityState> {
    let mut initial_states = Vec::new();

    // ─────────────────────────────────────────────────────────────────────────
    // PLAYER 1 BASE (South: 0, -1000)
    // ─────────────────────────────────────────────────────────────────────────
    let p1_base_pos = shared::map::P1_BASE_POS;
    let p1_hq_id = matchmaker.alloc_net_id();

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
        ProductionBuilding {
            queue: Vec::new(),
            current_timer: 0.0,
            max_queue_size: 5,
            rally_point: p1_base_pos + Vec2::new(0.0, 100.0),
        },
        Health::new(BuildingKind::BaseHQ.max_health()),
        Faction::Player1,
        Radius(55.0),
        RoomId(room_id),
        NetEntity {
            net_id: p1_hq_id,
            owner_peer_id: p1_peer,
        },
        Transform::from_xyz(p1_base_pos.x, p1_base_pos.y, 1.0),
    ));

    initial_states.push(EntityState {
        net_id: p1_hq_id,
        kind: EntityKind::Building(BuildingKind::BaseHQ),
        faction: Faction::Player1,
        position: p1_base_pos,
        rotation: 0.0,
        current_hp: BuildingKind::BaseHQ.max_health(),
        max_hp: BuildingKind::BaseHQ.max_health(),
    });

    // P1 Starting Mineral Field (3 nodes)
    let mut p1_primary_mineral_e = None;
    for &min_pos in shared::map::P1_MAIN_MINERALS.iter() {
        let min_id = matchmaker.alloc_net_id();
        let e = commands.spawn((
            ResourceNode::new(1500),
            Radius(32.0),
            RoomId(room_id),
            NetEntity {
                net_id: min_id,
                owner_peer_id: 0,
            },
            Transform::from_xyz(min_pos.x, min_pos.y, 1.0),
        )).id();

        if p1_primary_mineral_e.is_none() {
            p1_primary_mineral_e = Some(e);
        }

        initial_states.push(EntityState {
            net_id: min_id,
            kind: EntityKind::ResourceNode,
            faction: Faction::Neutral,
            position: min_pos,
            rotation: 0.0,
            current_hp: 1500.0,
            max_hp: 1500.0,
        });
    }

    // P1 Starting SCVs (2 workers auto-harvesting at start)
    for &pos in shared::map::P1_STARTER_WORKERS.iter() {
        let scv_id = matchmaker.alloc_net_id();

        commands.spawn((
            Unit {
                name: "SCV Worker".to_string(),
                supply_cost: 1,
            },
            Worker {
                state: WorkerState::MovingToResource,
                target_node: p1_primary_mineral_e,
                ..default()
            },
            Health::new(80.0),
            Radius(14.0),
            MoveSpeed(190.0),
            Velocity::default(),
            Faction::Player1,
            RoomId(room_id),
            NetEntity {
                net_id: scv_id,
                owner_peer_id: p1_peer,
            },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));

        initial_states.push(EntityState {
            net_id: scv_id,
            kind: EntityKind::Unit(UnitKind::Worker),
            faction: Faction::Player1,
            position: pos,
            rotation: 0.0,
            current_hp: 80.0,
            max_hp: 80.0,
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    // PLAYER 2 / HOSTILE AI BASE (North: 0, 1000)
    // ─────────────────────────────────────────────────────────────────────────
    let p2_base_pos = shared::map::P2_BASE_POS;
    let p2_faction = match mode {
        GameMode::SoloVsAi => Faction::HostileAi,
        GameMode::Multiplayer1v1 | GameMode::CustomPrivate => Faction::Player2,
    };
    let p2_owner = p2_peer.unwrap_or(2);

    let p2_hq_id = matchmaker.alloc_net_id();
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
        ProductionBuilding {
            queue: Vec::new(),
            current_timer: 0.0,
            max_queue_size: 5,
            rally_point: p2_base_pos + Vec2::new(0.0, -100.0),
        },
        Health::new(BuildingKind::BaseHQ.max_health()),
        p2_faction,
        Radius(55.0),
        RoomId(room_id),
        NetEntity {
            net_id: p2_hq_id,
            owner_peer_id: p2_owner,
        },
        Transform::from_xyz(p2_base_pos.x, p2_base_pos.y, 1.0),
    ));

    initial_states.push(EntityState {
        net_id: p2_hq_id,
        kind: EntityKind::Building(BuildingKind::BaseHQ),
        faction: p2_faction,
        position: p2_base_pos,
        rotation: 0.0,
        current_hp: BuildingKind::BaseHQ.max_health(),
        max_hp: BuildingKind::BaseHQ.max_health(),
    });

    // P2 / AI Starting Mineral Field (3 nodes)
    let mut p2_primary_mineral_e = None;
    for &min_pos in shared::map::P2_MAIN_MINERALS.iter() {
        let min_id = matchmaker.alloc_net_id();
        let e = commands.spawn((
            ResourceNode::new(1500),
            Radius(32.0),
            RoomId(room_id),
            NetEntity {
                net_id: min_id,
                owner_peer_id: 0,
            },
            Transform::from_xyz(min_pos.x, min_pos.y, 1.0),
        )).id();

        if p2_primary_mineral_e.is_none() {
            p2_primary_mineral_e = Some(e);
        }

        initial_states.push(EntityState {
            net_id: min_id,
            kind: EntityKind::ResourceNode,
            faction: Faction::Neutral,
            position: min_pos,
            rotation: 0.0,
            current_hp: 1500.0,
            max_hp: 1500.0,
        });
    }

    // P2 Starting SCVs (2 workers auto-harvesting at start)
    for &pos in shared::map::P2_STARTER_WORKERS.iter() {
        let scv_id = matchmaker.alloc_net_id();

        commands.spawn((
            Unit {
                name: "SCV Worker".to_string(),
                supply_cost: 1,
            },
            Worker {
                state: WorkerState::MovingToResource,
                target_node: p2_primary_mineral_e,
                ..default()
            },
            Health::new(80.0),
            Radius(14.0),
            MoveSpeed(190.0),
            Velocity::default(),
            p2_faction,
            RoomId(room_id),
            NetEntity {
                net_id: scv_id,
                owner_peer_id: p2_owner,
            },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));

        initial_states.push(EntityState {
            net_id: scv_id,
            kind: EntityKind::Unit(UnitKind::Worker),
            faction: p2_faction,
            position: pos,
            rotation: 0.0,
            current_hp: 80.0,
            max_hp: 80.0,
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    // EXPANSION MINERAL FIELDS (Natural Expansions & Contested Thirds)
    // ─────────────────────────────────────────────────────────────────────────
    let all_expansions = [
        &shared::map::P1_NATURAL_EXPANSION_MINERALS[..],
        &shared::map::P2_NATURAL_EXPANSION_MINERALS[..],
        &shared::map::CONTESTED_WEST_MINERALS[..],
        &shared::map::CONTESTED_EAST_MINERALS[..],
    ];

    for exp_cluster in all_expansions {
        for &exp_pos in exp_cluster {
            let exp_id = matchmaker.alloc_net_id();
            commands.spawn((
                ResourceNode::new(1500),
                Radius(32.0),
                RoomId(room_id),
                NetEntity {
                    net_id: exp_id,
                    owner_peer_id: 0,
                },
                Transform::from_xyz(exp_pos.x, exp_pos.y, 1.0),
            ));

            initial_states.push(EntityState {
                net_id: exp_id,
                kind: EntityKind::ResourceNode,
                faction: Faction::Neutral,
                position: exp_pos,
                rotation: 0.0,
                current_hp: 1500.0,
                max_hp: 1500.0,
            });
        }
    }

    initial_states
}
