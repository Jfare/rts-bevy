use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use crate::components::Faction;
use crate::grid::BuildingKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum UnitKind {
    Worker,
    Soldier,
}

impl UnitKind {
    pub fn name(&self) -> &'static str {
        match self {
            UnitKind::Worker => "SCV Worker",
            UnitKind::Soldier => "Marine Soldier",
        }
    }

    pub fn mineral_cost(&self) -> u32 {
        match self {
            UnitKind::Worker => 50,
            UnitKind::Soldier => 100,
        }
    }

    pub fn supply_cost(&self) -> u32 {
        match self {
            UnitKind::Worker => 1,
            UnitKind::Soldier => 2,
        }
    }

    pub fn train_duration(&self) -> f32 {
        match self {
            UnitKind::Worker => 3.0,
            UnitKind::Soldier => 4.0,
        }
    }
}

/// Nätverksmeddelanden som skickas från Klient till Server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
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
        resource_position: Vec2,
    },
    RequestStop {
        unit_net_ids: Vec<u32>,
    },
    RequestHoldPosition {
        unit_net_ids: Vec<u32>,
    },
}

/// 30 Hz position- och hälsosnapshot för en entitet
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub net_id: u32,
    pub position: Vec2,
    pub rotation: f32,
    pub current_hp: f32,
    pub max_hp: f32,
}

/// Nätverksmeddelanden som skickas från Server till Klient
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    BuildingSpawned {
        net_id: u32,
        faction: Faction,
        building_kind: BuildingKind,
        position: Vec2,
    },
    UnitSpawned {
        net_id: u32,
        faction: Faction,
        unit_kind: UnitKind,
        position: Vec2,
        rally_position: Vec2,
    },
    QueueUpdated {
        building_net_id: u32,
        queue_count: usize,
    },
    EconomySync {
        faction: Faction,
        minerals: u32,
        current_supply: u32,
        max_supply: u32,
    },
    EntitySnapshotBatch {
        tick: u32,
        snapshots: Vec<EntitySnapshot>,
    },
    ProjectileFired {
        attacker_net_id: u32,
        target_net_id: u32,
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
    MatchEnded {
        winning_faction: Faction,
        duration_seconds: f32,
    },
}
