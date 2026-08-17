use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Spelkartans dimensioner och rutnätsinställningar
#[derive(Debug, Clone, Resource, Reflect)]
pub struct WorldGridConfig {
    pub min_bounds: Vec2,
    pub max_bounds: Vec2,
    pub cell_size: f32,
    pub major_interval: u32,
    pub snap_size: f32,
}

impl Default for WorldGridConfig {
    fn default() -> Self {
        Self {
            min_bounds: Vec2::new(-1600.0, -1600.0),
            max_bounds: Vec2::new(1600.0, 1600.0),
            cell_size: 64.0,
            major_interval: 4,
            snap_size: 16.0,
        }
    }
}

impl WorldGridConfig {
    pub fn is_inside(&self, pos: Vec2, padding: f32) -> bool {
        pos.x >= (self.min_bounds.x + padding)
            && pos.x <= (self.max_bounds.x - padding)
            && pos.y >= (self.min_bounds.y + padding)
            && pos.y <= (self.max_bounds.y - padding)
    }

    pub fn snap_position(&self, pos: Vec2) -> Vec2 {
        let snap = self.snap_size;
        Vec2::new(
            (pos.x / snap).round() * snap,
            (pos.y / snap).round() * snap,
        )
    }

    pub fn width(&self) -> f32 {
        self.max_bounds.x - self.min_bounds.x
    }

    pub fn height(&self) -> f32 {
        self.max_bounds.y - self.min_bounds.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum BuildingKind {
    BaseHQ,
    Barracks,
    SupplyDepot,
    Turret,
}

impl BuildingKind {
    pub fn name(&self) -> &'static str {
        match self {
            BuildingKind::BaseHQ => "Base HQ",
            BuildingKind::Barracks => "Barracks",
            BuildingKind::SupplyDepot => "Supply Depot",
            BuildingKind::Turret => "Gun Turret",
        }
    }

    pub fn mineral_cost(&self) -> u32 {
        match self {
            BuildingKind::BaseHQ => 400,
            BuildingKind::Barracks => 150,
            BuildingKind::SupplyDepot => 100,
            BuildingKind::Turret => 125,
        }
    }

    pub fn size(&self) -> Vec2 {
        match self {
            BuildingKind::BaseHQ => Vec2::new(110.0, 110.0),
            BuildingKind::Barracks => Vec2::new(96.0, 96.0),
            BuildingKind::SupplyDepot => Vec2::new(64.0, 64.0),
            BuildingKind::Turret => Vec2::new(56.0, 56.0),
        }
    }

    pub fn build_duration(&self) -> f32 {
        match self {
            BuildingKind::BaseHQ => 5.0,
            BuildingKind::Barracks => 3.5,
            BuildingKind::SupplyDepot => 2.5,
            BuildingKind::Turret => 3.0,
        }
    }

    pub fn max_health(&self) -> f32 {
        match self {
            BuildingKind::BaseHQ => 1200.0,
            BuildingKind::Barracks => 700.0,
            BuildingKind::SupplyDepot => 500.0,
            BuildingKind::Turret => 450.0,
        }
    }
}

