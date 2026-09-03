use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::NavGrid;
use shared::protocol::{ClientMessage, UnitKind};
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::selection::screen_to_world_2d;
use crate::stats::MatchStats;

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
    outcome_opt: Option<Res<MatchOutcome>>,
    mut economy: ResMut<PlayerEconomy>,
    mut building_query: Query<(
        Entity,
        &mut Building,
        &mut Health,
        &Faction,
        Option<&SupplyDepot>,
    )>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory) || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat) {
        return;
    }

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
    outcome_opt: Option<Res<MatchOutcome>>,
    net_client: Option<Res<NetClient>>,
    nav_grid: Res<NavGrid>,
    mut prod_query: Query<(
        Entity,
        &mut ProductionBuilding,
        &Building,
        &Transform,
        &Faction,
        &Radius,
    )>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory) || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat) {
        return;
    }

    let is_online = net_client.as_ref().map(|n| n.status == NetStatus::InGame).unwrap_or(false);
    let dt = time.delta_secs();

    for (building_entity, mut prod, building, transform, faction, radius) in &mut prod_query {
        if !building.is_constructed || prod.queue.is_empty() {
            continue;
        }

        let target_duration = prod.queue[0].build_duration;

        if is_online {
            // In online matches, smoothly advance the client-side timer for visuals,
            // while unit completion and entity spawning is handled authoritatively by the server
            prod.current_timer = (prod.current_timer + dt).min(target_duration);
            continue;
        }

        prod.current_timer += dt;
        if prod.current_timer >= target_duration {
            let completed_unit = prod.queue.remove(0);
            prod.current_timer = 0.0;

            let building_pos = transform.translation.truncate();
            let spawn_offset = Vec2::new(0.0, -radius.0 - 22.0);
            let spawn_pos = building_pos + spawn_offset;
            let rally_waypoints = nav_grid.find_path(spawn_pos, prod.rally_point);

            // Spawn the unit based on name
            let is_worker = completed_unit.name.contains("SCV");
            let is_tank = completed_unit.name.contains("Tank");
            let net_id = 5000 + (building_entity.index() % 1000) * 10 + (prod.queue.len() as u32);

            if is_worker {
                commands.spawn((
                    Unit {
                        name: "SCV Worker".to_string(),
                        supply_cost: 1,
                    },
                    Worker::default(),
                    TacticalStance::default(),
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
                    MoveTarget::with_waypoints(prod.rally_point, false, rally_waypoints),
                    Transform::from_xyz(spawn_pos.x, spawn_pos.y, 2.0),
                ));
            } else if is_tank {
                commands.spawn((
                    Unit {
                        name: "Siege Tank".to_string(),
                        supply_cost: 3,
                    },
                    SiegeTank::default(),
                    TacticalStance::default(),
                    Health::new(220.0),
                    Radius(22.0),
                    MoveSpeed(140.0),
                    Velocity::default(),
                    *faction,
                    Selectable::default(),
                    NetEntity {
                        net_id,
                        owner_peer_id: if *faction == Faction::Player1 { 1 } else { 2 },
                    },
                    MoveTarget::with_waypoints(prod.rally_point, false, rally_waypoints),
                    Transform::from_xyz(spawn_pos.x, spawn_pos.y, 2.0),
                ));
            } else {
                commands.spawn((
                    Unit {
                        name: "Marine Soldier".to_string(),
                        supply_cost: 2,
                    },
                    Soldier::default(),
                    Stimpack::default(),
                    TacticalStance::default(),
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
                    MoveTarget::with_waypoints(prod.rally_point, false, rally_waypoints),
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
    net_client: Res<NetClient>,
    outcome_opt: Option<Res<MatchOutcome>>,
    mut economy: ResMut<PlayerEconomy>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
    mut prod_query: Query<(
        &mut ProductionBuilding,
        &Building,
        &Faction,
        &Selectable,
        Option<&NetEntity>,
        Option<&BaseHQ>,
        Option<&Barracks>,
    )>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory) || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat) {
        return;
    }

    let my_faction = net_client.my_faction;

    // Key 'V' for SCV Worker at Base HQ
    if keyboard.just_pressed(KeyCode::KeyV) {
        for (mut prod, building, faction, selectable, net_entity_opt, base_hq, _) in &mut prod_query {
            if *faction == my_faction && selectable.is_selected && building.is_constructed && base_hq.is_some() {
                if prod.queue.len() < prod.max_queue_size {
                    if !economy.has_minerals(*faction, 50) {
                        info!("⚠️ [Economy] Not enough minerals for SCV Worker (Requires 50 💎)!");
                        continue;
                    }

                    if !economy.has_supply(*faction, 1) {
                        sound_events.send(SoundEffect::SupplyBlocked);
                        info!("⚠️ [Economy] Not enough supply for SCV Worker (Requires 1 ⚡) - Build a Supply Depot [P]!");
                        continue;
                    }

                    economy.spend_minerals(*faction, 50);
                    economy.register_supply(*faction, 1);
                    if *faction == Faction::Player1 {
                        stats.minerals_spent += 50;
                        stats.units_trained += 1;
                    }
                    sound_events.send(SoundEffect::UnitTrained);

                    prod.queue.push(QueuedUnit {
                        name: "SCV Worker".to_string(),
                        mineral_cost: 50,
                        supply_cost: 1,
                        build_duration: 3.0,
                    });

                    if let Some(net) = net_entity_opt {
                        if net_client.status != NetStatus::Disconnected {
                            net_client.send(&ClientMessage::RequestTrainUnit {
                                building_net_id: net.net_id,
                                unit_kind: UnitKind::Worker,
                            });
                        }
                    }

                    info!("⛏️ [Queue] SCV Worker queued! Queue size: {}", prod.queue.len());
                }
            }
        }
    }

    // Key 'M' for Marine at Barracks
    if keyboard.just_pressed(KeyCode::KeyM) {
        for (mut prod, building, faction, selectable, net_entity_opt, _, barracks) in &mut prod_query {
            if *faction == my_faction && selectable.is_selected && building.is_constructed && barracks.is_some() {
                if prod.queue.len() >= prod.max_queue_size {
                    info!("⚠️ [Queue] Production queue is full!");
                    continue;
                }

                if !economy.has_minerals(*faction, 100) {
                    info!("⚠️ [Economy] Not enough minerals for Marine (Requires 100 💎)!");
                    continue;
                }

                if !economy.has_supply(*faction, 2) {
                    sound_events.send(SoundEffect::SupplyBlocked);
                    info!("⚠️ [Economy] Not enough supply for Marine (Requires 2 ⚡) - Build a Supply Depot [P]!");
                    continue;
                }

                economy.spend_minerals(*faction, 100);
                economy.register_supply(*faction, 2);
                if *faction == Faction::Player1 {
                    stats.minerals_spent += 100;
                    stats.units_trained += 1;
                }
                sound_events.send(SoundEffect::UnitTrained);

                prod.queue.push(QueuedUnit {
                    name: "Marine Soldier".to_string(),
                    mineral_cost: 100,
                    supply_cost: 2,
                    build_duration: 4.0,
                });

                if let Some(net) = net_entity_opt {
                    if net_client.status != NetStatus::Disconnected {
                        net_client.send(&ClientMessage::RequestTrainUnit {
                            building_net_id: net.net_id,
                            unit_kind: UnitKind::Soldier,
                        });
                    }
                }

                info!("🔫 [Queue] Marine Soldier queued! Queue size: {}", prod.queue.len());
            }
        }
    }

    // Key 'T', 'S', or 'K' for Siege Tank at Barracks
    if keyboard.just_pressed(KeyCode::KeyT) || keyboard.just_pressed(KeyCode::KeyS) || keyboard.just_pressed(KeyCode::KeyK) {
        for (mut prod, building, faction, selectable, net_entity_opt, _, barracks) in &mut prod_query {
            if *faction == my_faction && selectable.is_selected && building.is_constructed && barracks.is_some() {
                if prod.queue.len() >= prod.max_queue_size {
                    info!("⚠️ [Queue] Production queue is full!");
                    continue;
                }

                if !economy.has_minerals(*faction, 200) {
                    info!("⚠️ [Economy] Not enough minerals for Siege Tank (Requires 200 💎)!");
                    continue;
                }

                if !economy.has_supply(*faction, 3) {
                    sound_events.send(SoundEffect::SupplyBlocked);
                    info!("⚠️ [Economy] Not enough supply for Siege Tank (Requires 3 ⚡) - Build a Supply Depot [P]!");
                    continue;
                }

                economy.spend_minerals(*faction, 200);
                economy.register_supply(*faction, 3);
                if *faction == Faction::Player1 {
                    stats.minerals_spent += 200;
                    stats.units_trained += 1;
                }
                sound_events.send(SoundEffect::UnitTrained);

                prod.queue.push(QueuedUnit {
                    name: "Siege Tank".to_string(),
                    mineral_cost: 200,
                    supply_cost: 3,
                    build_duration: 5.0,
                });

                if let Some(net) = net_entity_opt {
                    if net_client.status != NetStatus::Disconnected {
                        net_client.send(&ClientMessage::RequestTrainUnit {
                            building_net_id: net.net_id,
                            unit_kind: UnitKind::Tank,
                        });
                    }
                }

                info!("🛡️ [Queue] Siege Tank queued! Queue size: {}", prod.queue.len());
            }
        }
    }
}


/// Allows right-clicking to change the rally point of a selected production building
fn handle_rally_point_order(
    mouse_button: Res<ButtonInput<MouseButton>>,
    net_client: Res<NetClient>,
    outcome_opt: Option<Res<MatchOutcome>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>)>,
    mut prod_query: Query<(&mut ProductionBuilding, &Faction, &Selectable, Option<&NetEntity>), Without<Unit>>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory) || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat) {
        return;
    }

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
    let my_faction = net_client.my_faction;

    for (mut prod, faction, selectable, net_entity_opt) in &mut prod_query {
        if *faction == my_faction && selectable.is_selected {
            prod.rally_point = target_world_pos;
            if let Some(net) = net_entity_opt {
                if net_client.status != NetStatus::Disconnected {
                    net_client.send(&ClientMessage::RequestSetRallyPoint {
                        building_net_id: net.net_id,
                        rally_position: target_world_pos,
                    });
                }
            }
            info!("📍 [Rally Point] Production rally updated to {:?}", target_world_pos);
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
