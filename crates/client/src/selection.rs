use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::{Faction, Radius, Selectable};

use crate::minimap::{get_minimap_screen_rect, MinimapState};
use crate::net::NetClient;


/// Helper to convert screen cursor coordinates to 2D world coordinates accurately across all platforms
pub fn screen_to_world_2d(
    cursor_pos: Vec2,
    window_size: Vec2,
    camera_pos: Vec2,
    camera_scale: f32,
) -> Vec2 {
    let centered = Vec2::new(
        cursor_pos.x - window_size.x * 0.5,
        (window_size.y * 0.5) - cursor_pos.y, // Invert Y
    );
    camera_pos + centered * camera_scale
}

/// State of the active drag selection marquee box
#[derive(Debug, Resource, Default)]
pub struct SelectionState {
    pub drag_start_screen: Option<Vec2>,
    pub drag_start_world: Option<Vec2>,
    pub current_world_pos: Option<Vec2>,
    pub is_dragging: bool,
}

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectionState>()
            .add_systems(Update, (handle_selection_input, draw_selection_gizmos));
    }
}


/// Handles mouse input for single-click and drag-box entity selection
fn handle_selection_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    net_client: Res<NetClient>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>)>,
    mut selection_state: ResMut<SelectionState>,
    mut selectable_query: Query<(Entity, &Transform, &Radius, &Faction, &mut Selectable)>,
) {

    let Ok((_camera, cam_transform, ortho_opt)) = camera_query.get_single() else {
        return;
    };
    let Ok(window) = window_query.get_single() else {
        return;
    };

    let Some(cursor_screen) = window.cursor_position() else {
        return;
    };

    let mm_rect = get_minimap_screen_rect(window, &MinimapState::default());
    let is_over_minimap = cursor_screen.x >= mm_rect.min.x
        && cursor_screen.x <= mm_rect.max.x
        && cursor_screen.y >= mm_rect.min.y
        && cursor_screen.y <= mm_rect.max.y;

    let win_size = Vec2::new(window.width(), window.height());
    let cam_pos = cam_transform.translation.truncate();
    let cam_scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);

    let cursor_world_pos = screen_to_world_2d(cursor_screen, win_size, cam_pos, cam_scale);

    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    // 1. Mouse Button Pressed (Start Drag)
    if mouse_button.just_pressed(MouseButton::Left) {
        if is_over_minimap {
            return;
        }
        selection_state.drag_start_screen = Some(cursor_screen);
        selection_state.drag_start_world = Some(cursor_world_pos);
        selection_state.current_world_pos = Some(cursor_world_pos);
        selection_state.is_dragging = false;
    }

    // 2. Mouse Button Held (Update Drag)
    if mouse_button.pressed(MouseButton::Left) {
        if let Some(start_screen) = selection_state.drag_start_screen {
            if start_screen.distance(cursor_screen) > 6.0 {
                selection_state.is_dragging = true;
                selection_state.current_world_pos = Some(cursor_world_pos);
            }
        }
    }

    // 3. Mouse Button Released (Commit Selection)
    if mouse_button.just_released(MouseButton::Left) {
        let start_world = selection_state.drag_start_world.unwrap_or(cursor_world_pos);
        let current_world = cursor_world_pos;

        if !shift_held {
            // Clear existing selection if not shift-adding
            for (_, _, _, _, mut sel) in &mut selectable_query {
                sel.is_selected = false;
            }
        }

        if selection_state.is_dragging {
            // Drag Box Selection
            let min_x = start_world.x.min(current_world.x);
            let max_x = start_world.x.max(current_world.x);
            let min_y = start_world.y.min(current_world.y);
            let max_y = start_world.y.max(current_world.y);

            let mut friendly_selected = false;

            // Pass 1: Select friendly units inside the box
            for (_, transform, _, faction, mut sel) in &mut selectable_query {
                let pos = transform.translation.truncate();
                if pos.x >= min_x && pos.x <= max_x && pos.y >= min_y && pos.y <= max_y {
                    if *faction == net_client.my_faction {
                        sel.is_selected = true;
                        friendly_selected = true;
                    }

                }
            }

            // Pass 2: If no friendly units were inside, select any units inside for inspection
            if !friendly_selected {
                for (_, transform, _, _, mut sel) in &mut selectable_query {
                    let pos = transform.translation.truncate();
                    if pos.x >= min_x && pos.x <= max_x && pos.y >= min_y && pos.y <= max_y {
                        sel.is_selected = true;
                    }
                }
            }
        } else {
            // Single Click Selection
            let mut closest_entity = None;
            let mut closest_dist = f32::MAX;

            for (entity, transform, radius, _, _) in &selectable_query {
                let pos = transform.translation.truncate();
                let dist = pos.distance(start_world);
                if dist <= (radius.0 + 24.0) && dist < closest_dist {
                    closest_dist = dist;
                    closest_entity = Some(entity);
                }
            }

            if let Some(target_entity) = closest_entity {
                if let Ok((_, _, _, _, mut sel)) = selectable_query.get_mut(target_entity) {
                    if shift_held {
                        sel.is_selected = !sel.is_selected;
                    } else {
                        sel.is_selected = true;
                    }
                }
            }
        }

        // Reset drag state
        selection_state.drag_start_screen = None;
        selection_state.drag_start_world = None;
        selection_state.current_world_pos = None;
        selection_state.is_dragging = false;
    }
}

/// Renders selection rings around selected entities and the active marquee drag box
fn draw_selection_gizmos(
    mut gizmos: Gizmos,
    selection_state: Res<SelectionState>,
    query: Query<(&Transform, &Radius, &Faction, &Selectable)>,
) {
    let friendly_ring_col = Color::srgba(0.22, 0.90, 0.40, 0.95);
    let friendly_outer_col = Color::srgba(0.22, 0.90, 0.40, 0.40);
    let enemy_ring_col = Color::srgba(0.95, 0.30, 0.30, 0.95);
    let enemy_outer_col = Color::srgba(0.95, 0.30, 0.30, 0.40);

    // 1. Draw Selection Rings around selected units
    for (transform, radius, faction, selectable) in &query {
        if selectable.is_selected {
            let center = transform.translation.truncate();
            let r = radius.0 + 5.0;
            let (ring_col, outer_col) = if *faction == Faction::Player1 {
                (friendly_ring_col, friendly_outer_col)
            } else {
                (enemy_ring_col, enemy_outer_col)
            };

            gizmos.circle_2d(center, r, ring_col);
            gizmos.circle_2d(center, r + 2.5, outer_col);
        }
    }

    // 2. Draw Active Marquee Selection Box
    if selection_state.is_dragging {
        if let (Some(start), Some(current)) = (
            selection_state.drag_start_world,
            selection_state.current_world_pos,
        ) {
            let center = (start + current) * 0.5;
            let size = (current - start).abs();
            let box_border_col = Color::srgba(0.25, 0.95, 0.45, 0.90);
            let box_inner_col = Color::srgba(0.25, 0.95, 0.45, 0.35);

            gizmos.rect_2d(center, size, box_border_col);
            gizmos.rect_2d(center, (size - Vec2::splat(2.0)).max(Vec2::ZERO), box_inner_col);
        }
    }
}
