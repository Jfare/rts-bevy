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
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Contextual right-click handler: If right-clicking a mineral node with workers selected, start mining!
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

/// Worker Mining State Machine
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

                let Ok((_, node_transform, mut node)) = node_query.get_mut(node_entity) else {
                    worker.state = WorkerState::MovingToBase;
                    continue;
                };

                // Face the golden rock directly while in melee range
                let worker_pos = worker_transform.translation.truncate();
                let node_pos = node_transform.translation.truncate();
                let dir = (node_pos - worker_pos).normalize_or_zero();
                if dir.length_squared() > 0.0 {
                    let angle = dir.y.atan2(dir.x);
                    worker_transform.rotation = Quat::from_rotation_z(angle);
                }

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
                        info!("🪙 [Mining] Worker deposited {} gold for {:?}! New Bank Total: {}", worker.carried_minerals, faction, economy.get_minerals(*faction));
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

/// Renders the pulsating golden mining laser and carried gold nugget
fn draw_mining_visuals(
    time: Res<Time>,
    mut gizmos: Gizmos,
    worker_query: Query<(&Transform, &Worker)>,
    _node_query: Query<(Entity, &Transform, &ResourceNode)>,
) {
    let t = time.elapsed_secs();

    for (worker_transform, worker) in &worker_query {
        let worker_pos = worker_transform.translation.truncate();

        let rot = worker_transform.rotation.to_euler(EulerRot::ZYX).0;
        let forward = Vec2::new(rot.cos(), rot.sin());
        let right_side = Vec2::new(forward.y, -forward.x);

        // 1. Pickaxe Mining Animation & Strike Visuals
        if worker.state == WorkerState::Mining {
            // Rhythmic pickaxe swing cycle: ~0.42 seconds per swing
            let swing_freq = 2.4;
            let cycle = (t * swing_freq).fract(); // 0.0 to 1.0

            // Swing angle relative to worker forward orientation:
            // 0.00..0.55: Wind-up - raising pickaxe back/high (+55 deg)
            // 0.55..0.78: Power downswing - slamming forward down to the rock (-25 deg)
            // 0.78..1.00: Impact, recoil, and follow-through (+10 deg returning to wind-up)
            let angle_offset = if cycle < 0.55 {
                let p = cycle / 0.55;
                0.35 + 0.60 * (p * std::f32::consts::PI * 0.5).sin()
            } else if cycle < 0.78 {
                let p = (cycle - 0.55) / 0.23;
                0.95 - 1.40 * (p * p)
            } else {
                let p = (cycle - 0.78) / 0.22;
                -0.45 + 0.80 * (p * std::f32::consts::PI * 0.5).sin()
            };

            let swing_angle = rot + angle_offset;
            let swing_dir = Vec2::new(swing_angle.cos(), swing_angle.sin());
            let swing_perp = Vec2::new(-swing_dir.y, swing_dir.x);

            // Worker hands / handle pivot
            let hands_pos = worker_pos + forward * 7.0 + right_side * 3.5;
            let handle_len = 19.0;
            let pick_head_center = hands_pos + swing_dir * handle_len;

            // Worker arms holding pickaxe haft
            let left_shoulder = worker_pos + forward * 4.0 - right_side * 4.5;
            let right_shoulder = worker_pos + forward * 5.0 + right_side * 4.5;
            gizmos.line_2d(left_shoulder, hands_pos, Color::srgb(0.95, 0.75, 0.20));
            gizmos.line_2d(right_shoulder, hands_pos, Color::srgb(0.95, 0.75, 0.20));

            // Pickaxe wooden haft (shaft)
            gizmos.line_2d(hands_pos, pick_head_center, Color::srgb(0.72, 0.46, 0.22)); // Warm wood
            gizmos.line_2d(hands_pos + swing_perp * 0.8, pick_head_center + swing_perp * 0.8, Color::srgb(0.55, 0.32, 0.12));

            // Forged steel pickaxe head: double-pointed curved pick
            let front_tip = pick_head_center + swing_perp * 12.0 + swing_dir * 4.5;
            let back_tip = pick_head_center - swing_perp * 9.0 - swing_dir * 2.0;

            // Steel collar / eye
            gizmos.circle_2d(pick_head_center, 2.6, Color::srgb(0.38, 0.42, 0.48));
            gizmos.circle_2d(pick_head_center, 1.4, Color::srgb(0.65, 0.70, 0.78));

            // Curved pick blade lines
            gizmos.line_2d(back_tip, pick_head_center, Color::srgb(0.78, 0.82, 0.90)); // Rear pick
            gizmos.line_2d(pick_head_center, front_tip, Color::srgb(0.95, 0.98, 1.0)); // Front striking blade
            gizmos.line_2d(pick_head_center + swing_dir * 1.5, front_tip, Color::srgb(0.70, 0.75, 0.85)); // Blade bevel
            gizmos.circle_2d(front_tip, 1.4, Color::srgb(1.0, 1.0, 1.0)); // Glint on sharp chisel tip

            // Strike Impact: Sparks, Flash & Golden Rock Chips
            let is_striking = cycle >= 0.74 && cycle <= 0.88;
            if is_striking {
                // Bright golden impact flash
                gizmos.circle_2d(front_tip, 5.0, Color::srgba(1.0, 0.95, 0.50, 0.85));
                gizmos.circle_2d(front_tip, 2.5, Color::srgb(1.0, 1.0, 1.0));

                // Molten golden sparks radiating outward
                for s in 0..6 {
                    let s_angle = (s as f32) * 1.05 + t * 40.0;
                    let s_dist = 6.0 + (s as f32) * 2.5;
                    let spark_pos = front_tip + Vec2::new(s_angle.cos(), s_angle.sin()) * s_dist;
                    let spark_col = if s % 2 == 0 {
                        Color::srgb(1.0, 0.88, 0.25)
                    } else {
                        Color::srgb(1.0, 0.55, 0.15)
                    };
                    gizmos.circle_2d(spark_pos, 1.8, spark_col);
                }

                // Flying chipped golden rock fragments
                let chip_dir = forward;
                let chip_left = front_tip + chip_dir * 5.0 + right_side * 7.0;
                let chip_right = front_tip + chip_dir * 4.0 - right_side * 8.0;
                gizmos.line_2d(front_tip, chip_left, Color::srgb(1.0, 0.84, 0.18));
                gizmos.line_2d(front_tip, chip_right, Color::srgb(1.0, 0.92, 0.40));
            }
        } else {
            // Worker is Idle, Moving to Resource, or Returning to Base:
            // Render pickaxe resting / strapped at worker's side
            let rest_angle = rot - 0.55;
            let rest_dir = Vec2::new(rest_angle.cos(), rest_angle.sin());
            let rest_perp = Vec2::new(-rest_dir.y, rest_dir.x);
            let handle_base = worker_pos - right_side * 6.0 + forward * 2.0;
            let pick_head = handle_base + rest_dir * 14.0;

            // Shaft
            gizmos.line_2d(handle_base, pick_head, Color::srgb(0.72, 0.46, 0.22));
            // Collar
            gizmos.circle_2d(pick_head, 2.0, Color::srgb(0.40, 0.45, 0.50));
            // Pick head
            let p_back = pick_head - rest_perp * 6.0;
            let p_front = pick_head + rest_perp * 7.5;
            gizmos.line_2d(p_back, p_front, Color::srgb(0.85, 0.88, 0.95));
        }

        // 2. Draw Carried Gold Nugget on Worker
        if worker.carried_minerals > 0 {
            let nugget_center = worker_pos + Vec2::new(0.0, 18.0);
            let gold_bright = Color::srgb(1.0, 0.84, 0.18);
            let gold_highlight = Color::srgb(1.0, 0.98, 0.65);
            let gold_shadow = Color::srgb(0.75, 0.52, 0.10);

            // Chunky faceted golden nugget
            let pts = [
                nugget_center + Vec2::new(-2.0, 7.0),
                nugget_center + Vec2::new(4.5, 5.0),
                nugget_center + Vec2::new(6.0, -1.5),
                nugget_center + Vec2::new(2.5, -6.5),
                nugget_center + Vec2::new(-4.0, -5.0),
                nugget_center + Vec2::new(-6.0, 1.5),
            ];

            for i in 0..pts.len() {
                let next = (i + 1) % pts.len();
                gizmos.line_2d(pts[i], pts[next], gold_bright);
            }

            // Chiseled facets
            let apex = nugget_center + Vec2::new(-0.5, 1.5);
            gizmos.line_2d(apex, pts[0], gold_highlight);
            gizmos.line_2d(apex, pts[1], gold_highlight);
            gizmos.line_2d(apex, pts[2], gold_bright);
            gizmos.line_2d(apex, pts[3], gold_shadow);
            gizmos.line_2d(apex, pts[4], gold_shadow);
            gizmos.line_2d(apex, pts[5], gold_bright);
            gizmos.circle_2d(apex, 1.5, Color::srgb(1.0, 1.0, 0.9));
        }
    }
}
