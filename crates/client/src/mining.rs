use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::protocol::ClientMessage;
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::selection::screen_to_world_2d;
use crate::stats::MatchStats;

pub struct MiningPlugin;

impl Plugin for MiningPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_mining_click_orders,
                worker_mining_state_machine,
                draw_mining_visuals,
            ),
        );
    }
}

/// Contextual right-click handler: If right-clicking a mineral node with SCV workers selected, start mining!
fn handle_mining_click_orders(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    net_client: Res<NetClient>,
    outcome_opt: Option<Res<MatchOutcome>>,
    window_query: Query<&Window, With<PrimaryWindow>>,

    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>), With<Camera>>,
    node_query: Query<(Entity, &Transform, &Radius, &ResourceNode, Option<&NetEntity>), With<ResourceNode>>,
    mut worker_query: Query<(Entity, &Faction, &Selectable, &mut Worker, Option<&NetEntity>), (With<Worker>, Without<ResourceNode>)>,
    mut sound_events: EventWriter<SoundEffect>,
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
    let click_pos = screen_to_world_2d(cursor_screen, win_size, cam_pos, cam_scale);

    // Check if clicked on an active ResourceNode
    let mut clicked_node = None;
    for (node_entity, node_transform, radius, resource_node, net_opt) in &node_query {
        let node_pos = node_transform.translation.truncate();
        if click_pos.distance(node_pos) <= (radius.0 + 20.0) && resource_node.remaining_minerals > 0 {
            clicked_node = Some((node_entity, net_opt.map(|n| n.net_id)));
            break;
        }
    }

    let Some((target_node_entity, target_net_id_opt)) = clicked_node else {
        return;
    };

    let mut worker_net_ids = Vec::new();

    for (worker_entity, faction, selectable, mut worker, net_opt) in &mut worker_query {
        if selectable.is_selected && *faction == Faction::Player1 {
            worker.target_node = Some(target_node_entity);
            worker.state = WorkerState::MovingToResource;
            worker.harvest_timer = 0.0;
            commands.entity(worker_entity).remove::<MoveTarget>();

            if let Some(net) = net_opt {
                worker_net_ids.push(net.net_id);
            }
        }
    }

    if !worker_net_ids.is_empty() {
        sound_events.send(SoundEffect::OrderIssued);
        if net_client.status != NetStatus::Disconnected {
            if let Some(resource_net_id) = target_net_id_opt {
                net_client.send(&ClientMessage::RequestHarvest {
                    worker_net_ids,
                    resource_net_id,
                });
            }
        }
    }
}

/// SCV Worker Mining State Machine
fn worker_mining_state_machine(
    mut commands: Commands,
    time: Res<Time>,
    mut economy: ResMut<PlayerEconomy>,
    mut stats: ResMut<MatchStats>,
    mut worker_query: Query<(
        Entity,
        &mut Worker,
        &mut Transform,
        &MoveSpeed,
        &Faction,
        Option<&MoveTarget>,
    ), (With<Worker>, Without<ResourceNode>, Without<BaseHQ>)>,
    mut node_query: Query<(Entity, &Transform, &mut ResourceNode), (With<ResourceNode>, Without<Worker>, Without<BaseHQ>)>,
    base_query: Query<(Entity, &Transform, &Faction, &Building, &BaseHQ), (With<BaseHQ>, Without<Worker>, Without<ResourceNode>)>,
    mut sound_events: EventWriter<SoundEffect>,
) {
    let dt = time.delta_secs();

    for (worker_entity, mut worker, mut worker_transform, move_speed, faction, move_target_opt) in &mut worker_query {
        // If worker is active in mining loop, ensure ground MoveTarget is removed
        if worker.state != WorkerState::Idle && move_target_opt.is_some() {
            commands.entity(worker_entity).remove::<MoveTarget>();
        }

        match worker.state {
            WorkerState::Idle => {
                // Do nothing
            }

            WorkerState::MovingToResource => {
                let Some(node_entity) = worker.target_node else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                let Ok((_, node_transform, node)) = node_query.get_mut(node_entity) else {
                    // Node is gone or despawned
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if node.remaining_minerals == 0 {
                    // Node depleted, find another closest node
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                    continue;
                }

                let worker_pos = worker_transform.translation.truncate();
                let node_pos = node_transform.translation.truncate();
                let dist = worker_pos.distance(node_pos);

                if dist <= worker.interact_distance {
                    // Arrived at mineral patch, begin mining
                    worker.state = WorkerState::Mining;
                    worker.harvest_timer = 0.0;
                } else {
                    // Move towards node
                    let dir = (node_pos - worker_pos).normalize_or_zero();
                    let step = dir * move_speed.0 * dt;
                    worker_transform.translation.x += step.x;
                    worker_transform.translation.y += step.y;

                    // Face node
                    if dir.length_squared() > 0.0 {
                        let angle = dir.y.atan2(dir.x);
                        worker_transform.rotation = Quat::from_rotation_z(angle);
                    }
                }
            }

            WorkerState::Mining => {
                let Some(node_entity) = worker.target_node else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                let Ok((_, _, mut node)) = node_query.get_mut(node_entity) else {
                    worker.state = WorkerState::MovingToBase;
                    continue;
                };

                worker.harvest_timer += dt;

                if worker.harvest_timer >= worker.harvest_duration {
                    // Harvest minerals
                    let harvested = node.harvest(worker.harvest_capacity);
                    worker.carried_minerals = harvested;
                    worker.harvest_timer = 0.0;
                    worker.state = WorkerState::MovingToBase;
                    sound_events.send(SoundEffect::LaserMining);

                    // Find nearest friendly Base HQ
                    let worker_pos = worker_transform.translation.truncate();
                    let mut nearest_base = None;
                    let mut nearest_dist = f32::MAX;

                    for (base_entity, base_transform, base_faction, building, _) in &base_query {
                        if *base_faction == *faction && building.is_constructed {
                            let base_pos = base_transform.translation.truncate();
                            let d = worker_pos.distance(base_pos);
                            if d < nearest_dist {
                                nearest_dist = d;
                                nearest_base = Some(base_entity);
                            }
                        }
                    }
                    worker.target_base = nearest_base;
                }
            }

            WorkerState::MovingToBase => {
                let worker_pos = worker_transform.translation.truncate();

                // Check if target base is still valid, else search for closest one
                let mut target_base_pos = None;
                if let Some(base_entity) = worker.target_base {
                    if let Ok((_, base_transform, base_faction, building, _)) = base_query.get(base_entity) {
                        if *base_faction == *faction && building.is_constructed {
                            target_base_pos = Some(base_transform.translation.truncate());
                        }
                    }
                }

                if target_base_pos.is_none() {
                    let mut nearest_base = None;
                    let mut nearest_dist = f32::MAX;
                    for (b_ent, b_trans, b_fac, building, _) in &base_query {
                        if *b_fac == *faction && building.is_constructed {
                            let d = worker_pos.distance(b_trans.translation.truncate());
                            if d < nearest_dist {
                                nearest_dist = d;
                                nearest_base = Some((b_ent, b_trans.translation.truncate()));
                            }
                        }
                    }
                    if let Some((b_ent, b_pos)) = nearest_base {
                        worker.target_base = Some(b_ent);
                        target_base_pos = Some(b_pos);
                    }
                }

                let Some(base_pos) = target_base_pos else {
                    // No base available, idle
                    worker.state = WorkerState::Idle;
                    continue;
                };

                let dist = worker_pos.distance(base_pos);
                if dist <= worker.base_interact_distance {
                    // Deposit minerals into economy!
                    if worker.carried_minerals > 0 {
                        economy.add_minerals(*faction, worker.carried_minerals);
                        if *faction == Faction::Player1 {
                            stats.minerals_mined += worker.carried_minerals;
                        }
                        info!("💎 [Mining] Worker deposited {} minerals for {:?}! New Bank Total: {}", worker.carried_minerals, faction, economy.get_minerals(*faction));
                        worker.carried_minerals = 0;
                    }

                    // If original mineral patch still has minerals, return to it!
                    if let Some(node_entity) = worker.target_node {
                        if let Ok((_, _, node)) = node_query.get(node_entity) {
                            if node.remaining_minerals > 0 {
                                worker.state = WorkerState::MovingToResource;
                                continue;
                            }
                        }
                    }


                    // Otherwise try to find another mineral patch
                    let mut closest_node = None;
                    let mut closest_dist = f32::MAX;
                    for (n_ent, n_trans, n) in &node_query {
                        if n.remaining_minerals > 0 {
                            let d = worker_pos.distance(n_trans.translation.truncate());
                            if d < closest_dist {
                                closest_dist = d;
                                closest_node = Some(n_ent);
                            }
                        }
                    }

                    if let Some(next_node) = closest_node {
                        worker.target_node = Some(next_node);
                        worker.state = WorkerState::MovingToResource;
                    } else {
                        worker.target_node = None;
                        worker.state = WorkerState::Idle;
                    }
                } else {
                    // Move towards Base HQ
                    let dir = (base_pos - worker_pos).normalize_or_zero();
                    let step = dir * move_speed.0 * dt;
                    worker_transform.translation.x += step.x;
                    worker_transform.translation.y += step.y;

                    // Face base
                    if dir.length_squared() > 0.0 {
                        let angle = dir.y.atan2(dir.x);
                        worker_transform.rotation = Quat::from_rotation_z(angle);
                    }
                }
            }
        }
    }
}

/// Renders the pulsating cyan mining laser and carried mineral crystal
fn draw_mining_visuals(
    time: Res<Time>,
    mut gizmos: Gizmos,
    worker_query: Query<(&Transform, &Worker)>,
    node_query: Query<(Entity, &Transform, &ResourceNode)>,
) {
    let t = time.elapsed_secs();

    for (worker_transform, worker) in &worker_query {
        let worker_pos = worker_transform.translation.truncate();

        // 1. Draw Cyan Mining Laser
        if worker.state == WorkerState::Mining {
            if let Some(node_entity) = worker.target_node {
                if let Ok((_, node_transform, _)) = node_query.get(node_entity) {
                    let node_pos = node_transform.translation.truncate();

                    // Laser core
                    let pulse = (t * 20.0).sin() * 0.2 + 0.8;
                    let laser_core = Color::srgba(0.35, 0.90, 1.0, 0.95 * pulse);
                    let laser_glow = Color::srgba(0.15, 0.60, 1.0, 0.40 * pulse);

                    gizmos.line_2d(worker_pos, node_pos, laser_core);
                    gizmos.line_2d(worker_pos + Vec2::new(1.0, 1.0), node_pos + Vec2::new(1.0, 1.0), laser_glow);
                    gizmos.line_2d(worker_pos - Vec2::new(1.0, 1.0), node_pos - Vec2::new(1.0, 1.0), laser_glow);

                    // Sparks at impact point
                    let spark_offset = Vec2::new((t * 25.0).cos() * 6.0, (t * 25.0).sin() * 6.0);
                    gizmos.circle_2d(node_pos + spark_offset, 3.5, Color::srgba(0.8, 1.0, 1.0, 0.9));
                }
            }
        }

        // 2. Draw Carried Mineral Diamond on SCV Worker
        if worker.carried_minerals > 0 {
            let diamond_center = worker_pos + Vec2::new(0.0, 18.0);
            let diamond_col = Color::srgb(0.20, 0.95, 1.0);
            let d_top = diamond_center + Vec2::new(0.0, 7.0);
            let d_bottom = diamond_center + Vec2::new(0.0, -7.0);
            let d_left = diamond_center + Vec2::new(-5.0, 0.0);
            let d_right = diamond_center + Vec2::new(5.0, 0.0);

            gizmos.line_2d(d_top, d_right, diamond_col);
            gizmos.line_2d(d_right, d_bottom, diamond_col);
            gizmos.line_2d(d_bottom, d_left, diamond_col);
            gizmos.line_2d(d_left, d_top, diamond_col);
            gizmos.line_2d(d_left, d_right, Color::srgba(1.0, 1.0, 1.0, 0.7));
        }
    }
}
