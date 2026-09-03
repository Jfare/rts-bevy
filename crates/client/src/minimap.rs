use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::{Building, Faction, Health, MoveTarget, ResourceNode, Selectable, Unit};
use shared::grid::WorldGridConfig;
use shared::protocol::ClientMessage;
use crate::camera::RtsCamera;
use crate::fog_of_war::{FogOfWarGrid, FogState, FOG_GRID_DIM};
use crate::net::{NetClient, NetStatus};

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MinimapState>()
            .add_systems(Update, (draw_minimap_system, handle_minimap_input));
    }
}

#[derive(Resource)]
pub struct MinimapState {
    pub width: f32,
    pub height: f32,
    pub padding: f32,
    pub is_dragging: bool,
}

impl Default for MinimapState {
    fn default() -> Self {
        Self {
            width: 170.0,
            height: 170.0,
            padding: 12.0,
            is_dragging: false,
        }
    }
}

/// Helper to get the screen-space Rect of the minimap (bottom-left area)
pub fn get_minimap_screen_rect(window: &Window, state: &MinimapState) -> Rect {
    let x_min = state.padding;
    let y_max = window.height() - state.padding;
    let x_max = x_min + state.width;
    let y_min = y_max - state.height;
    Rect {
        min: Vec2::new(x_min, y_min),
        max: Vec2::new(x_max, y_max),
    }
}


/// Transforms a 2D world coordinate into a screen-space coordinate inside the minimap Rect
pub fn world_to_minimap_screen(
    world_pos: Vec2,
    grid_cfg: &WorldGridConfig,
    minimap_rect: &Rect,
) -> Vec2 {
    let norm_x = ((world_pos.x - grid_cfg.min_bounds.x) / grid_cfg.width()).clamp(0.0, 1.0);
    let norm_y = ((world_pos.y - grid_cfg.min_bounds.y) / grid_cfg.height()).clamp(0.0, 1.0);

    let screen_x = minimap_rect.min.x + norm_x * (minimap_rect.max.x - minimap_rect.min.x);
    // Invert Y because Bevy UI / Screen origin is Top-Left, whereas World Y is Up
    let screen_y = minimap_rect.max.y - norm_y * (minimap_rect.max.y - minimap_rect.min.y);

    Vec2::new(screen_x, screen_y)
}

/// Transforms a screen-space coordinate inside the minimap Rect back into a 2D world coordinate
pub fn minimap_screen_to_world(
    screen_pos: Vec2,
    grid_cfg: &WorldGridConfig,
    minimap_rect: &Rect,
) -> Vec2 {
    let norm_x = ((screen_pos.x - minimap_rect.min.x) / (minimap_rect.max.x - minimap_rect.min.x)).clamp(0.0, 1.0);
    let norm_y = ((minimap_rect.max.y - screen_pos.y) / (minimap_rect.max.y - minimap_rect.min.y)).clamp(0.0, 1.0);

    let world_x = grid_cfg.min_bounds.x + norm_x * grid_cfg.width();
    let world_y = grid_cfg.min_bounds.y + norm_y * grid_cfg.height();

    Vec2::new(world_x, world_y)
}

/// Renders the radar backdrop, blips, entities, and camera frustum
fn draw_minimap_system(
    mut gizmos: Gizmos,
    minimap_state: Res<MinimapState>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    fog: Res<FogOfWarGrid>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>), With<RtsCamera>>,
    net_client: Res<NetClient>,
    units_query: Query<(&Transform, &Faction, &Health), (With<Unit>, Without<Building>, Without<ResourceNode>)>,
    buildings_query: Query<(&Transform, &Faction, &Health), (With<Building>, Without<ResourceNode>)>,
    resources_query: Query<(&Transform, &ResourceNode)>,
) {
    let Ok(window) = window_query.get_single() else {
        return;
    };
    let Ok((_cam, cam_tf, ortho_opt)) = camera_query.get_single() else {
        return;
    };

    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);
    let mm_rect = get_minimap_screen_rect(window, &minimap_state);

    // Convert Screen coordinates to Camera 2D World coordinates to draw with Gizmos
    let win_size = Vec2::new(window.width(), window.height());
    let cam_pos = cam_tf.translation.truncate();
    let cam_scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);

    let to_world = |screen_pos: Vec2| -> Vec2 {
        let centered = Vec2::new(
            screen_pos.x - win_size.x * 0.5,
            (win_size.y * 0.5) - screen_pos.y,
        );
        cam_pos + centered * cam_scale
    };

    let p_min = to_world(Vec2::new(mm_rect.min.x, mm_rect.min.y));
    let p_max = to_world(Vec2::new(mm_rect.max.x, mm_rect.max.y));
    let center = (p_min + p_max) * 0.5;
    let size = (p_max - p_min).abs();

    // 1. Draw Radar Frame Background & Borders
    let border_color = Color::srgba(0.20, 0.45, 0.70, 0.90);
    gizmos.rect_2d(center, size, border_color);

    // Subtle radar crosshair in minimap center
    gizmos.line_2d(
        to_world(Vec2::new(mm_rect.min.x, (mm_rect.min.y + mm_rect.max.y) * 0.5)),
        to_world(Vec2::new(mm_rect.max.x, (mm_rect.min.y + mm_rect.max.y) * 0.5)),
        Color::srgba(0.15, 0.30, 0.45, 0.35),
    );
    gizmos.line_2d(
        to_world(Vec2::new((mm_rect.min.x + mm_rect.max.x) * 0.5, mm_rect.min.y)),
        to_world(Vec2::new((mm_rect.min.x + mm_rect.max.x) * 0.5, mm_rect.max.y)),
        Color::srgba(0.15, 0.30, 0.45, 0.35),
    );

    // 1.5. Draw Static Map Obstacles (Dark Slate Radar Terrain) - shrouded if unexplored
    let obs_color = Color::srgba(0.12, 0.16, 0.22, 0.90);
    let obs_border = Color::srgba(0.24, 0.35, 0.45, 0.70);
    let mm_scale = (mm_rect.max.x - mm_rect.min.x) / config.width();
    for obs in shared::map::STATIC_MAP_OBSTACLES {
        if fog.get_state_at_world_pos(obs.position, config) == FogState::Unexplored {
            continue;
        }
        let sc = world_to_minimap_screen(obs.position, config, &mm_rect);
        let wp = to_world(sc);
        let r_cam = obs.radius * mm_scale * cam_scale;
        gizmos.circle_2d(wp, r_cam, obs_color);
        gizmos.circle_2d(wp, r_cam, obs_border);
    }

    // 2. Draw Mineral Nodes (Gold Diamonds) - shrouded if unexplored
    let gold_color = Color::srgb(0.95, 0.80, 0.20);
    for (res_tf, _) in &resources_query {
        let r_pos = res_tf.translation.truncate();
        if fog.get_state_at_world_pos(r_pos, config) == FogState::Unexplored {
            continue;
        }
        let sc = world_to_minimap_screen(r_pos, config, &mm_rect);
        let wp = to_world(sc);
        gizmos.rect_2d(wp, Vec2::splat(3.5 * cam_scale), gold_color);
    }

    // 3. Draw Buildings (Sized colored boxes) - shrouded if hostile & unexplored
    for (b_tf, faction, hp) in &buildings_query {
        if hp.is_dead() {
            continue;
        }
        let b_pos = b_tf.translation.truncate();
        if *faction != net_client.my_faction && *faction != Faction::Neutral {
            if fog.get_state_at_world_pos(b_pos, config) == FogState::Unexplored {
                continue;
            }
        }

        let sc = world_to_minimap_screen(b_pos, config, &mm_rect);
        let wp = to_world(sc);
        let b_color = if *faction == net_client.my_faction {
            net_client.my_color.to_color()
        } else if *faction == Faction::Neutral {
            Color::srgb(0.60, 0.65, 0.70)
        } else {
            Color::srgb(0.95, 0.30, 0.30)
        };
        gizmos.rect_2d(wp, Vec2::splat(6.0 * cam_scale), b_color);
    }

    // 4. Draw Units (Small dots) - shrouded if hostile & not visible
    for (u_tf, faction, hp) in &units_query {
        if hp.is_dead() {
            continue;
        }
        let u_pos = u_tf.translation.truncate();
        if *faction != net_client.my_faction && *faction != Faction::Neutral {
            if fog.get_state_at_world_pos(u_pos, config) != FogState::Visible {
                continue;
            }
        }

        let sc = world_to_minimap_screen(u_pos, config, &mm_rect);
        let wp = to_world(sc);
        let u_color = if *faction == net_client.my_faction {
            net_client.my_color.to_color().lighter(0.15)
        } else {
            Color::srgb(0.95, 0.25, 0.25)
        };
        gizmos.circle_2d(wp, 2.5 * cam_scale, u_color);
    }

    // 5. Draw Fog of War Shroud Tiles on Minimap
    let f_cell_w = (mm_rect.max.x - mm_rect.min.x) / FOG_GRID_DIM as f32;
    let f_cell_h = (mm_rect.max.y - mm_rect.min.y) / FOG_GRID_DIM as f32;
    for cy in 0..FOG_GRID_DIM {
        for cx in 0..FOG_GRID_DIM {
            let state = fog.get_state(cx, cy);
            if state == FogState::Unexplored {
                let sc_center = Vec2::new(
                    mm_rect.min.x + (cx as f32 + 0.5) * f_cell_w,
                    mm_rect.max.y - (cy as f32 + 0.5) * f_cell_h,
                );
                let wp = to_world(sc_center);
                gizmos.rect_2d(wp, Vec2::new(f_cell_w * cam_scale + 0.5, f_cell_h * cam_scale + 0.5), Color::srgba(0.01, 0.02, 0.04, 0.88));
            } else if state == FogState::Explored {
                let sc_center = Vec2::new(
                    mm_rect.min.x + (cx as f32 + 0.5) * f_cell_w,
                    mm_rect.max.y - (cy as f32 + 0.5) * f_cell_h,
                );
                let wp = to_world(sc_center);
                gizmos.rect_2d(wp, Vec2::new(f_cell_w * cam_scale + 0.5, f_cell_h * cam_scale + 0.5), Color::srgba(0.02, 0.04, 0.06, 0.45));
            }
        }
    }

    // 6. Draw Camera View Frustum Rect (White outline box)
    let view_w = win_size.x * cam_scale;
    let view_h = win_size.y * cam_scale;
    let cam_world_min = cam_pos - Vec2::new(view_w, view_h) * 0.5;
    let cam_world_max = cam_pos + Vec2::new(view_w, view_h) * 0.5;

    let frustum_sc_min = world_to_minimap_screen(cam_world_min, config, &mm_rect);
    let frustum_sc_max = world_to_minimap_screen(cam_world_max, config, &mm_rect);

    let f_min = to_world(Vec2::new(
        frustum_sc_min.x.min(frustum_sc_max.x),
        frustum_sc_min.y.min(frustum_sc_max.y),
    ));
    let f_max = to_world(Vec2::new(
        frustum_sc_min.x.max(frustum_sc_max.x),
        frustum_sc_min.y.max(frustum_sc_max.y),
    ));
    let f_center = (f_min + f_max) * 0.5;
    let f_size = (f_max - f_min).abs();

    gizmos.rect_2d(f_center, f_size, Color::srgba(1.0, 1.0, 1.0, 0.85));
}

use shared::components::NetEntity;

/// Handles clicking or dragging inside the minimap to pan camera or issue orders
fn handle_minimap_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    mut camera_query: Query<&mut Transform, With<RtsCamera>>,
    mut minimap_state: ResMut<MinimapState>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    net_client: Res<NetClient>,
    mut unit_query: Query<(Entity, &Faction, &Selectable, &mut MoveTarget, Option<&NetEntity>), (With<Unit>, Without<Building>)>,
) {
    let Ok(window) = window_query.get_single() else {
        return;
    };
    let Ok(mut cam_tf) = camera_query.get_single_mut() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        minimap_state.is_dragging = false;
        return;
    };

    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);
    let mm_rect = get_minimap_screen_rect(window, &minimap_state);

    let is_inside = cursor_pos.x >= mm_rect.min.x
        && cursor_pos.x <= mm_rect.max.x
        && cursor_pos.y >= mm_rect.min.y
        && cursor_pos.y <= mm_rect.max.y;

    // 1. Left-Click or Drag: Pan Camera to Target World Coordinate
    if mouse_button.just_pressed(MouseButton::Left) && is_inside {
        minimap_state.is_dragging = true;
    }
    if mouse_button.just_released(MouseButton::Left) {
        minimap_state.is_dragging = false;
    }

    if minimap_state.is_dragging && mouse_button.pressed(MouseButton::Left) {
        let world_pos = minimap_screen_to_world(cursor_pos, config, &mm_rect);
        cam_tf.translation.x = world_pos.x;
        cam_tf.translation.y = world_pos.y;
    }

    // 2. Right-Click: Issue Squad Move / Attack Order via Minimap
    if mouse_button.just_pressed(MouseButton::Right) && is_inside {
        let target_world_pos = minimap_screen_to_world(cursor_pos, config, &mm_rect);
        let is_attack_move = keyboard.pressed(KeyCode::KeyA);
        let my_faction = net_client.my_faction;

        let mut unit_net_ids = Vec::new();
        for (_, faction, selectable, mut mt, net_opt) in &mut unit_query {
            if *faction == my_faction && selectable.is_selected {
                mt.destination = target_world_pos;
                mt.is_attack_move = is_attack_move;
                mt.waypoints = vec![target_world_pos];
                mt.current_waypoint_idx = 0;
                if let Some(net) = net_opt {
                    unit_net_ids.push(net.net_id);
                }
            }
        }

        if net_client.status != NetStatus::Disconnected {
            net_client.send(&ClientMessage::RequestMove {
                unit_net_ids,
                target_position: target_world_pos,
                is_attack_move,
            });
        }


        info!("🗺️ [Minimap] Dispatched move order to {:?}", target_world_pos);
    }
}
