use bevy::prelude::*;
use shared::components::*;
use shared::grid::BuildingKind;
use shared::protocol::{EntityKind, EntityState, FactionColor, GameMode, UnitKind};
use std::collections::HashMap;


#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PlayerSession {
    pub peer_id: u64,
    pub name: String,
    pub room_id: u32,
    pub faction: Faction,
    pub color: FactionColor,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Room {
    pub room_id: u32,
    pub room_code: Option<String>,
    pub mode: GameMode,
    pub p1_peer: Option<u64>,
    pub p2_peer: Option<u64>,
    pub is_active: bool,
    pub match_time: f32,
    pub countdown_timer: f32,
    pub current_wave: u32,
    pub time_until_next_wave: f32,
}


pub const MAX_ACTIVE_PVP_MATCHES: usize = 10;
pub const MAX_ACTIVE_SOLO_MATCHES: usize = 10;

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

    pub fn cancel_queue(&mut self, peer_id: u64) -> bool {
        if self.waiting_1v1_peer == Some(peer_id) {
            self.waiting_1v1_peer = None;
            self.players.remove(&peer_id);
            true
        } else {
            false
        }
    }

    pub fn alloc_net_id(&mut self) -> u32 {
        let id = self.next_net_id;
        self.next_net_id += 1;
        id
    }

    pub fn generate_room_code(&self) -> String {
        let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let mut code = String::with_capacity(4);
        let seed = (self.next_room_id * 1103515245 + 12345) as usize;
        for i in 0..4 {
            let idx = (seed >> (i * 4)) % chars.len();
            code.push(chars[idx] as char);
        }
        if self.rooms.values().any(|r| r.room_code.as_deref() == Some(&code)) {
            format!("{:04X}", (self.next_room_id * 7919) % 65535)
        } else {
            code
        }
    }

    pub fn find_room_by_code(&self, code: &str) -> Option<u32> {
        let upper = code.trim().to_uppercase();
        self.rooms.iter().find_map(|(id, r)| {
            if r.room_code.as_deref() == Some(&upper) && r.p2_peer.is_none() && r.mode == GameMode::CustomPrivate {
                Some(*id)
            } else {
                None
            }
        })
    }

    pub fn active_1v1_count(&self) -> usize {
        self.rooms
            .values()
            .filter(|r| (r.mode == GameMode::Multiplayer1v1 || r.mode == GameMode::CustomPrivate) && r.is_active)
            .count()
    }

    pub fn active_solo_count(&self) -> usize {
        self.rooms
            .values()
            .filter(|r| r.mode == GameMode::SoloVsAi && r.is_active)
            .count()
    }

    pub fn can_start_pvp(&self) -> bool {
        self.active_1v1_count() < MAX_ACTIVE_PVP_MATCHES
    }

    pub fn can_start_solo(&self) -> bool {
        self.active_solo_count() < MAX_ACTIVE_SOLO_MATCHES
    }

    pub fn get_room_peers(&self, room_id: u32) -> Vec<u64> {
        if let Some(room) = self.rooms.get(&room_id) {
            let mut peers = Vec::with_capacity(2);
            if let Some(p1) = room.p1_peer {
                peers.push(p1);
            }
            if let Some(p2) = room.p2_peer {
                peers.push(p2);
            }
            peers
        } else {
            Vec::new()
        }
    }

    #[allow(dead_code)]
    pub fn get_peer_room(&self, peer_id: u64) -> Option<u32> {
        self.players.get(&peer_id).map(|p| p.room_id)
    }

    #[allow(dead_code)]
    pub fn get_peer_faction(&self, peer_id: u64) -> Option<Faction> {
        self.players.get(&peer_id).map(|p| p.faction)
    }

    #[allow(dead_code)]
    pub fn deactivate_room(&mut self, room_id: u32) {
        if let Some(room) = self.rooms.get_mut(&room_id) {
            room.is_active = false;
        }
    }

    pub fn remove_room(&mut self, room_id: u32) -> Option<Room> {
        self.rooms.remove(&room_id)
    }

    /// Returns (queue_1v1, active_1v1, max_1v1, active_solo, max_solo, total_online)
    pub fn get_telemetry(&self) -> (u32, u32, u32, u32, u32, u32) {
        let queue_1v1 = if self.waiting_1v1_peer.is_some() { 1 } else { 0 };
        let active_1v1 = self.active_1v1_count() as u32;
        let max_1v1 = MAX_ACTIVE_PVP_MATCHES as u32;
        let active_solo = self.active_solo_count() as u32;
        let max_solo = MAX_ACTIVE_SOLO_MATCHES as u32;
        let total_online = self.players.len() as u32;
        (queue_1v1, active_1v1, max_1v1, active_solo, max_solo, total_online)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_match_entities_attaches_room_id() {
        let mut app = App::new();
        let mut matchmaker = Matchmaker::new();

        // Spawn entities for Room 1 (1v1) and Room 2 (Solo)
        let world = app.world_mut();
        let mut commands = world.commands();

        let states_r1 = spawn_match_entities(
            &mut commands,
            &mut matchmaker,
            1,
            GameMode::Multiplayer1v1,
            101,
            Some(102),
        );
        let states_r2 = spawn_match_entities(
            &mut commands,
            &mut matchmaker,
            2,
            GameMode::SoloVsAi,
            201,
            None,
        );

        // Apply commands to the world
        world.flush();

        assert!(!states_r1.is_empty(), "Room 1 should have spawned states");
        assert!(!states_r2.is_empty(), "Room 2 should have spawned states");

        let mut r1_count = 0;
        let mut r2_count = 0;
        let mut total_entities = 0;

        let mut query = world.query::<(&NetEntity, &RoomId)>();
        for (_net, room_id) in query.iter(world) {
            total_entities += 1;
            if room_id.0 == 1 {
                r1_count += 1;
            } else if room_id.0 == 2 {
                r2_count += 1;
            }
        }

        assert_eq!(total_entities, states_r1.len() + states_r2.len());
        assert_eq!(r1_count, states_r1.len());
        assert_eq!(r2_count, states_r2.len());
        assert_eq!(r1_count, 20, "Room 1 should spawn 20 entities (HQs, SCVs, Main & Expansion Minerals)");
    }

    #[test]
    fn test_matchmaker_room_peers_and_lookups() {
        let mut matchmaker = Matchmaker::new();

        // Set up Room 1 (1v1: peers 101 and 102)
        matchmaker.players.insert(
            101,
            PlayerSession {
                peer_id: 101,
                name: "Player 1".to_string(),
                room_id: 1,
                faction: Faction::Player1,
                color: FactionColor::Blue,
            },
        );
        matchmaker.players.insert(
            102,
            PlayerSession {
                peer_id: 102,
                name: "Player 2".to_string(),
                room_id: 1,
                faction: Faction::Player2,
                color: FactionColor::Red,
            },
        );
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::Multiplayer1v1,
                p1_peer: Some(101),
                p2_peer: Some(102),
                is_active: true,
                match_time: 0.0,
                countdown_timer: 0.0,
                current_wave: 0,
                time_until_next_wave: 40.0,
            },
        );

        // Set up Room 2 (Solo: peer 201)
        matchmaker.players.insert(
            201,
            PlayerSession {
                peer_id: 201,
                name: "Solo Commander".to_string(),
                room_id: 2,
                faction: Faction::Player1,
                color: FactionColor::Teal,
            },
        );
        matchmaker.rooms.insert(
            2,
            Room {
                room_id: 2,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(201),
                p2_peer: None,
                is_active: true,
                match_time: 0.0,
                countdown_timer: 0.0,
                current_wave: 0,
                time_until_next_wave: 40.0,
            },
        );

        // Verify lookups
        assert_eq!(matchmaker.get_room_peers(1), vec![101, 102]);
        assert_eq!(matchmaker.get_room_peers(2), vec![201]);
        assert_eq!(matchmaker.get_room_peers(999), Vec::<u64>::new());

        assert_eq!(matchmaker.get_peer_room(101), Some(1));
        assert_eq!(matchmaker.get_peer_room(102), Some(1));
        assert_eq!(matchmaker.get_peer_room(201), Some(2));
        assert_eq!(matchmaker.get_peer_room(999), None);

        assert_eq!(matchmaker.get_peer_faction(101), Some(Faction::Player1));
        assert_eq!(matchmaker.get_peer_faction(102), Some(Faction::Player2));
    }

    #[test]
    fn test_matchmaker_lifecycle_and_telemetry() {
        let mut matchmaker = Matchmaker::new();

        // Initial empty state
        assert_eq!(matchmaker.get_telemetry(), (0, 0, 10, 0, 10, 0));

        // Add 1v1 match and Solo match
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::Multiplayer1v1,
                p1_peer: Some(101),
                p2_peer: Some(102),
                is_active: true,
                match_time: 0.0,
                countdown_timer: 0.0,
                current_wave: 0,
                time_until_next_wave: 40.0,
            },
        );
        matchmaker.players.insert(
            101,
            PlayerSession {
                peer_id: 101,
                name: "P1".to_string(),
                room_id: 1,
                faction: Faction::Player1,
                color: FactionColor::Blue,
            },
        );
        matchmaker.players.insert(
            102,
            PlayerSession {
                peer_id: 102,
                name: "P2".to_string(),
                room_id: 1,
                faction: Faction::Player2,
                color: FactionColor::Red,
            },
        );

        matchmaker.rooms.insert(
            2,
            Room {
                room_id: 2,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(201),
                p2_peer: None,
                is_active: true,
                match_time: 0.0,
                countdown_timer: 0.0,
                current_wave: 0,
                time_until_next_wave: 40.0,
            },
        );
        matchmaker.players.insert(
            201,
            PlayerSession {
                peer_id: 201,
                name: "Solo".to_string(),
                room_id: 2,
                faction: Faction::Player1,
                color: FactionColor::Amber,
            },
        );

        // Telemetry should reflect: queue=0, active_1v1=1, max_1v1=10, active_solo=1, max_solo=10, total_online=3
        assert_eq!(matchmaker.get_telemetry(), (0, 1, 10, 1, 10, 3));

        // Deactivate Room 1 (e.g. match finished)
        matchmaker.deactivate_room(1);
        assert_eq!(matchmaker.active_1v1_count(), 0);
        assert_eq!(matchmaker.get_telemetry(), (0, 0, 10, 1, 10, 3));

        // Remove Room 1 and its players (e.g. players left)
        matchmaker.remove_room(1);
        matchmaker.players.remove(&101);
        matchmaker.players.remove(&102);
        assert_eq!(matchmaker.get_telemetry(), (0, 0, 10, 1, 10, 1));

        // Remove Room 2
        matchmaker.remove_room(2);
        matchmaker.players.remove(&201);
        assert_eq!(matchmaker.get_telemetry(), (0, 0, 10, 0, 10, 0));
    }
}
