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

// ─────────────────────────────────────────────────────────────────────────────
// A* NAVIGATION GRID & PATHFINDING
// ─────────────────────────────────────────────────────────────────────────────

pub const NAV_GRID_DIM: usize = 64;
pub const NAV_CELL_SIZE: f32 = 50.0; // 64 * 50 = 3200 px

#[derive(Debug, Clone, Resource, Reflect)]
pub struct NavGrid {
    pub blocked: [bool; NAV_GRID_DIM * NAV_GRID_DIM],
}

impl Default for NavGrid {
    fn default() -> Self {
        Self {
            blocked: [false; NAV_GRID_DIM * NAV_GRID_DIM],
        }
    }
}

impl NavGrid {
    pub fn clear(&mut self) {
        self.blocked.fill(false);
    }

    #[inline]
    pub fn world_to_grid(pos: Vec2) -> Option<(usize, usize)> {
        let min_x = -1600.0;
        let min_y = -1600.0;
        if pos.x < min_x || pos.x > 1600.0 || pos.y < min_y || pos.y > 1600.0 {
            return None;
        }
        let gx = ((pos.x - min_x) / NAV_CELL_SIZE).floor() as usize;
        let gy = ((pos.y - min_y) / NAV_CELL_SIZE).floor() as usize;
        Some((gx.min(NAV_GRID_DIM - 1), gy.min(NAV_GRID_DIM - 1)))
    }

    #[inline]
    pub fn grid_to_world(gx: usize, gy: usize) -> Vec2 {
        Vec2::new(
            -1600.0 + (gx as f32 + 0.5) * NAV_CELL_SIZE,
            -1600.0 + (gy as f32 + 0.5) * NAV_CELL_SIZE,
        )
    }

    #[inline]
    pub fn is_blocked(&self, gx: usize, gy: usize) -> bool {
        if gx >= NAV_GRID_DIM || gy >= NAV_GRID_DIM {
            true
        } else {
            self.blocked[gy * NAV_GRID_DIM + gx]
        }
    }

    pub fn set_blocked(&mut self, gx: usize, gy: usize, blocked: bool) {
        if gx < NAV_GRID_DIM && gy < NAV_GRID_DIM {
            self.blocked[gy * NAV_GRID_DIM + gx] = blocked;
        }
    }

    /// Marks circular obstacle bounding area as blocked
    pub fn mark_circle(&mut self, center: Vec2, radius: f32) {
        let cell_r = ((radius + 12.0) / NAV_CELL_SIZE).ceil() as isize;
        if let Some((cgx, cgy)) = Self::world_to_grid(center) {
            let cx = cgx as isize;
            let cy = cgy as isize;
            for dy in -cell_r..=cell_r {
                for dx in -cell_r..=cell_r {
                    let nx = cx + dx;
                    let ny = cy + dy;
                    if nx >= 0 && nx < NAV_GRID_DIM as isize && ny >= 0 && ny < NAV_GRID_DIM as isize {
                        let cell_world = Self::grid_to_world(nx as usize, ny as usize);
                        if cell_world.distance(center) <= (radius + 10.0) {
                            self.set_blocked(nx as usize, ny as usize, true);
                        }
                    }
                }
            }
        }
    }

    /// Checks if line of sight between two world points is unblocked
    pub fn is_line_clear(&self, p1: Vec2, p2: Vec2) -> bool {
        let dist = p1.distance(p2);
        if dist < 1.0 {
            return true;
        }
        let steps = (dist / (NAV_CELL_SIZE * 0.4)).ceil() as usize;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let sample = p1.lerp(p2, t);
            if let Some((gx, gy)) = Self::world_to_grid(sample) {
                if self.is_blocked(gx, gy) {
                    return false;
                }
            }
        }
        true
    }

    /// Finds nearest walkable cell if a coordinate is blocked
    pub fn find_nearest_walkable(&self, gx: usize, gy: usize) -> Option<(usize, usize)> {
        if !self.is_blocked(gx, gy) {
            return Some((gx, gy));
        }
        for r in 1..8 {
            let r_i = r as isize;
            for dy in -r_i..=r_i {
                for dx in -r_i..=r_i {
                    if dx.abs() == r_i || dy.abs() == r_i {
                        let nx = gx as isize + dx;
                        let ny = gy as isize + dy;
                        if nx >= 0 && nx < NAV_GRID_DIM as isize && ny >= 0 && ny < NAV_GRID_DIM as isize {
                            let u_x = nx as usize;
                            let u_y = ny as usize;
                            if !self.is_blocked(u_x, u_y) {
                                return Some((u_x, u_y));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// 8-Directional A* Pathfinding from Start to Goal with line-of-sight shortcutting
    pub fn find_path(&self, start: Vec2, goal: Vec2) -> Vec<Vec2> {
        // Fast path: if direct line is clear, return straight goal
        if self.is_line_clear(start, goal) {
            return vec![goal];
        }

        let Some((sgx, sgy)) = Self::world_to_grid(start) else {
            return vec![goal];
        };
        let Some((ggx, ggy)) = Self::world_to_grid(goal) else {
            return vec![goal];
        };

        let start_node = match self.find_nearest_walkable(sgx, sgy) {
            Some(node) => node,
            None => return vec![goal],
        };
        let goal_node = match self.find_nearest_walkable(ggx, ggy) {
            Some(node) => node,
            None => return vec![goal],
        };

        if start_node == goal_node {
            return vec![goal];
        }

        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        #[derive(Copy, Clone, PartialEq)]
        struct NodeState {
            cost: f32,
            heuristic: f32,
            gx: usize,
            gy: usize,
        }

        impl Eq for NodeState {}

        impl Ord for NodeState {
            fn cmp(&self, other: &Self) -> Ordering {
                let f_self = self.cost + self.heuristic;
                let f_other = other.cost + other.heuristic;
                f_other.partial_cmp(&f_self).unwrap_or(Ordering::Equal)
            }
        }

        impl PartialOrd for NodeState {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut g_costs = vec![f32::INFINITY; NAV_GRID_DIM * NAV_GRID_DIM];
        let mut parent_map = vec![None; NAV_GRID_DIM * NAV_GRID_DIM];
        let mut open_set = BinaryHeap::new();

        let start_idx = start_node.1 * NAV_GRID_DIM + start_node.0;
        g_costs[start_idx] = 0.0;

        let heuristic = |ax: usize, ay: usize, bx: usize, by: usize| -> f32 {
            let dx = (ax as isize - bx as isize).abs() as f32;
            let dy = (ay as isize - by as isize).abs() as f32;
            (dx * dx + dy * dy).sqrt() * NAV_CELL_SIZE
        };

        open_set.push(NodeState {
            cost: 0.0,
            heuristic: heuristic(start_node.0, start_node.1, goal_node.0, goal_node.1),
            gx: start_node.0,
            gy: start_node.1,
        });

        let mut found = false;

        while let Some(current) = open_set.pop() {
            if current.gx == goal_node.0 && current.gy == goal_node.1 {
                found = true;
                break;
            }

            let cur_idx = current.gy * NAV_GRID_DIM + current.gx;
            if current.cost > g_costs[cur_idx] {
                continue;
            }

            let neighbors: [(isize, isize, f32); 8] = [
                (1, 0, 1.0),
                (-1, 0, 1.0),
                (0, 1, 1.0),
                (0, -1, 1.0),
                (1, 1, 1.414),
                (1, -1, 1.414),
                (-1, 1, 1.414),
                (-1, -1, 1.414),
            ];

            for (dx, dy, step_cost) in neighbors {
                let nx = current.gx as isize + dx;
                let ny = current.gy as isize + dy;

                if nx < 0 || nx >= NAV_GRID_DIM as isize || ny < 0 || ny >= NAV_GRID_DIM as isize {
                    continue;
                }

                let unx = nx as usize;
                let uny = ny as usize;

                if self.is_blocked(unx, uny) {
                    continue;
                }

                // Prevent cutting through corners of blocked cardinal neighbors on diagonals
                if dx != 0 && dy != 0 {
                    if self.is_blocked(current.gx, uny) || self.is_blocked(unx, current.gy) {
                        continue;
                    }
                }

                let next_cost = current.cost + step_cost * NAV_CELL_SIZE;
                let next_idx = uny * NAV_GRID_DIM + unx;

                if next_cost < g_costs[next_idx] {
                    g_costs[next_idx] = next_cost;
                    parent_map[next_idx] = Some((current.gx, current.gy));
                    open_set.push(NodeState {
                        cost: next_cost,
                        heuristic: heuristic(unx, uny, goal_node.0, goal_node.1),
                        gx: unx,
                        gy: uny,
                    });
                }
            }
        }

        if !found {
            return vec![goal];
        }

        // Reconstruct path
        let mut raw_waypoints = Vec::new();
        let mut curr = goal_node;
        raw_waypoints.push(goal);

        while curr != start_node {
            raw_waypoints.push(Self::grid_to_world(curr.0, curr.1));
            let idx = curr.1 * NAV_GRID_DIM + curr.0;
            match parent_map[idx] {
                Some(prev) => curr = prev,
                None => break,
            }
        }
        raw_waypoints.reverse();

        // Line-of-sight shortcutting (String Pulling)
        if raw_waypoints.len() <= 2 {
            return raw_waypoints;
        }

        let mut smoothed = Vec::new();
        smoothed.push(raw_waypoints[0]);
        let mut anchor = 0;

        while anchor < raw_waypoints.len() - 1 {
            let mut furthest = anchor + 1;
            for test_idx in (anchor + 2)..raw_waypoints.len() {
                if self.is_line_clear(raw_waypoints[anchor], raw_waypoints[test_idx]) {
                    furthest = test_idx;
                }
            }
            smoothed.push(raw_waypoints[furthest]);
            anchor = furthest;
        }

        smoothed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_to_grid_and_back() {
        let p = Vec2::new(0.0, 0.0);
        let (gx, gy) = NavGrid::world_to_grid(p).expect("Should map center");
        assert_eq!(gx, 32);
        assert_eq!(gy, 32);

        let world_back = NavGrid::grid_to_world(gx, gy);
        assert!((world_back - Vec2::new(25.0, 25.0)).length() < 1.0);
    }

    #[test]
    fn test_unobstructed_path_is_straight() {
        let nav = NavGrid::default();
        let start = Vec2::new(-200.0, -200.0);
        let goal = Vec2::new(200.0, 200.0);
        let path = nav.find_path(start, goal);
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], goal);
    }

    #[test]
    fn test_path_navigates_around_obstacle() {
        let mut nav = NavGrid::default();
        // Place building obstacle at center (0, 0)
        nav.mark_circle(Vec2::new(0.0, 0.0), 60.0);

        let start = Vec2::new(-300.0, 0.0);
        let goal = Vec2::new(300.0, 0.0);

        assert!(!nav.is_line_clear(start, goal));

        let path = nav.find_path(start, goal);
        assert!(path.len() >= 2);
        assert_eq!(path.last().copied(), Some(goal));

        // Ensure no waypoint is inside blocked obstacle
        for wp in &path {
            if let Some((gx, gy)) = NavGrid::world_to_grid(*wp) {
                assert!(!nav.is_blocked(gx, gy), "Waypoint should not be blocked!");
            }
        }
    }
}


