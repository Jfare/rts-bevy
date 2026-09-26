use bevy::prelude::*;
use shared::components::*;
use shared::protocol::ServerMessage;

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;

/// Worker mining, building construction, and resource dropoff loop
pub fn server_mining_system(
    _commands: Commands,
    time: Res<Time>,
    net_channels: Option<Res<ServerNetworkChannels>>,
    mut matchmaker: ResMut<Matchmaker>,
    mut workers: Query<(Entity, &mut Transform, &MoveSpeed, &Faction, &RoomId, &mut Worker, Option<&MoveTarget>)>,
    mut nodes: Query<(Entity, &Transform, &mut ResourceNode, &NetEntity, &RoomId), Without<Worker>>,
    bases: Query<(Entity, &Transform, &Faction, &RoomId), (With<BaseHQ>, Without<Worker>, Without<ResourceNode>)>,
    mut buildings: Query<(
        Entity,
        &Transform,
        &mut Building,
        &mut Health,
        &Radius,
        &Faction,
        &RoomId,
        &NetEntity,
        Option<&SupplyDepot>,
    ), (Without<Worker>, Without<ResourceNode>)>,
) {
    let dt = time.delta_secs();

    // Track active miners targeting each resource node to enforce MAX_WORKERS_PER_ROCK
    let mut node_worker_counts: std::collections::HashMap<Entity, usize> = std::collections::HashMap::new();
    // Track workers assigned to construct each building
    let mut building_worker_counts: std::collections::HashMap<Entity, usize> = std::collections::HashMap::new();

    for (_, _, _, _, _, worker, move_target_opt) in &workers {
        if move_target_opt.is_none() {
            if worker.state != WorkerState::Idle {
                if let Some(target) = worker.target_node {
                    *node_worker_counts.entry(target).or_default() += 1;
                }
            }
            if worker.state == WorkerState::MovingToBuilding || worker.state == WorkerState::Building {
                if let Some(target_b) = worker.target_building {
                    *building_worker_counts.entry(target_b).or_default() += 1;
                }
            }
        }
    }

    // Collect unconstructed buildings
    let mut unconstructed_buildings = Vec::new();
    for (b_ent, b_tf, b, _, b_rad, b_fac, b_room, b_net, _) in buildings.iter() {
        if !b.is_constructed {
            unconstructed_buildings.push((
                b_ent,
                b_tf.translation.truncate(),
                b_rad.0,
                *b_fac,
                b_room.0,
                b_net.net_id,
            ));
        }
    }

    // Count idle workers per room and faction (excluding manual player override)
    let mut idle_worker_count: std::collections::HashMap<(u32, Faction), usize> = std::collections::HashMap::new();
    for (_, _, _, faction, worker_room, worker, move_target_opt) in &workers {
        if move_target_opt.is_none() && worker.state == WorkerState::Idle && !worker.manual_override {
            *idle_worker_count.entry((worker_room.0, *faction)).or_default() += 1;
        }
    }

    // If an unconstructed building has 0 assigned builders and 0 idle workers in that room/faction,
    // draft the closest mining worker to construct it!
    for &(b_ent, b_pos, _, b_fac, b_room, _) in &unconstructed_buildings {
        let assigned_count = building_worker_counts.get(&b_ent).copied().unwrap_or(0);
        let idles = idle_worker_count.get(&(b_room, b_fac)).copied().unwrap_or(0);
        if assigned_count == 0 && idles == 0 {
            let mut best_miner: Option<(Entity, f32)> = None;
            for (w_ent, w_tf, _, w_fac, w_room, w, w_move) in &workers {
                if w_move.is_none() && !w.manual_override && w_room.0 == b_room && *w_fac == b_fac {
                    if w.state == WorkerState::Mining
                        || w.state == WorkerState::MovingToResource
                        || w.state == WorkerState::MovingToBase
                    {
                        let dist = w_tf.translation.truncate().distance(b_pos);
                        if best_miner.map_or(true, |(_, d)| dist < d) {
                            best_miner = Some((w_ent, dist));
                        }
                    }
                }
            }

            if let Some((drafted_e, _)) = best_miner {
                if let Ok((_, mut tf, _, _, _, mut w, _)) = workers.get_mut(drafted_e) {
                    if let Some(target_node) = w.target_node {
                        if let Some(c) = node_worker_counts.get_mut(&target_node) {
                            *c = c.saturating_sub(1);
                        }
                    }
                    w.target_node = None;
                    w.carried_minerals = 0;
                    w.target_building = Some(b_ent);
                    w.state = WorkerState::MovingToBuilding;
                    *building_worker_counts.entry(b_ent).or_default() += 1;
                    let dir = (b_pos - tf.translation.truncate()).normalize_or_zero();
                    if dir.length_squared() > 0.0 {
                        tf.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                    }
                }
            }
        }
    }

    for (_worker_e, mut transform, speed, faction, worker_room, mut worker, move_target_opt) in &mut workers {
        let is_room_active = matchmaker
            .rooms
            .get(&worker_room.0)
            .map(|r| r.is_active && r.countdown_timer <= 0.0)
            .unwrap_or(true);
        if !is_room_active {
            continue;
        }

        // If worker received a manual move order, cancel automated mining/building loop
        if move_target_opt.is_some() && worker.state != WorkerState::Idle {
            if let Some(target) = worker.target_node {
                if let Some(c) = node_worker_counts.get_mut(&target) {
                    *c = c.saturating_sub(1);
                }
            }
            if let Some(target_b) = worker.target_building {
                if let Some(c) = building_worker_counts.get_mut(&target_b) {
                    *c = c.saturating_sub(1);
                }
            }
            worker.state = WorkerState::Idle;
            worker.target_node = None;
            worker.target_building = None;
        }

        match worker.state {
            WorkerState::Idle => {
                if move_target_opt.is_none() && !worker.manual_override {
                    let w_pos = transform.translation.truncate();
                    if worker.carried_minerals > 0 {
                        worker.state = WorkerState::MovingToBase;
                    } else {
                        // 1. Prioritize unconstructed friendly buildings
                        let mut best_bldg: Option<(Entity, Vec2)> = None;
                        let mut best_bldg_dist = f32::MAX;
                        for &(b_ent, b_pos, _, b_fac, b_room, _) in &unconstructed_buildings {
                            if b_room == worker_room.0 && b_fac == *faction {
                                if let Ok((_, _, b, ..)) = buildings.get(b_ent) {
                                    if !b.is_constructed {
                                        let dist = w_pos.distance(b_pos);
                                        if dist < best_bldg_dist {
                                            best_bldg_dist = dist;
                                            best_bldg = Some((b_ent, b_pos));
                                        }
                                    }
                                }
                            }
                        }

                        if let Some((b_ent, b_pos)) = best_bldg {
                            worker.target_building = Some(b_ent);
                            worker.target_node = None;
                            worker.state = WorkerState::MovingToBuilding;
                            *building_worker_counts.entry(b_ent).or_default() += 1;
                            let dir = (b_pos - w_pos).normalize_or_zero();
                            if dir.length_squared() > 0.0 {
                                transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                            }
                            continue;
                        }

                        // 2. Fall back to mining nearest available gold rock
                        let mut best_node = None;
                        let mut best_dist = WORKER_AUTO_MINE_RANGE;

                        for (node_e, node_tf, node, _, node_room) in &nodes {
                            if node_room.0 == worker_room.0 && node.remaining_minerals > 0 {
                                let current_miners = node_worker_counts.get(&node_e).copied().unwrap_or(0);
                                if current_miners >= MAX_WORKERS_PER_ROCK {
                                    continue;
                                }

                                let n_pos = node_tf.translation.truncate();
                                let dist = w_pos.distance(n_pos);
                                if dist <= best_dist {
                                    let mut has_friendly_base_nearby = false;
                                    let mut min_friendly_base_dist = f32::MAX;
                                    let mut min_enemy_base_dist = f32::MAX;

                                    for (base_e, base_tf, base_faction, base_room) in &bases {
                                        if base_room.0 == worker_room.0 {
                                            let b_pos = base_tf.translation.truncate();
                                            let b_dist = n_pos.distance(b_pos);
                                            let is_constructed = if let Ok((_, _, b, ..)) = buildings.get(base_e) {
                                                b.is_constructed
                                            } else {
                                                true
                                            };
                                            if *base_faction == *faction {
                                                if is_constructed && b_dist <= BASE_HQ_RESOURCE_RADIUS {
                                                    has_friendly_base_nearby = true;
                                                }
                                                if b_dist < min_friendly_base_dist {
                                                    min_friendly_base_dist = b_dist;
                                                }
                                            } else if base_faction.is_hostile_to(faction) {
                                                if b_dist < min_enemy_base_dist {
                                                    min_enemy_base_dist = b_dist;
                                                }
                                            }
                                        }
                                    }

                                    if has_friendly_base_nearby && min_enemy_base_dist >= min_friendly_base_dist {
                                        best_dist = dist;
                                        best_node = Some((node_e, n_pos));
                                    }
                                }
                            }
                        }

                        if let Some((node_e, n_pos)) = best_node {
                            *node_worker_counts.entry(node_e).or_default() += 1;
                            worker.target_node = Some(node_e);
                            worker.state = WorkerState::MovingToResource;
                            let dir = (n_pos - w_pos).normalize_or_zero();
                            if dir.length_squared() > 0.0 {
                                transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                            }
                        } else {
                            // 3. Fall back to repairing damaged friendly constructed buildings
                            let mut best_repair_bldg: Option<(Entity, Vec2)> = None;
                            let mut best_repair_dist = f32::MAX;
                            for (b_ent, b_tf, b, hp, _, b_fac, b_room, ..) in buildings.iter() {
                                if b_room.0 == worker_room.0 && *b_fac == *faction && b.is_constructed && hp.current < hp.max {
                                    let b_pos = b_tf.translation.truncate();
                                    let dist = w_pos.distance(b_pos);
                                    if dist < best_repair_dist {
                                        best_repair_dist = dist;
                                        best_repair_bldg = Some((b_ent, b_pos));
                                    }
                                }
                            }

                            if let Some((b_ent, b_pos)) = best_repair_bldg {
                                worker.target_building = Some(b_ent);
                                worker.target_node = None;
                                worker.state = WorkerState::MovingToRepair;
                                let dir = (b_pos - w_pos).normalize_or_zero();
                                if dir.length_squared() > 0.0 {
                                    transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                                }
                            }
                        }
                    }
                }
            }
            WorkerState::MovingToBuilding => {
                let Some(b_ent) = worker.target_building else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((_, b_tf, b, _, b_rad, b_fac, b_room, ..)) = buildings.get(b_ent) {
                    if b_room.0 != worker_room.0 || *b_fac != *faction || b.is_constructed {
                        if let Some(c) = building_worker_counts.get_mut(&b_ent) {
                            *c = c.saturating_sub(1);
                        }
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = transform.translation.truncate();
                    let b_pos = b_tf.translation.truncate();
                    let dist = w_pos.distance(b_pos);
                    let build_range = b_rad.0 + 26.0;

                    if dist <= build_range {
                        worker.state = WorkerState::Building;
                    } else {
                        let dir = (b_pos - w_pos).normalize_or_zero();
                        transform.translation.x += dir.x * speed.0 * dt;
                        transform.translation.y += dir.y * speed.0 * dt;
                        let angle = dir.y.atan2(dir.x);
                        transform.rotation = Quat::from_rotation_z(angle);
                    }
                } else {
                    if let Some(c) = building_worker_counts.get_mut(&b_ent) {
                        *c = c.saturating_sub(1);
                    }
                    worker.target_building = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::Building => {
                let Some(b_ent) = worker.target_building else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((_, b_tf, mut b, mut hp, b_rad, b_fac, b_room, b_net, supply_depot_opt)) =
                    buildings.get_mut(b_ent)
                {
                    if b_room.0 != worker_room.0 || *b_fac != *faction || b.is_constructed {
                        if let Some(c) = building_worker_counts.get_mut(&b_ent) {
                            *c = c.saturating_sub(1);
                        }
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = transform.translation.truncate();
                    let b_pos = b_tf.translation.truncate();
                    let dist = w_pos.distance(b_pos);
                    let build_range = b_rad.0 + 30.0;

                    if dist > build_range {
                        worker.state = WorkerState::MovingToBuilding;
                        continue;
                    }

                    // Face the building directly
                    let dir = (b_pos - w_pos).normalize_or_zero();
                    if dir.length_squared() > 0.0 {
                        transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                    }

                    // Collaborative construction: each active worker advances build timer by dt
                    b.build_timer += dt;
                    hp.current = (hp.max * b.progress()).max(15.0);

                    if b.build_timer >= b.build_duration {
                        b.is_constructed = true;
                        b.build_timer = b.build_duration;
                        hp.current = hp.max;

                        if let Some(depot) = supply_depot_opt {
                            if let Some(room) = matchmaker.rooms.get_mut(&worker_room.0) {
                                room.economy.add_max_supply(*faction, depot.supply_provided);
                            }
                        }

                        if let Some(ref channels) = net_channels {
                            let peers = matchmaker.get_room_peers(worker_room.0);
                            if !peers.is_empty() {
                                let _ = channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                    peer_ids: peers,
                                    msg: ServerMessage::BuildingConstructed {
                                        building_net_id: b_net.net_id,
                                    },
                                });
                            }
                        }

                        if let Some(c) = building_worker_counts.get_mut(&b_ent) {
                            *c = c.saturating_sub(1);
                        }
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                    }
                } else {
                    if let Some(c) = building_worker_counts.get_mut(&b_ent) {
                        *c = c.saturating_sub(1);
                    }
                    worker.target_building = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::MovingToRepair => {
                let Some(b_ent) = worker.target_building else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((_, b_tf, b, hp, b_rad, b_fac, b_room, ..)) = buildings.get(b_ent) {
                    if b_room.0 != worker_room.0 || *b_fac != *faction || !b.is_constructed || hp.current >= hp.max {
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = transform.translation.truncate();
                    let b_pos = b_tf.translation.truncate();
                    let dist = w_pos.distance(b_pos);
                    let repair_range = b_rad.0 + 26.0;

                    if dist <= repair_range {
                        worker.state = WorkerState::Repairing;
                    } else {
                        let dir = (b_pos - w_pos).normalize_or_zero();
                        transform.translation.x += dir.x * speed.0 * dt;
                        transform.translation.y += dir.y * speed.0 * dt;
                        let angle = dir.y.atan2(dir.x);
                        transform.rotation = Quat::from_rotation_z(angle);
                    }
                } else {
                    worker.target_building = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::Repairing => {
                let Some(b_ent) = worker.target_building else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((_, b_tf, b, mut hp, b_rad, b_fac, b_room, ..)) = buildings.get_mut(b_ent) {
                    if b_room.0 != worker_room.0 || *b_fac != *faction || !b.is_constructed || hp.current >= hp.max {
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = transform.translation.truncate();
                    let b_pos = b_tf.translation.truncate();
                    let dist = w_pos.distance(b_pos);
                    let repair_range = b_rad.0 + 30.0;

                    if dist > repair_range {
                        worker.state = WorkerState::MovingToRepair;
                        continue;
                    }

                    // Face the building directly
                    let dir = (b_pos - w_pos).normalize_or_zero();
                    if dir.length_squared() > 0.0 {
                        transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                    }

                    hp.current = (hp.current + WORKER_REPAIR_RATE * dt).min(hp.max);

                    if hp.current >= hp.max {
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                    }
                } else {
                    worker.target_building = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::MovingToResource => {
                let Some(node_e) = worker.target_node else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((_, node_tf, node, _, node_room)) = nodes.get(node_e) {
                    if node_room.0 != worker_room.0 || node.remaining_minerals == 0 {
                        if let Some(c) = node_worker_counts.get_mut(&node_e) {
                            *c = c.saturating_sub(1);
                        }
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
                    if let Some(c) = node_worker_counts.get_mut(&node_e) {
                        *c = c.saturating_sub(1);
                    }
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::Mining => {
                let Some(node_e) = worker.target_node else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((_, node_tf, mut node, _, node_room)) = nodes.get_mut(node_e) {
                    if node_room.0 != worker_room.0 || node.remaining_minerals == 0 {
                        if let Some(c) = node_worker_counts.get_mut(&node_e) {
                            *c = c.saturating_sub(1);
                        }
                        worker.target_node = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = transform.translation.truncate();
                    let n_pos = node_tf.translation.truncate();
                    let dir = (n_pos - w_pos).normalize_or_zero();
                    if dir.length_squared() > 0.0 {
                        transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                    }

                    worker.harvest_timer += dt;
                    if worker.harvest_timer >= worker.harvest_duration {
                        let amount = node.remaining_minerals.min(worker.harvest_capacity);
                        node.remaining_minerals -= amount;
                        worker.carried_minerals = amount;
                        worker.state = WorkerState::MovingToBase;
                        worker.harvest_timer = 0.0;

                        let mut best_base = None;
                        let mut min_dist = f32::MAX;
                        for (_, base_tf, base_faction, base_room) in &bases {
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
                            let dir = (base_pos - w_pos).normalize_or_zero();
                            if dir.length_squared() > 0.0 {
                                transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                            }
                        }
                    }
                } else {
                    if let Some(c) = node_worker_counts.get_mut(&node_e) {
                        *c = c.saturating_sub(1);
                    }
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::MovingToBase => {
                let w_pos = transform.translation.truncate();
                let mut best_base = None;
                let mut min_dist = f32::MAX;

                for (_, base_tf, base_faction, base_room) in &bases {
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
                        if let Some(node_e) = worker.target_node {
                            let node_valid = if let Ok((_, node_tf, node, _, node_room)) = nodes.get(node_e) {
                                if node_room.0 == worker_room.0 && node.remaining_minerals > 0 {
                                    let n_pos = node_tf.translation.truncate();
                                    bases.iter().any(|(base_e, base_tf, base_faction, base_room)| {
                                        let is_constructed = if let Ok((_, _, b, ..)) = buildings.get(base_e) {
                                            b.is_constructed
                                        } else {
                                            true
                                        };
                                        base_room.0 == worker_room.0
                                            && base_faction == faction
                                            && is_constructed
                                            && n_pos.distance(base_tf.translation.truncate()) <= BASE_HQ_RESOURCE_RADIUS
                                    })
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            if node_valid {
                                worker.state = WorkerState::MovingToResource;
                                if let Ok((_, node_tf, ..)) = nodes.get(node_e) {
                                    let dir = (node_tf.translation.truncate() - w_pos).normalize_or_zero();
                                    if dir.length_squared() > 0.0 {
                                        transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                                    }
                                }
                            } else {
                                if let Some(c) = node_worker_counts.get_mut(&node_e) {
                                    *c = c.saturating_sub(1);
                                }
                                worker.target_node = None;
                                worker.state = WorkerState::Idle;
                            }
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
                    if let Some(target) = worker.target_node {
                        if let Some(c) = node_worker_counts.get_mut(&target) {
                            *c = c.saturating_sub(1);
                        }
                    }
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                }
            }
        }
    }
}
