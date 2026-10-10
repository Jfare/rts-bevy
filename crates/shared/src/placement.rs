//! Building placement rules shared by the client (ghost preview) and the server (authoritative check).
//! Both sides MUST call `is_placement_clear` so they always agree.

use bevy::prelude::*;

use crate::grid::BuildingKind;

/// Buildings must fit fully inside [-HALF_EXTENT, HALF_EXTENT] on both axes.
pub const MAP_PLACEMENT_HALF_EXTENT: f32 = 1500.0;
/// Radius of a gold rock / resource node.
pub const RESOURCE_NODE_RADIUS: f32 = 36.0;
/// Radius used for every unit in placement checks (largest unit radius).
pub const UNIT_PLACEMENT_RADIUS: f32 = 16.0;

const BUILDING_GAP: f32 = 8.0;
const RESOURCE_GAP: f32 = 16.0;
const OBSTACLE_GAP: f32 = 8.0;

/// Returns true if `kind` can be placed at `pos`.
///
/// * `buildings`: (centre, radius) of every existing building in the same room/match
/// * `resource_nodes`: centre of every resource node in the same room/match
/// * `units`: centre of every unit in the same room/match
///
/// Tech-tree and mineral checks are NOT done here.
pub fn is_placement_clear<B, R, U>(
    kind: BuildingKind,
    pos: Vec2,
    buildings: B,
    resource_nodes: R,
    units: U,
) -> bool
where
    B: IntoIterator<Item = (Vec2, f32)>,
    R: IntoIterator<Item = Vec2>,
    U: IntoIterator<Item = Vec2>,
{
    let size = kind.size();
    let radius = kind.footprint_radius();
    let e = MAP_PLACEMENT_HALF_EXTENT;

    if pos.x - size.x * 0.5 < -e
        || pos.x + size.x * 0.5 > e
        || pos.y - size.y * 0.5 < -e
        || pos.y + size.y * 0.5 > e
    {
        return false;
    }

    for (b_pos, b_radius) in buildings {
        if pos.distance(b_pos) < radius + b_radius + BUILDING_GAP {
            return false;
        }
    }

    for r_pos in resource_nodes {
        if pos.distance(r_pos) < radius + RESOURCE_NODE_RADIUS + RESOURCE_GAP {
            return false;
        }
    }

    for u_pos in units {
        if pos.distance(u_pos) < radius + UNIT_PLACEMENT_RADIUS {
            return false;
        }
    }

    if crate::map::is_obstacle_blocked(pos, radius, OBSTACLE_GAP) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::P1_BASE_POS;

    const NO_B: [(Vec2, f32); 0] = [];
    const NO_P: [Vec2; 0] = [];

    #[test]
    fn test_footprint_radius() {
        assert_eq!(BuildingKind::BaseHQ.footprint_radius(), 55.0);
        assert_eq!(BuildingKind::Barracks.footprint_radius(), 48.0);
        assert_eq!(BuildingKind::SupplyDepot.footprint_radius(), 32.0);
        assert_eq!(BuildingKind::Turret.footprint_radius(), 28.0);
    }

    #[test]
    fn test_clear_spot_is_valid() {
        assert!(is_placement_clear(BuildingKind::Turret, P1_BASE_POS, NO_B, NO_P, NO_P));
    }

    #[test]
    fn test_blocked_by_building() {
        let b = [(P1_BASE_POS + Vec2::new(50.0, 0.0), 55.0)];
        assert!(!is_placement_clear(BuildingKind::Turret, P1_BASE_POS, b, NO_P, NO_P));
    }

    #[test]
    fn test_blocked_by_resource_node() {
        let r = [P1_BASE_POS + Vec2::new(60.0, 0.0)];
        assert!(!is_placement_clear(BuildingKind::Turret, P1_BASE_POS, NO_B, r, NO_P));
    }

    #[test]
    fn test_blocked_by_unit() {
        let u = [P1_BASE_POS + Vec2::new(20.0, 0.0)];
        assert!(!is_placement_clear(BuildingKind::Turret, P1_BASE_POS, NO_B, NO_P, u));
    }

    #[test]
    fn test_out_of_bounds() {
        assert!(!is_placement_clear(BuildingKind::Turret, Vec2::new(1490.0, 0.0), NO_B, NO_P, NO_P));
    }
}
