use bevy::prelude::*;
use shared::components::*;
use shared::grid::BuildingKind;
use shared::protocol::{EntityKind, EntityState, GameMode, UnitKind};
use std::collections::HashMap;


#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PlayerSession {
    pub peer_id: u64,
    pub name: String,
    pub room_id: u32,
    pub faction: Faction,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Room {
    pub room_id: u32,
    pub mode: GameMode,
    pub p1_peer: Option<u64>,
    pub p2_peer: Option<u64>,
    pub is_active: bool,
    pub match_time: f32,
}


#[derive(Resource, Default)]
pub struct Matchmaker {
    pub players: HashMap<u64, PlayerSession>,
    pub rooms: HashMap<u32, Room>,
    pub waiting_1v1_peer: Option<u64>,
    pub next_room_id: u32,
    pub next_net_id: u32,
}

impl Matchmaker {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            rooms: HashMap::new(),
            waiting_1v1_peer: None,
            next_room_id: 1,
            next_net_id: 1000,
        }
    }

    pub fn alloc_net_id(&mut self) -> u32 {
        let id = self.next_net_id;
        self.next_net_id += 1;
        id
    }
}

/// Spawns the standard initial RTS base layout for Player 1 and Player 2 (or AI)
pub fn spawn_match_entities(
    commands: &mut Commands,
    matchmaker: &mut Matchmaker,
    mode: GameMode,
    p1_peer: u64,
    p2_peer: Option<u64>,
) -> Vec<EntityState> {
    let mut initial_states = Vec::new();

    // ─────────────────────────────────────────────────────────────────────────
    // PLAYER 1 BASE
    // ─────────────────────────────────────────────────────────────────────────
    let p1_base_pos = Vec2::new(-700.0, 250.0);
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
            rally_point: p1_base_pos + Vec2::new(0.0, -100.0),
        },
        Health::new(BuildingKind::BaseHQ.max_health()),
        Faction::Player1,
        Radius(55.0),
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

    // P1 Starting Mineral Field
    let p1_minerals_pos = p1_base_pos + Vec2::new(180.0, -40.0);
    let p1_minerals_id = matchmaker.alloc_net_id();
    let p1_minerals_e = commands.spawn((
        ResourceNode::new(1500),
        Radius(32.0),
        NetEntity {
            net_id: p1_minerals_id,
            owner_peer_id: 0,
        },
        Transform::from_xyz(p1_minerals_pos.x, p1_minerals_pos.y, 1.0),
    )).id();

    initial_states.push(EntityState {
        net_id: p1_minerals_id,
        kind: EntityKind::ResourceNode,
        faction: Faction::Neutral,
        position: p1_minerals_pos,
        rotation: 0.0,
        current_hp: 1500.0,
        max_hp: 1500.0,
    });

    // P1 SCVs
    let worker_offsets = [Vec2::new(-60.0, -80.0), Vec2::new(60.0, -80.0)];
    for offset in worker_offsets {
        let pos = p1_base_pos + offset;
        let scv_id = matchmaker.alloc_net_id();

        commands.spawn((
            Unit {
                name: "SCV Worker".to_string(),
                supply_cost: 1,
            },
            Worker {
                state: WorkerState::MovingToResource,
                target_node: Some(p1_minerals_e),
                ..default()
            },
            Health::new(80.0),
            Radius(14.0),
            MoveSpeed(190.0),
            Velocity::default(),
            Faction::Player1,
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

    // P1 Marines
    let soldier_offsets = [
        Vec2::new(-120.0, 0.0),
        Vec2::new(-120.0, 40.0),
        Vec2::new(-120.0, -40.0),
    ];
    for offset in soldier_offsets {
        let pos = p1_base_pos + offset;
        let marine_id = matchmaker.alloc_net_id();

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
            Health::new(120.0),
            Radius(16.0),
            MoveSpeed(180.0),
            Velocity::default(),
            Faction::Player1,
            NetEntity {
                net_id: marine_id,
                owner_peer_id: p1_peer,
            },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));

        initial_states.push(EntityState {
            net_id: marine_id,
            kind: EntityKind::Unit(UnitKind::Soldier),
            faction: Faction::Player1,
            position: pos,
            rotation: 0.0,
            current_hp: 120.0,
            max_hp: 120.0,
        });
    }


    // ─────────────────────────────────────────────────────────────────────────
    // PLAYER 2 / HOSTILE AI BASE
    // ─────────────────────────────────────────────────────────────────────────
    let p2_base_pos = Vec2::new(700.0, -250.0);
    let p2_faction = match mode {
        GameMode::SoloVsAi => Faction::HostileAi,
        GameMode::Multiplayer1v1 => Faction::Player2,
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
            rally_point: p2_base_pos + Vec2::new(0.0, 100.0),
        },
        Health::new(BuildingKind::BaseHQ.max_health()),
        p2_faction,
        Radius(55.0),
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

    // Starting Defenders / Units for P2 / AI
    let p2_unit_offsets = [Vec2::new(-80.0, 0.0), Vec2::new(-80.0, -40.0)];
    for offset in p2_unit_offsets {
        let pos = p2_base_pos + offset;
        let def_id = matchmaker.alloc_net_id();

        commands.spawn((
            Unit {
                name: if p2_faction == Faction::HostileAi {
                    "Hostile Marine".to_string()
                } else {
                    "Marine Soldier".to_string()
                },
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
            Health::new(120.0),
            Radius(16.0),
            MoveSpeed(175.0),
            Velocity::default(),
            p2_faction,
            NetEntity {
                net_id: def_id,
                owner_peer_id: p2_owner,
            },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));

        initial_states.push(EntityState {
            net_id: def_id,
            kind: EntityKind::Unit(UnitKind::Soldier),
            faction: p2_faction,
            position: pos,
            rotation: 0.0,
            current_hp: 120.0,
            max_hp: 120.0,
        });
    }

    // P2 / AI Starting Mineral Field
    let p2_minerals_pos = p2_base_pos + Vec2::new(-180.0, 40.0);
    let p2_minerals_id = matchmaker.alloc_net_id();
    commands.spawn((
        ResourceNode::new(1500),
        Radius(32.0),
        NetEntity {
            net_id: p2_minerals_id,
            owner_peer_id: 0,
        },
        Transform::from_xyz(p2_minerals_pos.x, p2_minerals_pos.y, 1.0),
    ));

    initial_states.push(EntityState {
        net_id: p2_minerals_id,
        kind: EntityKind::ResourceNode,
        faction: Faction::Neutral,
        position: p2_minerals_pos,
        rotation: 0.0,
        current_hp: 1500.0,
        max_hp: 1500.0,
    });

    initial_states
}
