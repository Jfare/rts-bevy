use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::{AppState, Building, Faction, Unit, Worker};

use shared::grid::WorldGridConfig;
use crate::camera::RtsCamera;
use crate::net::NetClient;

pub const FOG_GRID_DIM: usize = 40;
pub const FOG_CELL_SIZE: f32 = 80.0; // 40 * 80 = 3200 px

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FogState {
    Unexplored = 0,
    Explored = 1,
    Visible = 2,
}

#[derive(Resource)]
pub struct FogOfWarGrid {
    pub cells: [u8; FOG_GRID_DIM * FOG_GRID_DIM],
}

impl Default for FogOfWarGrid {
    fn default() -> Self {
        Self {
            cells: [0; FOG_GRID_DIM * FOG_GRID_DIM],
        }
    }
}

impl FogOfWarGrid {
    pub fn world_to_grid(&self, pos: Vec2, grid_cfg: &WorldGridConfig) -> Option<(usize, usize)> {
        if !grid_cfg.is_inside(pos, 0.0) {
            return None;
        }
        let t_x = (pos.x - grid_cfg.min_bounds.x) / grid_cfg.width();
        let t_y = (pos.y - grid_cfg.min_bounds.y) / grid_cfg.height();

        let cx = ((t_x * FOG_GRID_DIM as f32).floor() as usize).min(FOG_GRID_DIM - 1);
        let cy = ((t_y * FOG_GRID_DIM as f32).floor() as usize).min(FOG_GRID_DIM - 1);
        Some((cx, cy))
    }

    pub fn grid_to_world_center(&self, cx: usize, cy: usize, grid_cfg: &WorldGridConfig) -> Vec2 {
        let step_x = grid_cfg.width() / FOG_GRID_DIM as f32;
        let step_y = grid_cfg.height() / FOG_GRID_DIM as f32;
        Vec2::new(
            grid_cfg.min_bounds.x + (cx as f32 + 0.5) * step_x,
            grid_cfg.min_bounds.y + (cy as f32 + 0.5) * step_y,
        )
    }

    pub fn get_state(&self, cx: usize, cy: usize) -> FogState {
        if cx >= FOG_GRID_DIM || cy >= FOG_GRID_DIM {
            return FogState::Unexplored;
        }
        match self.cells[cy * FOG_GRID_DIM + cx] {
            2 => FogState::Visible,
            1 => FogState::Explored,
            _ => FogState::Unexplored,
        }
    }

    pub fn set_state(&mut self, cx: usize, cy: usize, state: FogState) {
        if cx < FOG_GRID_DIM && cy < FOG_GRID_DIM {
            self.cells[cy * FOG_GRID_DIM + cx] = state as u8;
        }
    }

    pub fn get_state_at_world_pos(&self, pos: Vec2, grid_cfg: &WorldGridConfig) -> FogState {
        if let Some((cx, cy)) = self.world_to_grid(pos, grid_cfg) {
            self.get_state(cx, cy)
        } else {
            FogState::Unexplored
        }
    }
}

pub struct FogOfWarPlugin;

impl Plugin for FogOfWarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FogOfWarGrid>()
            .add_systems(
                Update,
                (
                    update_fog_of_war,
                    update_fog_unit_visibility,
                    draw_fog_of_war_overlay,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Updates fog of war tiles based on friendly vision sources
fn update_fog_of_war(
    mut fog: ResMut<FogOfWarGrid>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    net_client: Res<NetClient>,
    units_query: Query<(&Transform, &Faction, Option<&Worker>), (With<Unit>, Without<Building>)>,
    buildings_query: Query<(&Transform, &Faction, &Building)>,
) {
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);
    let my_faction = net_client.my_faction;

    // 1. Demote all Visible cells to Explored
    for cell in fog.cells.iter_mut() {
        if *cell == 2 {
            *cell = 1;
        }
    }

    let reveal_circle = |center_pos: Vec2, radius: f32, fog: &mut FogOfWarGrid| {
        let cell_radius = (radius / (config.width() / FOG_GRID_DIM as f32)).ceil() as isize;
        if let Some((cx, cy)) = fog.world_to_grid(center_pos, config) {
            let cx_i = cx as isize;
            let cy_i = cy as isize;

            for dy in -cell_radius..=cell_radius {
                for dx in -cell_radius..=cell_radius {
                    let nx = cx_i + dx;
                    let ny = cy_i + dy;
                    if nx >= 0 && nx < FOG_GRID_DIM as isize && ny >= 0 && ny < FOG_GRID_DIM as isize {
                        let cell_world = fog.grid_to_world_center(nx as usize, ny as usize, config);
                        if cell_world.distance(center_pos) <= radius {
                            fog.set_state(nx as usize, ny as usize, FogState::Visible);
                        }
                    }
                }
            }
        }
    };

    // 2. Vision from Friendly Units
    for (tf, faction, worker_opt) in &units_query {
        if *faction == my_faction {
            let sight_radius = if worker_opt.is_some() { 220.0 } else { 280.0 };
            reveal_circle(tf.translation.truncate(), sight_radius, &mut fog);
        }
    }

    // 3. Vision from Friendly Buildings
    for (tf, faction, building) in &buildings_query {
        if *faction == my_faction {
            let sight_radius = if building.name.contains("Base HQ") {
                480.0
            } else if building.name.contains("Turret") {
                400.0
            } else {
                360.0
            };
            reveal_circle(tf.translation.truncate(), sight_radius, &mut fog);
        }
    }
}

/// Toggles visibility of hostile units and buildings based on fog of war
fn update_fog_unit_visibility(
    fog: Res<FogOfWarGrid>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    net_client: Res<NetClient>,
    mut hostiles_query: Query<(&Transform, &Faction, &mut Visibility), (With<Unit>, Without<Building>)>,
    mut buildings_query: Query<(&Transform, &Faction, &mut Visibility), With<Building>>,
) {
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);
    let my_faction = net_client.my_faction;

    for (tf, faction, mut visibility) in &mut hostiles_query {
        if *faction != my_faction && *faction != Faction::Neutral {
            let pos = tf.translation.truncate();
            let is_visible = fog.get_state_at_world_pos(pos, config) == FogState::Visible;

            *visibility = if is_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }

    for (tf, faction, mut visibility) in &mut buildings_query {
        if *faction != my_faction && *faction != Faction::Neutral {
            let pos = tf.translation.truncate();
            let state = fog.get_state_at_world_pos(pos, config);
            let is_visible = state != FogState::Unexplored;

            *visibility = if is_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Renders fog of war tactical overlay in the camera viewport
fn draw_fog_of_war_overlay(
    mut gizmos: Gizmos,
    fog: Res<FogOfWarGrid>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    camera_query: Query<(&Transform, Option<&OrthographicProjection>), With<RtsCamera>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = window_query.get_single() else {
        return;
    };
    let Ok((cam_tf, ortho_opt)) = camera_query.get_single() else {
        return;
    };

    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);

    let cam_pos = cam_tf.translation.truncate();
    let cam_scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);
    let view_w = window.width() * cam_scale;
    let view_h = window.height() * cam_scale;

    let min_view = cam_pos - Vec2::new(view_w, view_h) * 0.6;
    let max_view = cam_pos + Vec2::new(view_w, view_h) * 0.6;

    let cell_size = Vec2::splat(FOG_CELL_SIZE + 1.0); // +1.0 to prevent seams
    let unexplored_color = Color::srgba(0.015, 0.025, 0.04, 0.98);
    let explored_color = Color::srgba(0.03, 0.05, 0.08, 0.65);

    for cy in 0..FOG_GRID_DIM {
        for cx in 0..FOG_GRID_DIM {
            let cell_center = fog.grid_to_world_center(cx, cy, config);

            // Culling: Only draw cells within camera view
            if cell_center.x < min_view.x || cell_center.x > max_view.x
                || cell_center.y < min_view.y || cell_center.y > max_view.y {
                continue;
            }

            match fog.get_state(cx, cy) {
                FogState::Unexplored => {
                    gizmos.rect_2d(cell_center, cell_size, unexplored_color);
                }
                FogState::Explored => {
                    gizmos.rect_2d(cell_center, cell_size, explored_color);
                }
                FogState::Visible => {
                    // Transparent / clear
                }
            }
        }
    }
}
