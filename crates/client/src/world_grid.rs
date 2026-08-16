use bevy::prelude::*;
use shared::grid::WorldGridConfig;

pub struct WorldGridPlugin;

impl Plugin for WorldGridPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldGridConfig>()
            .add_systems(Startup, setup_ground_mesh)
            .add_systems(Update, draw_grid_lines_system);
    }
}

/// Spawns the ground quad background
fn setup_ground_mesh(mut commands: Commands, grid_config: Res<WorldGridConfig>) {
    let size = Vec2::new(grid_config.width(), grid_config.height());
    let center = (grid_config.min_bounds + grid_config.max_bounds) * 0.5;

    // Dark tactical RTS terrain
    commands.spawn((
        Sprite {
            color: Color::srgb(0.08, 0.11, 0.10),
            custom_size: Some(size),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, -10.0),
    ));
}

/// Procedurally renders minor & major grid lines and borders with Bevy Gizmos
fn draw_grid_lines_system(mut gizmos: Gizmos, grid_config: Res<WorldGridConfig>) {
    let minor_color = Color::srgba(0.18, 0.24, 0.22, 0.45);
    let major_color = Color::srgba(0.28, 0.38, 0.33, 0.85);
    let border_color = Color::srgba(0.35, 0.65, 0.50, 0.95);
    let center_color = Color::srgba(0.45, 0.80, 0.65, 0.90);

    let min_x = grid_config.min_bounds.x;
    let max_x = grid_config.max_bounds.x;
    let min_y = grid_config.min_bounds.y;
    let max_y = grid_config.max_bounds.y;
    let cell_size = grid_config.cell_size;
    let major_interval = grid_config.major_interval as f32 * cell_size;

    // Vertical lines
    let mut x = min_x;
    while x <= max_x {
        let is_major = ((x - min_x).abs() % major_interval).abs() < 1.0;
        let col = if is_major { major_color } else { minor_color };
        gizmos.line_2d(Vec2::new(x, min_y), Vec2::new(x, max_y), col);
        x += cell_size;
    }

    // Horizontal lines
    let mut y = min_y;
    while y <= max_y {
        let is_major = ((y - min_y).abs() % major_interval).abs() < 1.0;
        let col = if is_major { major_color } else { minor_color };
        gizmos.line_2d(Vec2::new(min_x, y), Vec2::new(max_x, y), col);
        y += cell_size;
    }

    // Center Origin Crosshair
    let cross_size = 48.0;
    gizmos.line_2d(Vec2::new(-cross_size, 0.0), Vec2::new(cross_size, 0.0), center_color);
    gizmos.line_2d(Vec2::new(0.0, -cross_size), Vec2::new(0.0, cross_size), center_color);
    gizmos.circle_2d(Vec2::ZERO, 8.0, center_color);

    // Map Boundary Outline (Rect2d)
    let center = (grid_config.min_bounds + grid_config.max_bounds) * 0.5;
    let size = Vec2::new(grid_config.width(), grid_config.height());
    gizmos.rect_2d(center, size, border_color);
}
