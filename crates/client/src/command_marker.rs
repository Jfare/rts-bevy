use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::{Faction, MoveTarget, Selectable};
use crate::selection::screen_to_world_2d;

/// Visual expanding and fading marker at ground destination when right-clicking
#[derive(Component)]
pub struct CommandMarker {
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub initial_radius: f32,
    pub color: Color,
}

pub struct CommandMarkerPlugin;

impl Plugin for CommandMarkerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (handle_right_click_orders, update_and_draw_command_markers),
        );
    }
}

/// Handles right-click movement/attack-move order dispatch
fn handle_right_click_orders(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>)>,
    mut unit_query: Query<(Entity, &Faction, &Selectable, Option<&mut MoveTarget>)>,
) {
    if !mouse_button.just_pressed(MouseButton::Right) {
        return;
    }

    let Ok((_camera, cam_transform, ortho_opt)) = camera_query.get_single() else {
        return;
    };
    let Ok(window) = window_query.get_single() else {
        return;
    };

    let Some(cursor_screen) = window.cursor_position() else {
        return;
    };

    let win_size = Vec2::new(window.width(), window.height());
    let cam_pos = cam_transform.translation.truncate();
    let cam_scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);

    let target_world_pos = screen_to_world_2d(cursor_screen, win_size, cam_pos, cam_scale);

    let is_attack_move = keyboard.pressed(KeyCode::KeyA);

    // 1. Collect selected player units
    let mut selected_units = Vec::new();
    for (entity, faction, selectable, move_target) in &mut unit_query {
        if *faction == Faction::Player1 && selectable.is_selected {
            selected_units.push((entity, move_target));
        }
    }

    if selected_units.is_empty() {
        return;
    }

    let unit_count = selected_units.len();

    // 2. Assign formation destinations so units don't overlap into one exact point
    for (i, (entity, move_target_opt)) in selected_units.into_iter().enumerate() {
        let formation_offset = if unit_count > 1 {
            let angle = (i as f32) * 2.39996; // Golden angle spread
            let dist = 24.0 * (i as f32).sqrt();
            Vec2::new(angle.cos(), angle.sin()) * dist
        } else {
            Vec2::ZERO
        };

        let destination = target_world_pos + formation_offset;

        if let Some(mut existing_target) = move_target_opt {
            existing_target.destination = destination;
            existing_target.is_attack_move = is_attack_move;
        } else {
            commands.entity(entity).insert(MoveTarget {
                destination,
                is_attack_move,
            });
        }
    }

    // 3. Spawn visual tactical pulse marker
    let marker_color = if is_attack_move {
        Color::srgba(1.0, 0.35, 0.25, 0.95) // Red-orange for Attack-Move
    } else {
        Color::srgba(0.25, 0.95, 0.45, 0.95) // Bright green for Move
    };

    commands.spawn((
        CommandMarker {
            lifetime: 0.0,
            max_lifetime: 0.45,
            initial_radius: 20.0,
            color: marker_color,
        },
        Transform::from_xyz(target_world_pos.x, target_world_pos.y, 1.0),
    ));
}

/// Updates timers, shrinks and draws command pulse markers
fn update_and_draw_command_markers(
    mut commands: Commands,
    time: Res<Time>,
    mut gizmos: Gizmos,
    mut query: Query<(Entity, &mut CommandMarker, &Transform)>,
) {
    let dt = time.delta_secs();

    for (entity, mut marker, transform) in &mut query {
        marker.lifetime += dt;
        if marker.lifetime >= marker.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        let progress = marker.lifetime / marker.max_lifetime;
        let radius = marker.initial_radius * (1.0 - progress * 0.4);
        let alpha = (1.0 - progress) * 0.9;
        let color = marker.color.with_alpha(alpha);

        let pos = transform.translation.truncate();
        gizmos.circle_2d(pos, radius, color);
        gizmos.circle_2d(pos, radius * 0.5, color.with_alpha(alpha * 0.6));
    }
}
