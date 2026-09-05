use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::AppState;
use shared::grid::WorldGridConfig;

/// Component marker for the RTS 2D camera
#[derive(Component, Reflect)]
pub struct RtsCamera {
    pub pan_speed: f32,
    pub edge_margin: f32,
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub target_zoom: f32,
    pub zoom_speed: f32,
}

impl Default for RtsCamera {
    fn default() -> Self {
        Self {
            pan_speed: 900.0,
            edge_margin: 20.0,
            min_zoom: 0.35,
            max_zoom: 2.5,
            target_zoom: 1.0,
            zoom_speed: 12.0,
        }
    }
}

pub struct RtsCameraPlugin;

impl Plugin for RtsCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (camera_pan_system, camera_zoom_system).run_if(in_state(AppState::InGame)),
        );
    }
}

/// System for panning the camera using WASD, Arrow keys, or screen edge boundaries
fn camera_pan_system(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    grid_config: Option<Res<WorldGridConfig>>,
    mut camera_query: Query<(&RtsCamera, &mut Transform, Option<&OrthographicProjection>)>,
) {
    let Ok((rts_cam, mut transform, ortho_opt)) = camera_query.get_single_mut() else {
        return;
    };
    let Ok(window) = window_query.get_single() else {
        return;
    };

    let mut pan_direction = Vec2::ZERO;

    // 1. Keyboard Panning (WASD / Arrows)
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        pan_direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        pan_direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        pan_direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        pan_direction.x += 1.0;
    }

    // 2. Mouse Edge Panning (Only if cursor is within window bounds)
    if let Some(cursor_pos) = window.cursor_position() {
        let win_w = window.width();
        let win_h = window.height();

        if cursor_pos.x >= 0.0 && cursor_pos.x <= win_w && cursor_pos.y >= 0.0 && cursor_pos.y <= win_h {
            if cursor_pos.x <= rts_cam.edge_margin {
                pan_direction.x -= 1.0;
            } else if cursor_pos.x >= win_w - rts_cam.edge_margin {
                pan_direction.x += 1.0;
            }

            // Window coordinates have (0,0) at top-left
            if cursor_pos.y <= rts_cam.edge_margin {
                pan_direction.y += 1.0;
            } else if cursor_pos.y >= win_h - rts_cam.edge_margin {
                pan_direction.y -= 1.0;
            }
        }
    }

    if pan_direction.length_squared() > 0.0 {
        pan_direction = pan_direction.normalize();
        let scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);
        let speed = rts_cam.pan_speed * scale;
        let delta_pos = pan_direction * speed * time.delta_secs();
        transform.translation.x += delta_pos.x;
        transform.translation.y += delta_pos.y;
    }

    // 3. Map boundary clamping
    let default_config = WorldGridConfig::default();
    let config = grid_config.as_deref().unwrap_or(&default_config);
    let padding = 100.0;
    transform.translation.x = transform
        .translation
        .x
        .clamp(config.min_bounds.x + padding, config.max_bounds.x - padding);
    transform.translation.y = transform
        .translation
        .y
        .clamp(config.min_bounds.y + padding, config.max_bounds.y - padding);
}

/// System for smooth zoom scaling with mouse wheel
fn camera_zoom_system(
    time: Res<Time>,
    mut mouse_wheel_events: EventReader<MouseWheel>,
    mut camera_query: Query<(&mut RtsCamera, Option<&mut OrthographicProjection>)>,
) {
    let Ok((mut rts_cam, mut ortho_opt)) = camera_query.get_single_mut() else {
        return;
    };

    for event in mouse_wheel_events.read() {
        let zoom_delta = -event.y * 0.15;
        rts_cam.target_zoom = (rts_cam.target_zoom + zoom_delta).clamp(rts_cam.min_zoom, rts_cam.max_zoom);
    }

    // Smooth interpolation towards target zoom
    if let Some(ref mut ortho) = ortho_opt {
        let dt = time.delta_secs();
        ortho.scale = ortho.scale.lerp(rts_cam.target_zoom, dt * rts_cam.zoom_speed);
    }
}
