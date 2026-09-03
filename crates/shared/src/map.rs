use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use crate::grid::NavGrid;

// ─────────────────────────────────────────────────────────────────────────────
// 1v1 MIRRORED COMPETITIVE MAP: "IRON MERIDIAN"
// Symmetrical Top vs Bottom (180° Rotational Point Symmetry: (x, y) <-> (-x, -y))
// ─────────────────────────────────────────────────────────────────────────────

pub const P1_BASE_POS: Vec2 = Vec2::new(0.0, -1000.0);
pub const P2_BASE_POS: Vec2 = Vec2::new(0.0, 1000.0);

pub const P1_STARTER_WORKERS: [Vec2; 2] = [
    Vec2::new(-45.0, -920.0),
    Vec2::new(45.0, -920.0),
];

pub const P2_STARTER_WORKERS: [Vec2; 2] = [
    Vec2::new(45.0, 920.0),
    Vec2::new(-45.0, 920.0),
];

// Main Mineral Fields (positioned in front of the back cliff wall, with a 100% clear mining line to Base HQ)
pub const P1_MAIN_MINERALS: [Vec2; 3] = [
    Vec2::new(-110.0, -1180.0),
    Vec2::new(0.0, -1200.0),
    Vec2::new(110.0, -1180.0),
];

pub const P2_MAIN_MINERALS: [Vec2; 3] = [
    Vec2::new(110.0, 1180.0),
    Vec2::new(0.0, 1200.0),
    Vec2::new(-110.0, 1180.0),
];

// Natural & Contested Expansion Mineral Fields
pub const P1_NATURAL_EXPANSION_MINERALS: [Vec2; 2] = [
    Vec2::new(880.0, -750.0),
    Vec2::new(960.0, -680.0),
];

pub const P2_NATURAL_EXPANSION_MINERALS: [Vec2; 2] = [
    Vec2::new(-880.0, 750.0),
    Vec2::new(-960.0, 680.0),
];

pub const CONTESTED_WEST_MINERALS: [Vec2; 2] = [
    Vec2::new(-1250.0, -50.0),
    Vec2::new(-1250.0, 50.0),
];

pub const CONTESTED_EAST_MINERALS: [Vec2; 2] = [
    Vec2::new(1250.0, 50.0),
    Vec2::new(1250.0, -50.0),
];

// ─────────────────────────────────────────────────────────────────────────────
// STATIC MAP OBSTACLES (ROCKS, CLIFF BLUFFS, CHOKEPOINTS)
// All obstacles are placed BEHIND minerals or flanking chokepoints, leaving
// 100% clear lines for SCV mining between HQs and mineral nodes.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum ObstacleKind {
    RockMonolith,
    CliffRidge,
    BaseRampBluff,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MapObstacle {
    pub position: Vec2,
    pub radius: f32,
    pub kind: ObstacleKind,
}

pub const STATIC_MAP_OBSTACLES: &[MapObstacle] = &[
    // ── South Base Back Cliff Wall (Placed BEHIND South Minerals at y = -1320..-1340) ──
    MapObstacle { position: Vec2::new(-200.0, -1320.0), radius: 80.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(0.0, -1340.0), radius: 80.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(200.0, -1320.0), radius: 80.0, kind: ObstacleKind::CliffRidge },

    // ── North Base Back Cliff Wall (Placed BEHIND North Minerals at y = 1320..1340) ──
    MapObstacle { position: Vec2::new(200.0, 1320.0), radius: 80.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(0.0, 1340.0), radius: 80.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(-200.0, 1320.0), radius: 80.0, kind: ObstacleKind::CliffRidge },

    // ── South Base Ramp Bluffs (Flanking the South Choke at (0, -720)) ──
    MapObstacle { position: Vec2::new(-240.0, -720.0), radius: 75.0, kind: ObstacleKind::BaseRampBluff },
    MapObstacle { position: Vec2::new(-360.0, -750.0), radius: 80.0, kind: ObstacleKind::BaseRampBluff },
    MapObstacle { position: Vec2::new(240.0, -720.0), radius: 75.0, kind: ObstacleKind::BaseRampBluff },
    MapObstacle { position: Vec2::new(360.0, -750.0), radius: 80.0, kind: ObstacleKind::BaseRampBluff },

    // ── North Base Ramp Bluffs (Flanking the North Choke at (0, 720)) ──
    MapObstacle { position: Vec2::new(240.0, 720.0), radius: 75.0, kind: ObstacleKind::BaseRampBluff },
    MapObstacle { position: Vec2::new(360.0, 750.0), radius: 80.0, kind: ObstacleKind::BaseRampBluff },
    MapObstacle { position: Vec2::new(-240.0, 720.0), radius: 75.0, kind: ObstacleKind::BaseRampBluff },
    MapObstacle { position: Vec2::new(-360.0, 750.0), radius: 80.0, kind: ObstacleKind::BaseRampBluff },

    // ── Natural Expansion Back Cliff Walls (Placed BEHIND expansion minerals) ──
    MapObstacle { position: Vec2::new(1080.0, -780.0), radius: 75.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(1120.0, -650.0), radius: 75.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(-1080.0, 780.0), radius: 75.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(-1120.0, 650.0), radius: 75.0, kind: ObstacleKind::CliffRidge },

    // ── Contested Expansion Back Cliff Walls (Against map outer borders) ──
    MapObstacle { position: Vec2::new(-1400.0, 0.0), radius: 85.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(1400.0, 0.0), radius: 85.0, kind: ObstacleKind::CliffRidge },

    // ── Central Battlefield Monoliths (Forms the Center Killzone Corridor) ──
    MapObstacle { position: Vec2::new(-240.0, 0.0), radius: 80.0, kind: ObstacleKind::RockMonolith },
    MapObstacle { position: Vec2::new(240.0, 0.0), radius: 80.0, kind: ObstacleKind::RockMonolith },

    // ── Mid-Flank Mountain Ridges (Separates Mid from Side Passages) ──
    MapObstacle { position: Vec2::new(-600.0, -250.0), radius: 75.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(600.0, 250.0), radius: 75.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(600.0, -250.0), radius: 75.0, kind: ObstacleKind::CliffRidge },
    MapObstacle { position: Vec2::new(-600.0, 250.0), radius: 75.0, kind: ObstacleKind::CliffRidge },
];

/// Marks all static map obstacles onto the A* navigation grid with unit clearance padding
pub fn mark_static_obstacles(nav_grid: &mut NavGrid) {
    for obs in STATIC_MAP_OBSTACLES {
        nav_grid.mark_circle(obs.position, obs.radius);
    }
}

/// Checks whether a circular footprint overlaps any static map obstacle
pub fn is_obstacle_blocked(pos: Vec2, radius: f32, margin: f32) -> bool {
    for obs in STATIC_MAP_OBSTACLES {
        if pos.distance(obs.position) < (radius + obs.radius + margin) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_and_mineral_symmetry() {
        // Test 180° rotational symmetry: P2 position should be -P1 position
        assert_eq!(P1_BASE_POS, -P2_BASE_POS);

        for (p1_min, p2_min) in P1_MAIN_MINERALS.iter().zip(P2_MAIN_MINERALS.iter()) {
            assert_eq!(*p1_min, -*p2_min, "Mineral nodes must be 180° rotationally symmetric");
        }

        for (p1_w, p2_w) in P1_STARTER_WORKERS.iter().zip(P2_STARTER_WORKERS.iter()) {
            assert_eq!(*p1_w, -*p2_w, "Worker spawns must be 180° rotationally symmetric");
        }

        for (p1_exp, p2_exp) in P1_NATURAL_EXPANSION_MINERALS.iter().zip(P2_NATURAL_EXPANSION_MINERALS.iter()) {
            assert_eq!(*p1_exp, -*p2_exp, "Expansion minerals must be 180° rotationally symmetric");
        }

        for (west_min, east_min) in CONTESTED_WEST_MINERALS.iter().zip(CONTESTED_EAST_MINERALS.iter()) {
            assert_eq!(*west_min, -*east_min, "Contested minerals must be 180° rotationally symmetric");
        }
    }

    #[test]
    fn test_obstacle_180_degree_symmetry() {
        // For every obstacle at pos (x, y) with radius r, there must exist a matching obstacle at (-x, -y) with radius r
        for obs in STATIC_MAP_OBSTACLES {
            let mirrored_pos = -obs.position;
            let found = STATIC_MAP_OBSTACLES.iter().any(|other| {
                (other.position - mirrored_pos).length() < 0.001
                    && (other.radius - obs.radius).abs() < 0.001
                    && other.kind == obs.kind
            });
            assert!(
                found,
                "Obstacle at {:?} (radius: {}) must have a mirrored counter-part at {:?}",
                obs.position, obs.radius, mirrored_pos
            );
        }
    }

    #[test]
    fn test_nav_grid_marks_all_static_obstacles() {
        let mut nav = NavGrid::default();
        mark_static_obstacles(&mut nav);

        for obs in STATIC_MAP_OBSTACLES {
            let (gx, gy) = NavGrid::world_to_grid(obs.position).expect("Obstacle should be in grid bounds");
            assert!(nav.is_blocked(gx, gy), "Center of obstacle {:?} must be blocked in NavGrid", obs.position);
        }
    }

    #[test]
    fn test_minerals_and_bases_have_unobstructed_clearance() {
        assert!(!is_obstacle_blocked(P1_BASE_POS, 60.0, 0.0), "P1 Base HQ must not overlap obstacles");
        assert!(!is_obstacle_blocked(P2_BASE_POS, 60.0, 0.0), "P2 Base HQ must not overlap obstacles");

        // Verify clear line of sight and no obstacle intersection for all mineral nodes
        for &min in P1_MAIN_MINERALS.iter() {
            assert!(!is_obstacle_blocked(min, 32.0, 0.0), "P1 Main mineral node {:?} must not overlap obstacles", min);
        }
        for &min in P2_MAIN_MINERALS.iter() {
            assert!(!is_obstacle_blocked(min, 32.0, 0.0), "P2 Main mineral node {:?} must not overlap obstacles", min);
        }
        for &min in P1_NATURAL_EXPANSION_MINERALS.iter() {
            assert!(!is_obstacle_blocked(min, 32.0, 0.0), "P1 Natural expansion node {:?} must not overlap obstacles", min);
        }
        for &min in P2_NATURAL_EXPANSION_MINERALS.iter() {
            assert!(!is_obstacle_blocked(min, 32.0, 0.0), "P2 Natural expansion node {:?} must not overlap obstacles", min);
        }
        for &min in CONTESTED_WEST_MINERALS.iter() {
            assert!(!is_obstacle_blocked(min, 32.0, 0.0), "West contested mineral node {:?} must not overlap obstacles", min);
        }
        for &min in CONTESTED_EAST_MINERALS.iter() {
            assert!(!is_obstacle_blocked(min, 32.0, 0.0), "East contested mineral node {:?} must not overlap obstacles", min);
        }
    }
}
