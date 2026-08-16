use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// FACTION & IDENTITY
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Component, Reflect)]
pub enum Faction {
    Player1,
    Player2,
    HostileAi,
    Neutral,
}

impl Faction {
    pub fn is_hostile_to(&self, other: &Faction) -> bool {
        match (self, other) {
            (Faction::Neutral, _) | (_, Faction::Neutral) => false,
            (a, b) => a != b,
        }
    }

    pub fn color_rgba(&self) -> [f32; 4] {
        match self {
            Faction::Player1 => [0.22, 0.58, 0.98, 1.0], // Blue (#38bdf8 / #2563eb)
            Faction::Player2 => [0.20, 0.85, 0.40, 1.0], // Green
            Faction::HostileAi => [0.95, 0.25, 0.25, 1.0], // Crimson Red
            Faction::Neutral => [0.65, 0.65, 0.70, 1.0], // Gray
        }
    }
}

impl Default for Faction {
    fn default() -> Self {
        Faction::Player1
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CORE ECS COMPONENTS
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Component, Reflect)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }

    pub fn take_damage(&mut self, amount: f32) {
        self.current = (self.current - amount).max(0.0);
    }

    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            0.0
        } else {
            (self.current / self.max).clamp(0.0, 1.0)
        }
    }
}

impl Default for Health {
    fn default() -> Self {
        Self::new(100.0)
    }
}

/// Unit stats
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Component, Reflect)]
pub struct Unit {
    pub name: String,
    pub supply_cost: u32,
}

/// Collision and selection radius
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Component, Reflect)]
pub struct Radius(pub f32);

impl Default for Radius {
    fn default() -> Self {
        Self(16.0)
    }
}

/// Speed in pixels per second
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Component, Reflect)]
pub struct MoveSpeed(pub f32);

impl Default for MoveSpeed {
    fn default() -> Self {
        Self(180.0)
    }
}

/// 2D Velocity vector
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Component, Default, Reflect)]
pub struct Velocity(pub Vec2);

/// Move target destination
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Component, Reflect)]
pub struct MoveTarget {
    pub destination: Vec2,
    pub is_attack_move: bool,
}

/// Selection marker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Component, Default, Reflect)]
pub struct Selectable {
    pub is_selected: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// WORKER (SCV) STATS & STATE MACHINE
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Reflect)]
pub enum WorkerState {
    #[default]
    Idle,
    MovingToResource,
    Mining,
    MovingToBase,
}

#[derive(Debug, Clone, PartialEq, Component, Reflect)]
pub struct Worker {
    pub state: WorkerState,
    pub harvest_capacity: u32,
    pub carried_minerals: u32,
    pub harvest_duration: f32,
    pub harvest_timer: f32,
    pub interact_distance: f32,
    pub base_interact_distance: f32,
    pub target_node: Option<Entity>,
    pub target_base: Option<Entity>,
}

impl Default for Worker {
    fn default() -> Self {
        Self {
            state: WorkerState::Idle,
            harvest_capacity: 10,
            carried_minerals: 0,
            harvest_duration: 1.8,
            harvest_timer: 0.0,
            interact_distance: 54.0,
            base_interact_distance: 80.0,
            target_node: None,
            target_base: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SOLDIER (MARINE) STATS & COMBAT STATE MACHINE
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Reflect)]
pub enum SoldierState {
    #[default]
    Idle,
    MovingToGround,
    AttackMoving,
    HoldingPosition,
    Patrolling,
    ChasingTarget,
    Attacking,
}

#[derive(Debug, Clone, PartialEq, Component, Reflect)]
pub struct Soldier {
    pub state: SoldierState,
    pub attack_range: f32,
    pub aggro_radius: f32,
    pub attack_damage: f32,
    pub attack_cooldown: f32,
    pub attack_timer: f32,
    pub recoil_timer: f32,
    pub scan_timer: f32,
    pub target: Option<Entity>,
    pub patrol_point_a: Vec2,
    pub patrol_point_b: Vec2,
    pub patrol_heading_to_b: bool,
}

impl Default for Soldier {
    fn default() -> Self {
        Self {
            state: SoldierState::Idle,
            attack_range: 150.0,
            aggro_radius: 240.0,
            attack_damage: 15.0,
            attack_cooldown: 0.85,
            attack_timer: 0.0,
            recoil_timer: 0.0,
            scan_timer: 0.0,
            target: None,
            patrol_point_a: Vec2::ZERO,
            patrol_point_b: Vec2::ZERO,
            patrol_heading_to_b: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// COMBAT PROJECTILES & VISUALS
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Component, Reflect)]
pub struct Projectile {
    pub origin: Vec2,
    pub target_entity: Option<Entity>,
    pub target_pos: Vec2,
    pub speed: f32,
    pub damage: f32,
    pub faction: Faction,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

#[derive(Debug, Clone, Component, Reflect)]
pub struct MuzzleFlash {
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource, Default, Reflect)]
pub enum MatchOutcome {
    #[default]
    InProgress,
    Victory,
    Defeat,
}

// ─────────────────────────────────────────────────────────────────────────────
// BUILDINGS & PRODUCTION
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Component, Reflect)]
pub struct Building {
    pub name: String,
    pub size: Vec2,
    pub is_constructed: bool,
    pub build_duration: f32,
    pub build_timer: f32,
}

impl Building {
    pub fn new(name: &str, size: Vec2, build_duration: f32, is_constructed: bool) -> Self {
        Self {
            name: name.to_string(),
            size,
            is_constructed,
            build_duration,
            build_timer: if is_constructed { build_duration } else { 0.0 },
        }
    }

    pub fn progress(&self) -> f32 {
        if self.build_duration <= 0.0 {
            1.0
        } else {
            (self.build_timer / self.build_duration).clamp(0.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct QueuedUnit {
    pub name: String,
    pub mineral_cost: u32,
    pub supply_cost: u32,
    pub build_duration: f32,
}

#[derive(Debug, Clone, PartialEq, Component, Reflect)]
pub struct ProductionBuilding {
    pub queue: Vec<QueuedUnit>,
    pub current_timer: f32,
    pub max_queue_size: usize,
    pub rally_point: Vec2,
}

impl Default for ProductionBuilding {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            current_timer: 0.0,
            max_queue_size: 5,
            rally_point: Vec2::new(0.0, 100.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Component, Default, Reflect)]
pub struct BaseHQ {
    pub supply_provided: u32,
    pub dropoff_radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Default, Reflect)]
pub struct Barracks;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Default, Reflect)]
pub struct SupplyDepot {
    pub supply_provided: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Component, Reflect)]
pub struct ResourceNode {
    pub remaining_minerals: u32,
    pub max_minerals: u32,
}

impl ResourceNode {
    pub fn new(amount: u32) -> Self {
        Self {
            remaining_minerals: amount,
            max_minerals: amount,
        }
    }

    pub fn harvest(&mut self, desired: u32) -> u32 {
        let amount = desired.min(self.remaining_minerals);
        self.remaining_minerals -= amount;
        amount
    }
}

impl Default for ResourceNode {
    fn default() -> Self {
        Self::new(1500)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NETWORK REPLICATION ENTITY
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Component, Reflect)]
pub struct NetEntity {
    pub net_id: u32,
    pub owner_peer_id: u64,
}
