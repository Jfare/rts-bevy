use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use crate::components::Faction;
use crate::grid::BuildingKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum UnitKind {
    Worker,
    Soldier,
    Tank,
}

impl UnitKind {
    pub fn name(&self) -> &'static str {
        match self {
            UnitKind::Worker => "SCV Worker",
            UnitKind::Soldier => "Marine Soldier",
            UnitKind::Tank => "Siege Tank",
        }
    }

    pub fn mineral_cost(&self) -> u32 {
        match self {
            UnitKind::Worker => 50,
            UnitKind::Soldier => 100,
            UnitKind::Tank => 200,
        }
    }

    pub fn supply_cost(&self) -> u32 {
        match self {
            UnitKind::Worker => 1,
            UnitKind::Soldier => 2,
            UnitKind::Tank => 3,
        }
    }

    pub fn train_duration(&self) -> f32 {
        match self {
            UnitKind::Worker => 3.0,
            UnitKind::Soldier => 4.0,
            UnitKind::Tank => 5.0,
        }
    }

    pub fn max_health(&self) -> f32 {
        match self {
            UnitKind::Worker => 80.0,
            UnitKind::Soldier => 120.0,
            UnitKind::Tank => 220.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect, Default)]
pub enum GameMode {
    #[default]
    SoloVsAi,
    Multiplayer1v1,
    CustomPrivate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect, Default)]
pub enum FactionColor {
    #[default]
    Blue,
    Red,
    Teal,
    Amber,
    Purple,
    Green,
}

impl FactionColor {
    pub fn to_color(&self) -> Color {
        match self {
            FactionColor::Blue => Color::srgb(0.23, 0.51, 0.96),   // #3b82f6
            FactionColor::Red => Color::srgb(0.94, 0.27, 0.27),    // #ef4444
            FactionColor::Teal => Color::srgb(0.08, 0.72, 0.65),   // #14b8a6
            FactionColor::Amber => Color::srgb(0.96, 0.62, 0.04),  // #f59e0b
            FactionColor::Purple => Color::srgb(0.66, 0.33, 0.97), // #a855f7
            FactionColor::Green => Color::srgb(0.13, 0.77, 0.37),  // #22c55e
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            FactionColor::Blue => "Cobalt Blue",
            FactionColor::Red => "Crimson Red",
            FactionColor::Teal => "Cyber Teal",
            FactionColor::Amber => "Solar Amber",
            FactionColor::Purple => "Void Purple",
            FactionColor::Green => "Emerald Green",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect, Default)]
pub enum PingType {
    #[default]
    Attention,
    Attack,
    Defend,
}

impl PingType {
    pub fn name(&self) -> &'static str {
        match self {
            PingType::Attention => "ATTENTION",
            PingType::Attack => "ATTACK HERE",
            PingType::Defend => "DEFEND HERE",
        }
    }

    pub fn to_color(&self) -> Color {
        match self {
            PingType::Attention => Color::srgb(0.25, 0.85, 1.0),
            PingType::Attack => Color::srgb(1.0, 0.25, 0.25),
            PingType::Defend => Color::srgb(0.25, 1.0, 0.45),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum EntityKind {
    Unit(UnitKind),
    Building(BuildingKind),
    ResourceNode,
}

/// Messages sent from Client to Server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    JoinLobby {
        player_name: String,
        mode: GameMode,
        room_code: Option<String>,
        faction_color: Option<FactionColor>,
    },
    RequestBuild {
        building_kind: BuildingKind,
        position: Vec2,
    },
    RequestTrainUnit {
        building_net_id: u32,
        unit_kind: UnitKind,
    },
    RequestSetRallyPoint {
        building_net_id: u32,
        rally_position: Vec2,
    },
    RequestMove {
        unit_net_ids: Vec<u32>,
        target_position: Vec2,
        is_attack_move: bool,
    },
    RequestAttackTarget {
        unit_net_ids: Vec<u32>,
        target_net_id: u32,
    },
    RequestHarvest {
        worker_net_ids: Vec<u32>,
        resource_net_id: u32,
    },
    RequestStop {
        unit_net_ids: Vec<u32>,
    },
    RequestHoldPosition {
        unit_net_ids: Vec<u32>,
    },
    RequestPatrol {
        unit_net_ids: Vec<u32>,
        target_position: Vec2,
    },
    CancelQueue,
    RequestStimpack {
        unit_net_ids: Vec<u32>,
    },
    RequestToggleSiegeMode {
        unit_net_ids: Vec<u32>,
    },
    SendChatMessage {
        text: String,
    },
    SendTacticalPing {
        position: Vec2,
        ping_type: PingType,
    },
    Ping {
        timestamp: u64,
    },
    RequestLobbyStats,
}

/// 30 Hz position, rotation, HP, and visual state for an active entity
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub net_id: u32,
    pub position: Vec2,
    pub rotation: f32,
    pub current_hp: f32,
    pub max_hp: f32,
    pub is_mining: bool,
    pub laser_target: Option<Vec2>,
}

/// Complete initial state of an entity on match start
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub net_id: u32,
    pub kind: EntityKind,
    pub faction: Faction,
    pub position: Vec2,
    pub rotation: f32,
    pub current_hp: f32,
    pub max_hp: f32,
}

/// Messages sent from Server to Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    LobbyJoined {
        player_id: u64,
        assigned_faction: Faction,
        room_id: u32,
        room_code: Option<String>,
        is_game_ready: bool,
    },
    MatchFound {
        opponent_name: String,
        opponent_color: FactionColor,
        countdown_seconds: f32,
    },
    QueueCancelled,
    LobbyStats {
        queue_1v1: u32,
        active_1v1_matches: u32,
        max_1v1_matches: u32,
        active_solo_matches: u32,
        max_solo_matches: u32,
        total_online: u32,
    },
    GameStarted {
        p1_pos: Vec2,
        p2_pos: Vec2,
        wave_initial_delay: f32,
    },
    InitialWorldState {
        entities: Vec<EntityState>,
        p1_minerals: u32,
        p1_supply: u32,
        p1_max_supply: u32,
        p2_minerals: u32,
        p2_supply: u32,
        p2_max_supply: u32,
    },
    TickSnapshotBatch {
        tick: u32,
        snapshots: Vec<EntitySnapshot>,
        p1_minerals: u32,
        p1_supply: u32,
        p1_max_supply: u32,
        p2_minerals: u32,
        p2_supply: u32,
        p2_max_supply: u32,
        next_wave_seconds: f32,
        current_wave: u32,
    },
    BuildingSpawned {
        net_id: u32,
        faction: Faction,
        building_kind: BuildingKind,
        position: Vec2,
        max_hp: f32,
    },
    UnitSpawned {
        net_id: u32,
        faction: Faction,
        unit_kind: UnitKind,
        position: Vec2,
        max_hp: f32,
    },
    QueueUpdated {
        building_net_id: u32,
        queue_count: usize,
        current_progress: f32,
    },
    UnitsOrderedMove {
        unit_net_ids: Vec<u32>,
        destinations: Vec<Vec2>,
        is_attack_move: bool,
    },
    UnitsOrderedAttackTarget {
        unit_net_ids: Vec<u32>,
        target_net_id: u32,
    },
    WorkersOrderedHarvest {
        worker_net_ids: Vec<u32>,
        resource_net_id: u32,
    },
    UnitsOrderedStop {
        unit_net_ids: Vec<u32>,
    },
    UnitsOrderedHoldPosition {
        unit_net_ids: Vec<u32>,
    },
    UnitsOrderedPatrol {
        unit_net_ids: Vec<u32>,
        destinations: Vec<Vec2>,
    },
    UnitsActivatedStimpack {
        unit_net_ids: Vec<u32>,
    },
    UnitsToggledSiegeMode {
        unit_net_ids: Vec<u32>,
    },
    ProjectileFired {
        attacker_net_id: u32,
        target_net_id: u32,
        origin: Vec2,
        target_pos: Vec2,
        damage: f32,
    },
    EntityDamaged {
        target_net_id: u32,
        current_hp: f32,
        max_hp: f32,
    },
    EntityDied {
        net_id: u32,
        faction: Faction,
    },
    ChatMessageReceived {
        sender_name: String,
        faction: Faction,
        color: FactionColor,
        text: String,
        is_system: bool,
    },
    TacticalPingReceived {
        sender_name: String,
        faction: Faction,
        color: FactionColor,
        position: Vec2,
        ping_type: PingType,
    },
    MatchEnded {
        winning_faction: Faction,
        duration_seconds: f32,
    },
    Pong {
        client_timestamp: u64,
        server_time: u64,
    },
    ErrorMessage {
        reason: String,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// BINARY SERIALIZATION HELPERS
// ─────────────────────────────────────────────────────────────────────────────

pub fn encode_client_msg(msg: &ClientMessage) -> Result<Vec<u8>, bincode::Error> {
    bincode::serialize(msg)
}

pub fn decode_client_msg(bytes: &[u8]) -> Result<ClientMessage, bincode::Error> {
    bincode::deserialize(bytes)
}

pub fn encode_server_msg(msg: &ServerMessage) -> Result<Vec<u8>, bincode::Error> {
    bincode::serialize(msg)
}

pub fn decode_server_msg(bytes: &[u8]) -> Result<ServerMessage, bincode::Error> {
    bincode::deserialize(bytes)
}
