use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::{Faction, MoveTarget, NetEntity, Radius, ResourceNode, Selectable, Worker};
use shared::protocol::ClientMessage;
use crate::net::{NetClient, NetStatus};
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
    mut net_client: ResMut<NetClient>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>)>,
    node_query: Query<(&Transform, &Radius, &ResourceNode), With<ResourceNode>>,
    mut unit_query: Query<(Entity, &Faction, &Selectable, Option<&NetEntity>, Option<&mut MoveTarget>, Option<&Worker>)>,
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

    // Check if clicked directly on an active mineral resource node
    let is_clicking_mineral = node_query.iter().any(|(t, r, n)| {
        t.translation.truncate().distance(target_world_pos) <= (r.0 + 20.0) && n.remaining_minerals > 0
    });

    // 1. Collect selected player units and their NetIDs
    let mut selected_units = Vec::new();
    let mut selected_net_ids = Vec::new();
    let mut has_workers = false;

    for (entity, faction, selectable, net_entity_opt, move_target, worker_opt) in &mut unit_query {
        if *faction == net_client.my_faction && selectable.is_selected {
            // If clicking mineral and unit is worker, skip ground move (mining system handles harvest)
            if is_clicking_mineral && worker_opt.is_some() {
                has_workers = true;
                continue;
            }

            selected_units.push((entity, move_target));
            if let Some(net) = net_entity_opt {
                selected_net_ids.push(net.net_id);
            }
        }
    }

    // If only workers were selected and clicking mineral, show cyan harvest marker and exit
    if is_clicking_mineral && has_workers && selected_units.is_empty() {
        commands.spawn((
            CommandMarker {
                lifetime: 0.0,
                max_lifetime: 0.45,
                initial_radius: 20.0,
                color: Color::srgba(0.20, 0.90, 1.0, 0.95), // Cyan for harvest order
            },
            Transform::from_xyz(target_world_pos.x, target_world_pos.y, 1.0),
        ));
        return;
    }

    if selected_units.is_empty() {
        return;
    }

    // Send networked command if online
    if net_client.status != NetStatus::Disconnected && !selected_net_ids.is_empty() {
        net_client.send(&ClientMessage::RequestMove {
            unit_net_ids: selected_net_ids,
            target_position: target_world_pos,
            is_attack_move,
        });
    }

    let unit_count = selected_units.len();

    // 2. Assign formation destinations (Client-side local prediction)
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
