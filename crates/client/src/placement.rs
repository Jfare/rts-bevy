use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::BuildingKind;
use shared::protocol::ClientMessage;
use crate::net::{NetClient, NetStatus};
use crate::selection::screen_to_world_2d;

/// Active placement state when the player is positioning a new building
#[derive(Debug, Resource, Default)]
pub struct PlacementState {
    pub active_kind: Option<BuildingKind>,
    pub ghost_pos: Vec2,
    pub is_valid: bool,
    pub mineral_cost: u32,
}

pub struct PlacementPlugin;

impl Plugin for PlacementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlacementState>()
            .add_systems(
                Update,
                (
                    handle_placement_input,
                    update_placement_validation,
                    draw_placement_ghost,
                ),
            );
    }
}

/// Handles hotkeys and mouse clicks for building placement
fn handle_placement_input(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut net_client: ResMut<NetClient>,
    mut economy: ResMut<PlayerEconomy>,
    mut state: ResMut<PlacementState>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>)>,
) {

    // 1. Hotkeys to enter placement mode
    if keyboard.just_pressed(KeyCode::KeyB) {
        state.active_kind = Some(BuildingKind::Barracks);
        state.mineral_cost = BuildingKind::Barracks.mineral_cost();
        info!("🏗️ [Build Mode] Barracks ($150) selected for placement");
    }
    if keyboard.just_pressed(KeyCode::KeyP) {
        state.active_kind = Some(BuildingKind::SupplyDepot);
        state.mineral_cost = BuildingKind::SupplyDepot.mineral_cost();
        info!("🏗️ [Build Mode] Supply Depot ($100) selected for placement");
    }
    if keyboard.just_pressed(KeyCode::KeyH) {
        state.active_kind = Some(BuildingKind::BaseHQ);
        state.mineral_cost = BuildingKind::BaseHQ.mineral_cost();
        info!("🏗️ [Build Mode] Base HQ ($400) selected for placement");
    }

    // 2. Cancel placement on Escape or Right-Click
    if keyboard.just_pressed(KeyCode::Escape) || mouse_button.just_pressed(MouseButton::Right) {
        if state.active_kind.is_some() {
            state.active_kind = None;
            info!("❌ [Build Mode] Placement cancelled");
            return;
        }
    }

    let Some(building_kind) = state.active_kind else {
        return;
    };

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
    let raw_world_pos = screen_to_world_2d(cursor_screen, win_size, cam_pos, cam_scale);

    // Snap to 16.0 pixel grid
    let snap_pos = (raw_world_pos / 16.0).round() * 16.0;
    state.ghost_pos = snap_pos;

    // 3. Confirm placement on Left-Click
    if mouse_button.just_pressed(MouseButton::Left) && state.is_valid {
        if economy.spend_minerals(Faction::Player1, state.mineral_cost) {
            let spawn_pos = state.ghost_pos;

            if net_client.status != NetStatus::Disconnected {
                net_client.send(&ClientMessage::RequestBuild {
                    building_kind,
                    position: spawn_pos,
                });
            }


            let size = building_kind.size();
            let duration = building_kind.build_duration();
            let max_hp = building_kind.max_health();


            let radius = match building_kind {
                BuildingKind::BaseHQ => 55.0,
                BuildingKind::Barracks => 46.0,
                BuildingKind::SupplyDepot => 30.0,
            };

            let mut entity_cmds = commands.spawn((
                Building::new(building_kind.name(), size, duration, false),
                Health::new(max_hp),
                Faction::Player1,
                Selectable::default(),
                Radius(radius),
                Transform::from_xyz(spawn_pos.x, spawn_pos.y, 1.0),
            ));

            match building_kind {
                BuildingKind::BaseHQ => {
                    entity_cmds.insert((
                        BaseHQ {
                            supply_provided: 10,
                            dropoff_radius: 70.0,
                        },
                        ProductionBuilding {
                            queue: Vec::new(),
                            current_timer: 0.0,
                            max_queue_size: 5,
                            rally_point: spawn_pos + Vec2::new(0.0, -100.0),
                        },
                    ));
                }
                BuildingKind::Barracks => {
                    entity_cmds.insert((
                        Barracks,
                        ProductionBuilding {
                            queue: Vec::new(),
                            current_timer: 0.0,
                            max_queue_size: 5,
                            rally_point: spawn_pos + Vec2::new(0.0, -90.0),
                        },
                    ));
                }
                BuildingKind::SupplyDepot => {
                    entity_cmds.insert(SupplyDepot {
                        supply_provided: 8,
                    });
                }
            }

            info!(
                "🏗️ [Build] Placed {} at {:?}! Construction started.",
                building_kind.name(),
                spawn_pos
            );

            // If Shift is NOT held, exit placement mode
            let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
            if !shift_held {
                state.active_kind = None;
            }
        }
    }
}

/// Validates ghost placement against map boundaries, collisions, and mineral costs
fn update_placement_validation(
    economy: Res<PlayerEconomy>,
    mut state: ResMut<PlacementState>,
    building_query: Query<(&Transform, &Radius, &Building)>,
    resource_query: Query<(&Transform, &Radius), With<ResourceNode>>,
    unit_query: Query<(&Transform, &Radius), With<Unit>>,
) {
    let Some(building_kind) = state.active_kind else {
        state.is_valid = false;
        return;
    };

    // 1. Check Mineral Funds
    if !economy.has_minerals(Faction::Player1, state.mineral_cost) {
        state.is_valid = false;
        return;
    }

    let ghost_pos = state.ghost_pos;
    let size = building_kind.size();
    let ghost_radius = match building_kind {
        BuildingKind::BaseHQ => 55.0,
        BuildingKind::Barracks => 46.0,
        BuildingKind::SupplyDepot => 30.0,
    };

    // 2. Check Map Boundaries (-1500 .. 1500)
    if ghost_pos.x - size.x * 0.5 < -1500.0
        || ghost_pos.x + size.x * 0.5 > 1500.0
        || ghost_pos.y - size.y * 0.5 < -1500.0
        || ghost_pos.y + size.y * 0.5 > 1500.0
    {
        state.is_valid = false;
        return;
    }

    // 3. Check Overlap with Existing Buildings
    for (transform, radius, _) in &building_query {
        let b_pos = transform.translation.truncate();
        let min_dist = ghost_radius + radius.0 + 8.0;
        if ghost_pos.distance(b_pos) < min_dist {
            state.is_valid = false;
            return;
        }
    }

    // 4. Check Overlap with Resource Nodes
    for (transform, radius) in &resource_query {
        let r_pos = transform.translation.truncate();
        let min_dist = ghost_radius + radius.0 + 16.0;
        if ghost_pos.distance(r_pos) < min_dist {
            state.is_valid = false;
            return;
        }
    }

    // 5. Check Overlap with Units
    for (transform, radius) in &unit_query {
        let u_pos = transform.translation.truncate();
        let min_dist = ghost_radius + radius.0;
        if ghost_pos.distance(u_pos) < min_dist {
            state.is_valid = false;
            return;
        }
    }

    state.is_valid = true;
}

/// Renders the ghost building preview and snapping grid indicators
fn draw_placement_ghost(mut gizmos: Gizmos, state: Res<PlacementState>) {
    let Some(building_kind) = state.active_kind else {
        return;
    };

    let pos = state.ghost_pos;
    let size = building_kind.size();

    let (border_col, fill_col) = if state.is_valid {
        (
            Color::srgba(0.20, 0.95, 0.45, 0.95), // Green
            Color::srgba(0.20, 0.95, 0.45, 0.35),
        )
    } else {
        (
            Color::srgba(0.95, 0.25, 0.25, 0.95), // Red
            Color::srgba(0.95, 0.25, 0.25, 0.35),
        )
    };

    // Ghost rectangle
    gizmos.rect_2d(pos, size, border_col);
    gizmos.rect_2d(pos, (size - Vec2::splat(4.0)).max(Vec2::ZERO), fill_col);

    // Diagonal construction cross
    let half = size * 0.5;
    gizmos.line_2d(pos - half, pos + half, border_col.with_alpha(0.6));
    gizmos.line_2d(
        pos + Vec2::new(-half.x, half.y),
        pos + Vec2::new(half.x, -half.y),
        border_col.with_alpha(0.6),
    );

    // 16px Grid alignment crosshair
    gizmos.line_2d(
        pos + Vec2::new(-size.x, 0.0),
        pos + Vec2::new(size.x, 0.0),
        Color::srgba(1.0, 1.0, 1.0, 0.2),
    );
    gizmos.line_2d(
        pos + Vec2::new(0.0, -size.y),
        pos + Vec2::new(0.0, size.y),
        Color::srgba(1.0, 1.0, 1.0, 0.2),
    );
}
