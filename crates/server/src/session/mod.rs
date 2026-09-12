pub mod spawner;

use bevy::prelude::*;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::protocol::{ClientPlatform, FactionColor, GameMode};
use std::collections::{HashMap, HashSet};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PlayerSession {
    pub peer_id: u64,
    pub name: String,
    pub room_id: u32,
    pub faction: Faction,
    pub color: FactionColor,
    pub platform: ClientPlatform,
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
    pub economy: PlayerEconomy,
}

impl Room {
    pub fn new(
        room_id: u32,
        room_code: Option<String>,
        mode: GameMode,
        p1_peer: Option<u64>,
        p2_peer: Option<u64>,
    ) -> Self {
        let mut economy = PlayerEconomy::new();
        // 2 starting workers for Player 1 = 2 supply
        economy.register_supply(Faction::Player1, 2);
        let p2_faction = match mode {
            GameMode::SoloVsAi => Faction::HostileAi,
            GameMode::Multiplayer1v1 | GameMode::CustomPrivate => Faction::Player2,
        };
        // 2 starting workers for Player 2 / AI = 2 supply
        economy.register_supply(p2_faction, 2);

        Self {
            room_id,
            room_code,
            mode,
            p1_peer,
            p2_peer,
            is_active: true,
            match_time: 0.0,
            countdown_timer: 0.0,
            current_wave: 0,
            time_until_next_wave: 40.0,
            economy,
        }
    }
}

pub const MAX_ACTIVE_PVP_MATCHES: usize = 10;
pub const MAX_ACTIVE_SOLO_MATCHES: usize = 10;

#[derive(Resource, Default)]
pub struct Matchmaker {
    pub connected_peers: HashSet<u64>,
    pub players: HashMap<u64, PlayerSession>,
    pub rooms: HashMap<u32, Room>,
    pub waiting_1v1_peer: Option<u64>,
    pub next_room_id: u32,
    pub next_net_id: u32,
}

impl Matchmaker {
    pub fn new() -> Self {
        Self {
            connected_peers: HashSet::new(),
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
        let total_online = self.connected_peers.len().max(self.players.len()) as u32;
        (queue_1v1, active_1v1, max_1v1, active_solo, max_solo, total_online)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::spawner::spawn_match_entities;

    #[test]
    fn test_spawn_match_entities_attaches_room_id() {
        let mut app = App::new();
        let mut matchmaker = Matchmaker::new();

        let states_r1 = spawn_match_entities(
            &mut app.world_mut().commands(),
            &mut matchmaker,
            1,
            GameMode::Multiplayer1v1,
            101,
            Some(102),
        );

        let states_r2 = spawn_match_entities(
            &mut app.world_mut().commands(),
            &mut matchmaker,
            2,
            GameMode::SoloVsAi,
            201,
            None,
        );

        app.update();

        let mut r1_count = 0;
        let mut r2_count = 0;
        let mut query = app.world_mut().query::<&RoomId>();
        for room_id in query.iter(app.world()) {
            if room_id.0 == 1 {
                r1_count += 1;
            } else if room_id.0 == 2 {
                r2_count += 1;
            }
        }

        assert_eq!(r1_count, states_r1.len());
        assert_eq!(r2_count, states_r2.len());
        assert_eq!(r1_count, 16, "Room 1 should spawn 16 entities (HQs, Workers, 1 Golden Rock per base & Expansion Minerals)");
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
                platform: ClientPlatform::Desktop,
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
                platform: ClientPlatform::Mobile,
            },
        );
        matchmaker.rooms.insert(
            1,
            Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102)),
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
                platform: ClientPlatform::Desktop,
            },
        );
        matchmaker.rooms.insert(
            2,
            Room::new(2, None, GameMode::SoloVsAi, Some(201), None),
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
            Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102)),
        );
        matchmaker.players.insert(
            101,
            PlayerSession {
                peer_id: 101,
                name: "P1".to_string(),
                room_id: 1,
                faction: Faction::Player1,
                color: FactionColor::Blue,
                platform: ClientPlatform::Desktop,
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
                platform: ClientPlatform::Mobile,
            },
        );

        matchmaker.rooms.insert(
            2,
            Room::new(2, None, GameMode::SoloVsAi, Some(201), None),
        );
        matchmaker.players.insert(
            201,
            PlayerSession {
                peer_id: 201,
                name: "Solo".to_string(),
                room_id: 2,
                faction: Faction::Player1,
                color: FactionColor::Amber,
                platform: ClientPlatform::Desktop,
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

    #[test]
    fn test_matchmaker_connected_peers_telemetry() {
        let mut matchmaker = Matchmaker::new();
        assert_eq!(matchmaker.get_telemetry(), (0, 0, 10, 0, 10, 0));

        // 3 visitors view the site (connect via websocket)
        matchmaker.connected_peers.insert(10);
        matchmaker.connected_peers.insert(20);
        matchmaker.connected_peers.insert(30);
        assert_eq!(matchmaker.get_telemetry(), (0, 0, 10, 0, 10, 3));

        // One leaves the site
        matchmaker.connected_peers.remove(&20);
        assert_eq!(matchmaker.get_telemetry(), (0, 0, 10, 0, 10, 2));

        // All leave
        matchmaker.connected_peers.clear();
        assert_eq!(matchmaker.get_telemetry(), (0, 0, 10, 0, 10, 0));
    }
}
