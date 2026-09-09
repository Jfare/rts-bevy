use bevy::prelude::*;
use shared::components::*;

use crate::session::Matchmaker;

/// Worker mining and resource dropoff loop
pub fn server_mining_system(
    mut commands: Commands,
    time: Res<Time>,
    mut matchmaker: ResMut<Matchmaker>,
    mut workers: Query<(Entity, &mut Transform, &MoveSpeed, &Faction, &RoomId, &mut Worker, Option<&MoveTarget>)>,
    mut nodes: Query<(&Transform, &mut ResourceNode, &NetEntity, &RoomId), Without<Worker>>,
    bases: Query<(&Transform, &Faction, &RoomId), (With<BaseHQ>, Without<Worker>, Without<ResourceNode>)>,
) {
    let dt = time.delta_secs();
    for (worker_e, mut transform, speed, faction, worker_room, mut worker, move_target_opt) in &mut workers {
        let is_room_active = matchmaker.rooms.get(&worker_room.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active {
            continue;
        }

        if worker.state != WorkerState::Idle && move_target_opt.is_some() {
            commands.entity(worker_e).remove::<MoveTarget>();
        }

        match worker.state {
            WorkerState::Idle => {}
            WorkerState::MovingToResource => {
                let Some(node_e) = worker.target_node else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((node_tf, node, _, node_room)) = nodes.get(node_e) {
                    if node_room.0 != worker_room.0 || node.remaining_minerals == 0 {
                        worker.target_node = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = transform.translation.truncate();
                    let n_pos = node_tf.translation.truncate();
                    let dist = w_pos.distance(n_pos);

                    if dist <= worker.interact_distance {
                        worker.state = WorkerState::Mining;
                        worker.harvest_timer = 0.0;
                    } else {
                        let dir = (n_pos - w_pos).normalize_or_zero();
                        transform.translation.x += dir.x * speed.0 * dt;
                        transform.translation.y += dir.y * speed.0 * dt;
                        let angle = dir.y.atan2(dir.x);
                        transform.rotation = Quat::from_rotation_z(angle);
                    }
                } else {
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::Mining => {
                let Some(node_e) = worker.target_node else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((_, mut node, _, node_room)) = nodes.get_mut(node_e) {
                    if node_room.0 != worker_room.0 || node.remaining_minerals == 0 {
                        worker.target_node = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    worker.harvest_timer += dt;
                    if worker.harvest_timer >= worker.harvest_duration {
                        let amount = node.remaining_minerals.min(worker.harvest_capacity);
                        node.remaining_minerals -= amount;
                        worker.carried_minerals = amount;
                        worker.state = WorkerState::MovingToBase;
                        worker.harvest_timer = 0.0;
                    }
                } else {
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::MovingToBase => {
                let w_pos = transform.translation.truncate();
                let mut best_base = None;
                let mut min_dist = f32::MAX;

                for (base_tf, base_faction, base_room) in &bases {
                    if base_room.0 == worker_room.0 && base_faction == faction {
                        let b_pos = base_tf.translation.truncate();
                        let dist = w_pos.distance(b_pos);
                        if dist < min_dist {
                            min_dist = dist;
                            best_base = Some(b_pos);
                        }
                    }
                }

                if let Some(base_pos) = best_base {
                    if min_dist <= worker.base_interact_distance {
                        if let Some(room) = matchmaker.rooms.get_mut(&worker_room.0) {
                            room.economy.add_minerals(*faction, worker.carried_minerals);
                        }
                        worker.carried_minerals = 0;
                        if worker.target_node.is_some() {
                            worker.state = WorkerState::MovingToResource;
                        } else {
                            worker.state = WorkerState::Idle;
                        }
                    } else {
                        let dir = (base_pos - w_pos).normalize_or_zero();
                        transform.translation.x += dir.x * speed.0 * dt;
                        transform.translation.y += dir.y * speed.0 * dt;
                        let angle = dir.y.atan2(dir.x);
                        transform.rotation = Quat::from_rotation_z(angle);
                    }
                } else {
                    worker.state = WorkerState::Idle;
                }
            }
        }
    }
}

