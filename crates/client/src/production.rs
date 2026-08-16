use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::*;
use shared::economy::PlayerEconomy;
use crate::selection::screen_to_world_2d;

pub struct ProductionPlugin;

impl Plugin for ProductionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                building_construction_system,
                production_queue_system,
                handle_production_hotkeys,
                handle_rally_point_order,
                draw_production_and_construction_visuals,
            ),
        );
    }
}

/// Ticks construction progress for uncompleted buildings and activates them upon completion
fn building_construction_system(
    time: Res<Time>,
    mut economy: ResMut<PlayerEconomy>,
    mut building_query: Query<(
        Entity,
        &mut Building,
        &mut Health,
        &Faction,
        Option<&SupplyDepot>,
    )>,
) {
    let dt = time.delta_secs();

    for (_entity, mut building, mut health, faction, supply_depot_opt) in &mut building_query {
        if !building.is_constructed {
            building.build_timer += dt;
            health.current = (health.max * building.progress()).max(15.0);

            if building.build_timer >= building.build_duration {
                building.is_constructed = true;
                building.build_timer = building.build_duration;
                health.current = health.max;

                // If this is a Supply Depot, grant +8 supply capacity to faction
                if supply_depot_opt.is_some() {
                    economy.add_max_supply(*faction, 8);
                    info!("⚡ [Economy] Supply Depot constructed! Max supply +8 for {:?}", faction);
                }
            }
        }
    }
}

/// Advances production timers on training buildings and spawns completed units
fn production_queue_system(
    mut commands: Commands,
    time: Res<Time>,
    mut prod_query: Query<(
        Entity,
        &mut ProductionBuilding,
        &Building,
        &Transform,
        &Faction,
        &Radius,
    )>,
) {
    let dt = time.delta_secs();

    for (building_entity, mut prod, building, transform, faction, radius) in &mut prod_query {
        if !building.is_constructed || prod.queue.is_empty() {
            continue;
        }

        prod.current_timer += dt;
        let target_duration = prod.queue[0].build_duration;

        if prod.current_timer >= target_duration {
            let completed_unit = prod.queue.remove(0);
            prod.current_timer = 0.0;

            let building_pos = transform.translation.truncate();
            let spawn_offset = Vec2::new(0.0, -radius.0 - 22.0);
            let spawn_pos = building_pos + spawn_offset;

            // Spawn the unit based on name
            let is_worker = completed_unit.name.contains("SCV");
            let net_id = 5000 + (building_entity.index() % 1000) * 10 + (prod.queue.len() as u32);

            if is_worker {
                commands.spawn((
                    Unit {
                        name: "SCV Worker".to_string(),
                        supply_cost: 1,
                    },
                    Worker::default(),
                    Health::new(80.0),
                    Radius(14.0),
                    MoveSpeed(190.0),
                    Velocity::default(),
                    *faction,
                    Selectable::default(),
                    NetEntity {
                        net_id,
                        owner_peer_id: if *faction == Faction::Player1 { 1 } else { 2 },
                    },
                    MoveTarget {
                        destination: prod.rally_point,
                        is_attack_move: false,
                    },
                    Transform::from_xyz(spawn_pos.x, spawn_pos.y, 2.0),
                ));
            } else {
                commands.spawn((
                    Unit {
                        name: "Marine Soldier".to_string(),
                        supply_cost: 2,
                    },
                    Soldier::default(),
                    Health::new(120.0),
                    Radius(16.0),
                    MoveSpeed(180.0),
                    Velocity::default(),
                    *faction,
                    Selectable::default(),
                    NetEntity {
                        net_id,
                        owner_peer_id: if *faction == Faction::Player1 { 1 } else { 2 },
                    },
                    MoveTarget {
                        destination: prod.rally_point,
                        is_attack_move: false,
                    },
                    Transform::from_xyz(spawn_pos.x, spawn_pos.y, 2.0),
                ));
            }

            info!(
                "🎖️ [Production] {} spawned at {:?} and rallying to {:?}",
                completed_unit.name, spawn_pos, prod.rally_point
            );
        }
    }
}

/// Hotkeys for training units when a production building is selected
fn handle_production_hotkeys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut economy: ResMut<PlayerEconomy>,
    mut prod_query: Query<(
        &mut ProductionBuilding,
        &Building,
        &Faction,
        &Selectable,
        Option<&BaseHQ>,
        Option<&Barracks>,
    )>,
) {
    // Key 'V' for SCV Worker at Base HQ
    if keyboard.just_pressed(KeyCode::KeyV) {
        for (mut prod, building, faction, selectable, base_hq, _) in &mut prod_query {
            if *faction == Faction::Player1 && selectable.is_selected && building.is_constructed && base_hq.is_some() {
                if prod.queue.len() < prod.max_queue_size {
                    if economy.has_minerals(*faction, 50) && economy.has_supply(*faction, 1) {
                        economy.spend_minerals(*faction, 50);
                        economy.register_supply(*faction, 1);
                        prod.queue.push(QueuedUnit {
                            name: "SCV Worker".to_string(),
                            mineral_cost: 50,
                            supply_cost: 1,
                            build_duration: 3.0,
                        });
                        info!("⛏️ [Queue] SCV Worker queued! Queue size: {}", prod.queue.len());
                    }
                }
            }
        }
    }

    // Key 'M' for Marine at Barracks
    if keyboard.just_pressed(KeyCode::KeyM) {
        for (mut prod, building, faction, selectable, _, barracks) in &mut prod_query {
            if *faction == Faction::Player1 && selectable.is_selected && building.is_constructed && barracks.is_some() {
                if prod.queue.len() < prod.max_queue_size {
                    if economy.has_minerals(*faction, 100) && economy.has_supply(*faction, 2) {
                        economy.spend_minerals(*faction, 100);
                        economy.register_supply(*faction, 2);
                        prod.queue.push(QueuedUnit {
                            name: "Marine Soldier".to_string(),
                            mineral_cost: 100,
                            supply_cost: 2,
                            build_duration: 4.0,
                        });
                        info!("🔫 [Queue] Marine Soldier queued! Queue size: {}", prod.queue.len());
                    }
                }
            }
        }
    }
}

/// Allows right-clicking to change the rally point of a selected production building
fn handle_rally_point_order(
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>)>,
    mut prod_query: Query<(&mut ProductionBuilding, &Faction, &Selectable), Without<Unit>>,
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
    let target_pos = screen_to_world_2d(cursor_screen, win_size, cam_pos, cam_scale);

    for (mut prod, faction, selectable) in &mut prod_query {
        if *faction == Faction::Player1 && selectable.is_selected {
            prod.rally_point = target_pos;
            info!("📍 [Rally Point] New rally point set to {:?}", target_pos);
        }
    }
}

/// Renders construction progress bars and rally point dashed lines
fn draw_production_and_construction_visuals(
    mut gizmos: Gizmos,
    building_query: Query<(&Transform, &Radius, &Building, &Selectable)>,
    prod_query: Query<(&Transform, &Radius, &ProductionBuilding, &Building, &Selectable)>,
) {
    // 1. Draw Construction Progress Bars & Scaffolding
    for (transform, radius, building, _) in &building_query {
        if !building.is_constructed {
            let pos = transform.translation.truncate();
            let bar_w = radius.0 * 2.0;
            let bar_h = 7.0;
            let bar_y = pos.y + radius.0 + 14.0;

            let bar_center = Vec2::new(pos.x, bar_y);
            let bg_col = Color::srgba(0.08, 0.10, 0.14, 0.85);
            let fill_col = Color::srgb(0.95, 0.75, 0.20); // Amber construction

            // Bar background & border
            gizmos.rect_2d(bar_center, Vec2::new(bar_w, bar_h), bg_col);
            gizmos.rect_2d(bar_center, Vec2::new(bar_w, bar_h), Color::srgba(0.5, 0.5, 0.5, 0.6));

            // Filled progress
            let progress = building.progress();
            let fill_w = bar_w * progress;
            let fill_center = Vec2::new(pos.x - (bar_w - fill_w) * 0.5, bar_y);
            gizmos.rect_2d(fill_center, Vec2::new(fill_w, bar_h - 2.0), fill_col);

            // Construction hazard cross
            let size = Vec2::splat(radius.0 * 1.6);
            gizmos.rect_2d(pos, size, Color::srgba(0.95, 0.75, 0.20, 0.40));
        }
    }

    // 2. Draw Active Production Progress & Rally Lines
    for (transform, radius, prod, building, selectable) in &prod_query {
        let pos = transform.translation.truncate();

        // Production progress bar
        if building.is_constructed && !prod.queue.is_empty() {
            let bar_w = radius.0 * 1.8;
            let bar_h = 6.0;
            let bar_y = pos.y + radius.0 + 8.0;

            let bar_center = Vec2::new(pos.x, bar_y);
            let target_time = prod.queue[0].build_duration;
            let progress = (prod.current_timer / target_time).clamp(0.0, 1.0);

            // Background
            gizmos.rect_2d(bar_center, Vec2::new(bar_w, bar_h), Color::srgba(0.05, 0.05, 0.08, 0.85));
            // Fill (Electric blue)
            let fill_w = bar_w * progress;
            let fill_center = Vec2::new(pos.x - (bar_w - fill_w) * 0.5, bar_y);
            gizmos.rect_2d(fill_center, Vec2::new(fill_w, bar_h - 2.0), Color::srgb(0.25, 0.85, 1.0));
        }

        // Draw Rally Point Line when selected
        if selectable.is_selected {
            let rally_col = Color::srgba(0.95, 0.85, 0.25, 0.85);
            gizmos.line_2d(pos, prod.rally_point, rally_col);
            gizmos.circle_2d(prod.rally_point, 8.0, rally_col);
            gizmos.circle_2d(prod.rally_point, 3.0, Color::srgb(1.0, 1.0, 1.0));
        }
    }
}
