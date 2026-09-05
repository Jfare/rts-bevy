use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use shared::components::AppState;
use shared::grid::WorldGridConfig;
use shared::protocol::{ClientMessage, FactionColor, PingType};
use crate::camera::RtsCamera;
use crate::minimap::{get_minimap_screen_rect, minimap_screen_to_world, world_to_minimap_screen, MinimapState};
use crate::net::NetClient;

pub struct TacticalPingPlugin;

impl Plugin for TacticalPingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_tactical_ping_input,
                draw_tactical_pings_world_system,
                draw_tactical_pings_minimap_system,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[allow(dead_code)]
#[derive(Component, Debug, Clone)]
pub struct TacticalPingVisual {
    pub position: Vec2,
    pub ping_type: PingType,
    pub color: FactionColor,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

fn handle_tactical_ping_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    minimap_state: Res<MinimapState>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    net_client: Res<NetClient>,
    mut commands: Commands,
) {
    let is_alt_pressed = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
    if !is_alt_pressed {
        return;
    }

    if buttons.just_pressed(MouseButton::Left) {
        let Ok(window) = window_query.get_single() else {
            return;
        };
        let Some(cursor_pos) = window.cursor_position() else {
            return;
        };

        let default_cfg = WorldGridConfig::default();
        let config = grid_cfg.as_deref().unwrap_or(&default_cfg);
        let minimap_rect = get_minimap_screen_rect(window, &minimap_state);

        let ping_pos = if minimap_rect.contains(cursor_pos) {
            minimap_screen_to_world(cursor_pos, config, &minimap_rect)
        } else {
            let Ok((camera, cam_transform)) = camera_query.get_single() else {
                return;
            };
            let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor_pos) else {
                return;
            };
            world_pos
        };

        let ping_type = if keyboard.pressed(KeyCode::KeyA) {
            PingType::Attack
        } else if keyboard.pressed(KeyCode::KeyD) {
            PingType::Defend
        } else {
            PingType::Attention
        };

        // Dispatch over network
        net_client.send(&ClientMessage::SendTacticalPing {
            position: ping_pos,
            ping_type,
        });

        // Spawn local visual effect immediately
        commands.spawn((
            TacticalPingVisual {
                position: ping_pos,
                ping_type,
                color: net_client.my_color,
                lifetime: 0.0,
                max_lifetime: 3.5,
            },
            Transform::from_xyz(ping_pos.x, ping_pos.y, 4.0),
        ));
    }
}

fn draw_tactical_pings_world_system(
    mut commands: Commands,
    time: Res<Time>,
    mut gizmos: Gizmos,
    mut ping_query: Query<(Entity, &mut TacticalPingVisual)>,
) {
    let dt = time.delta_secs();

    for (entity, mut ping) in &mut ping_query {
        ping.lifetime += dt;
        if ping.lifetime >= ping.max_lifetime {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        let progress = ping.lifetime / ping.max_lifetime;
        let alpha = (1.0 - progress).clamp(0.0, 1.0);
        let base_color = ping.ping_type.to_color();

        let ping_color = Color::srgba(
            base_color.to_srgba().red,
            base_color.to_srgba().green,
            base_color.to_srgba().blue,
            alpha,
        );

        let pulse = (ping.lifetime * 4.0).sin().abs();
        let ring_radius = 24.0 + pulse * 28.0;

        // Expanding concentric beacon rings
        gizmos.circle_2d(ping.position, ring_radius, ping_color);
        gizmos.circle_2d(ping.position, ring_radius * 0.5, ping_color);
        gizmos.circle_2d(ping.position, 6.0, Color::WHITE.with_alpha(alpha));

        // Crosshair ticks
        let arm_len = 16.0;
        gizmos.line_2d(
            ping.position + Vec2::new(-arm_len, 0.0),
            ping.position + Vec2::new(arm_len, 0.0),
            ping_color,
        );
        gizmos.line_2d(
            ping.position + Vec2::new(0.0, -arm_len),
            ping.position + Vec2::new(0.0, arm_len),
            ping_color,
        );
    }
}

fn draw_tactical_pings_minimap_system(
    mut gizmos: Gizmos,
    window_query: Query<&Window, With<PrimaryWindow>>,
    minimap_state: Res<MinimapState>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    ping_query: Query<&TacticalPingVisual>,
) {
    let Ok(window) = window_query.get_single() else {
        return;
    };
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);
    let minimap_rect = get_minimap_screen_rect(window, &minimap_state);

    let half_w = window.width() * 0.5;
    let half_h = window.height() * 0.5;

    for ping in &ping_query {
        let screen_pt = world_to_minimap_screen(ping.position, config, &minimap_rect);
        let bevy_ui_pt = Vec2::new(screen_pt.x - half_w, half_h - screen_pt.y);

        let pulse = (ping.lifetime * 6.0).sin().abs();
        let size = 5.0 + pulse * 4.0;
        let color = ping.ping_type.to_color();

        gizmos.rect_2d(
            bevy_ui_pt,
            Vec2::splat(size),
            color,
        );
    }
}
