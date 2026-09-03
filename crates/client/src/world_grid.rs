use bevy::prelude::*;
use shared::grid::WorldGridConfig;
use shared::map::{ObstacleKind, STATIC_MAP_OBSTACLES};
use crate::fog_of_war::{FogOfWarGrid, FogState};

pub struct WorldGridPlugin;

impl Plugin for WorldGridPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldGridConfig>()
            .add_systems(Startup, setup_ground_mesh)
            .add_systems(
                Update,
                (
                    draw_grid_lines_system,
                    draw_map_terrain_and_obstacles_system,
                ),
            );
    }
}

/// Spawns the ground quad background
fn setup_ground_mesh(mut commands: Commands, grid_config: Res<WorldGridConfig>) {
    let size = Vec2::new(grid_config.width(), grid_config.height());
    let center = (grid_config.min_bounds + grid_config.max_bounds) * 0.5;

    // Dark tactical RTS basalt terrain
    commands.spawn((
        Sprite {
            color: Color::srgb(0.07, 0.09, 0.10),
            custom_size: Some(size),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, -10.0),
    ));
}

/// Procedurally renders minor & major grid lines and borders with Bevy Gizmos
fn draw_grid_lines_system(mut gizmos: Gizmos, grid_config: Res<WorldGridConfig>) {
    let minor_color = Color::srgba(0.16, 0.22, 0.20, 0.35);
    let major_color = Color::srgba(0.25, 0.35, 0.30, 0.70);
    let border_color = Color::srgba(0.35, 0.65, 0.50, 0.95);
    let center_color = Color::srgba(0.45, 0.80, 0.65, 0.80);

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

    // Center Origin Tactical Crosshair
    let cross_size = 64.0;
    gizmos.line_2d(Vec2::new(-cross_size, 0.0), Vec2::new(cross_size, 0.0), center_color);
    gizmos.line_2d(Vec2::new(0.0, -cross_size), Vec2::new(0.0, cross_size), center_color);
    gizmos.circle_2d(Vec2::ZERO, 10.0, center_color);
    gizmos.circle_2d(Vec2::ZERO, 28.0, center_color.with_alpha(0.40));

    // Map Boundary Outline (Rect2d)
    let center = (grid_config.min_bounds + grid_config.max_bounds) * 0.5;
    let size = Vec2::new(grid_config.width(), grid_config.height());
    gizmos.rect_2d(center, size, border_color);
}

/// Procedurally renders rock monoliths, cliff ridges, base ramps, and expansion plateaus
fn draw_map_terrain_and_obstacles_system(
    mut gizmos: Gizmos,
    fog: Res<FogOfWarGrid>,
    grid_cfg: Option<Res<WorldGridConfig>>,
) {
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);

    // ─────────────────────────────────────────────────────────────────────────
    // 1. STATIC MAP OBSTACLES (ROCKS, CLIFF BLUFFS, MOUNTAIN RIDGES)
    // ─────────────────────────────────────────────────────────────────────────
    let base_rock_fill = Color::srgba(0.11, 0.15, 0.18, 0.95);
    let inner_rock_ring = Color::srgba(0.18, 0.24, 0.29, 0.85);
    let rock_outline = Color::srgba(0.38, 0.50, 0.60, 0.90);
    let vein_glow = Color::srgba(0.20, 0.75, 0.95, 0.75);
    let bluff_ridge = Color::srgba(0.45, 0.55, 0.62, 0.80);

    for obs in STATIC_MAP_OBSTACLES {
        let pos = obs.position;
        let r = obs.radius;

        // Skip detailed rendering if shrouded in unexplored fog
        if fog.get_state_at_world_pos(pos, config) == FogState::Unexplored {
            continue;
        }

        // Foundation & Depth Shadows
        gizmos.circle_2d(pos, r, base_rock_fill);
        gizmos.circle_2d(pos, r - 5.0, inner_rock_ring);
        gizmos.circle_2d(pos, r * 0.6, inner_rock_ring.lighter(0.08));
        gizmos.circle_2d(pos, r, rock_outline);

        match obs.kind {
            ObstacleKind::RockMonolith => {
                // Central High-Density Basalt Monolith with Glowing Crystal Veins
                let d = r * 0.45;
                gizmos.line_2d(pos + Vec2::new(-d, -d), pos + Vec2::new(d, d), vein_glow);
                gizmos.line_2d(pos + Vec2::new(-d, d), pos + Vec2::new(d, -d), vein_glow);
                gizmos.circle_2d(pos, 8.0, vein_glow);
                gizmos.rect_2d(pos, Vec2::splat(12.0), rock_outline);
            }
            ObstacleKind::BaseRampBluff => {
                // Natural base ramp bluffs with layered cliff facets
                let d = r * 0.55;
                gizmos.line_2d(pos + Vec2::new(-d, 0.0), pos + Vec2::new(d, 0.0), bluff_ridge);
                gizmos.line_2d(pos + Vec2::new(0.0, -d), pos + Vec2::new(0.0, d), bluff_ridge);
                gizmos.circle_2d(pos + Vec2::new(d * 0.4, d * 0.4), 6.0, bluff_ridge);
                gizmos.circle_2d(pos + Vec2::new(-d * 0.4, -d * 0.4), 6.0, bluff_ridge);
            }
            ObstacleKind::CliffRidge => {
                // Outer mountain ridgelines with jagged diagonal facets
                let d = r * 0.50;
                gizmos.line_2d(pos + Vec2::new(-d, -d * 0.5), pos + Vec2::new(d, d * 0.5), bluff_ridge);
                gizmos.line_2d(pos + Vec2::new(-d * 0.5, d), pos + Vec2::new(d * 0.5, -d), bluff_ridge);
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 2. TACTICAL CHOKEPOINTS & BASE RAMP ENTRANCE GUIDES
    // ─────────────────────────────────────────────────────────────────────────
    let south_ramp = Vec2::new(0.0, -720.0);
    let north_ramp = Vec2::new(0.0, 720.0);
    let ramp_line_col = Color::srgba(0.20, 0.50, 0.70, 0.45);

    // South Ramp entrance lines
    gizmos.line_2d(south_ramp + Vec2::new(-130.0, -10.0), south_ramp + Vec2::new(130.0, -10.0), ramp_line_col);
    gizmos.line_2d(south_ramp + Vec2::new(-130.0, 10.0), south_ramp + Vec2::new(130.0, 10.0), ramp_line_col);
    gizmos.circle_2d(south_ramp, 16.0, ramp_line_col);

    // North Ramp entrance lines
    gizmos.line_2d(north_ramp + Vec2::new(-130.0, -10.0), north_ramp + Vec2::new(130.0, -10.0), ramp_line_col);
    gizmos.line_2d(north_ramp + Vec2::new(-130.0, 10.0), north_ramp + Vec2::new(130.0, 10.0), ramp_line_col);
    gizmos.circle_2d(north_ramp, 16.0, ramp_line_col);

    // ─────────────────────────────────────────────────────────────────────────
    // 3. EXPANSION BASE PLATEAU PERIMETERS
    // ─────────────────────────────────────────────────────────────────────────
    let plateau_col = Color::srgba(0.25, 0.60, 0.50, 0.30);
    let expansion_sites = [
        Vec2::new(750.0, -650.0),   // South Natural Exp
        Vec2::new(-750.0, 650.0),   // North Natural Exp
        Vec2::new(-1100.0, 0.0),    // Contested West Exp
        Vec2::new(1100.0, 0.0),     // Contested East Exp
    ];

    for exp_pos in expansion_sites {
        if fog.get_state_at_world_pos(exp_pos, config) != FogState::Unexplored {
            gizmos.circle_2d(exp_pos, 75.0, plateau_col);
            gizmos.circle_2d(exp_pos, 20.0, plateau_col);
            gizmos.line_2d(exp_pos + Vec2::new(-25.0, 0.0), exp_pos + Vec2::new(25.0, 0.0), plateau_col);
            gizmos.line_2d(exp_pos + Vec2::new(0.0, -25.0), exp_pos + Vec2::new(0.0, 25.0), plateau_col);
        }
    }
}
