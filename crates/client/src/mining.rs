use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::WorldGridConfig;
use shared::protocol::ClientMessage;
use crate::audio_sfx::SoundEffect;
use crate::fog_of_war::{FogOfWarGrid, FogState};
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
    mut stats: ResMut<MatchStats>,
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

    let current_miners = worker_query
        .iter()
        .filter(|(_, _, selectable, worker, _)| {
            !selectable.is_selected
                && worker.target_node == Some(target_node_entity)
                && worker.state != WorkerState::Idle
        })
        .count();
    let available_slots = MAX_WORKERS_PER_ROCK.saturating_sub(current_miners);

    let mut worker_net_ids = Vec::new();
    let mut any_assigned = false;

    for (worker_entity, faction, selectable, mut worker, net_opt) in &mut worker_query {
        if selectable.is_selected && *faction == net_client.my_faction {
            if worker_net_ids.len() < available_slots {
                any_assigned = true;
                worker.target_node = Some(target_node_entity);
                worker.state = WorkerState::MovingToResource;
                worker.harvest_timer = 0.0;
                worker.carried_minerals = 0;
                worker.mining_spot_index = None;
                commands.entity(worker_entity).remove::<MoveTarget>();

                if let Some(net) = net_opt {
                    worker_net_ids.push(net.net_id);
                }
            }
        }
    }

    if any_assigned {
        stats.record_action();
        sound_events.send(SoundEffect::OrderIssued);
        if net_client.status != NetStatus::Disconnected && !worker_net_ids.is_empty() {
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
    _commands: Commands,
    time: Res<Time>,
    net_client: Res<NetClient>,
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
    base_query: Query<(Entity, &Transform, &Faction, &BaseHQ), (With<BaseHQ>, Without<Worker>, Without<ResourceNode>)>,
    mut building_query: Query<(
        Entity,
        &Transform,
        &mut Building,
        &mut Health,
        &Radius,
        &Faction,
        Option<&SupplyDepot>,
    ), (Without<Worker>, Without<ResourceNode>)>,
    mut sound_events: EventWriter<SoundEffect>,
) {
    // In online matches, the server simulates mining authoritatively and replicates via TickSnapshotBatch
    if net_client.status == NetStatus::InGame {
        return;
    }

    let dt = time.delta_secs();

    // Track active miners targeting each resource node to enforce MAX_WORKERS_PER_ROCK
    let mut node_worker_counts: std::collections::HashMap<Entity, usize> = std::collections::HashMap::new();
    // Track occupied spots per resource node (0..MINING_SPOTS_PER_ROCK)
    let mut node_occupied_spots: std::collections::HashMap<Entity, [bool; MINING_SPOTS_PER_ROCK]> = std::collections::HashMap::new();
    let mut building_worker_counts: std::collections::HashMap<Entity, usize> = std::collections::HashMap::new();

    for (_, worker, _, _, _, move_target_opt) in &worker_query {
        if move_target_opt.is_none() {
            if worker.state != WorkerState::Idle {
                if let Some(target) = worker.target_node {
                    *node_worker_counts.entry(target).or_default() += 1;
                    if let Some(spot) = worker.mining_spot_index {
                        if spot < MINING_SPOTS_PER_ROCK && (worker.state == WorkerState::MovingToResource || worker.state == WorkerState::Mining) {
                            let spots = node_occupied_spots.entry(target).or_insert([false; MINING_SPOTS_PER_ROCK]);
                            spots[spot] = true;
                        }
                    }
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
    for (b_ent, b_tf, b, _, b_rad, b_fac, _) in building_query.iter() {
        if !b.is_constructed {
            unconstructed_buildings.push((
                b_ent,
                b_tf.translation.truncate(),
                b_rad.0,
                *b_fac,
            ));
        }
    }

    // Count idle workers per faction (excluding manual player override)
    let mut idle_worker_count: std::collections::HashMap<Faction, usize> = std::collections::HashMap::new();
    for (_, worker, _, _, faction, move_target_opt) in &worker_query {
        if move_target_opt.is_none() && worker.state == WorkerState::Idle && !worker.manual_override {
            *idle_worker_count.entry(*faction).or_default() += 1;
        }
    }

    // Draft closest mining worker if an unconstructed building has 0 builders and 0 idle workers exist
    for &(b_ent, b_pos, _, b_fac) in &unconstructed_buildings {
        let assigned_count = building_worker_counts.get(&b_ent).copied().unwrap_or(0);
        let idles = idle_worker_count.get(&b_fac).copied().unwrap_or(0);
        if assigned_count == 0 && idles == 0 {
            let mut best_miner: Option<(Entity, f32)> = None;
            for (w_ent, w, w_tf, _, w_fac, w_move) in &worker_query {
                if w_move.is_none() && !w.manual_override && *w_fac == b_fac {
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
                if let Ok((_, mut w, mut tf, ..)) = worker_query.get_mut(drafted_e) {
                    if let Some(target_node) = w.target_node {
                        if let Some(c) = node_worker_counts.get_mut(&target_node) {
                            *c = c.saturating_sub(1);
                        }
                        if let Some(spot) = w.mining_spot_index {
                            if let Some(spots) = node_occupied_spots.get_mut(&target_node) {
                                spots[spot] = false;
                            }
                        }
                    }
                    w.mining_spot_index = None;
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

    for (_worker_entity, mut worker, mut worker_transform, move_speed, faction, move_target_opt) in &mut worker_query {
        // If player ordered a manual move, cancel the automated mining/building loop
        if move_target_opt.is_some() && worker.state != WorkerState::Idle {
            if let Some(target) = worker.target_node {
                if let Some(c) = node_worker_counts.get_mut(&target) {
                    *c = c.saturating_sub(1);
                }
                if let Some(spot) = worker.mining_spot_index {
                    if let Some(spots) = node_occupied_spots.get_mut(&target) {
                        spots[spot] = false;
                    }
                }
            }
            if let Some(target_b) = worker.target_building {
                if let Some(c) = building_worker_counts.get_mut(&target_b) {
                    *c = c.saturating_sub(1);
                }
            }
            worker.mining_spot_index = None;
            worker.state = WorkerState::Idle;
            worker.target_node = None;
            worker.target_building = None;
        }

        match worker.state {
            WorkerState::Idle => {
                if move_target_opt.is_none() && !worker.manual_override {
                    let worker_pos = worker_transform.translation.truncate();
                    if worker.carried_minerals > 0 {
                        worker.state = WorkerState::MovingToBase;
                    } else {
                        // 1. Check for unconstructed friendly buildings
                        let mut best_bldg: Option<(Entity, Vec2)> = None;
                        let mut best_bldg_dist = f32::MAX;
                        for &(b_ent, b_pos, _, b_fac) in &unconstructed_buildings {
                            if b_fac == *faction {
                                if let Ok((_, _, b, ..)) = building_query.get(b_ent) {
                                    if !b.is_constructed {
                                        let dist = worker_pos.distance(b_pos);
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
                            worker.mining_spot_index = None;
                            worker.state = WorkerState::MovingToBuilding;
                            *building_worker_counts.entry(b_ent).or_default() += 1;
                            let dir = (b_pos - worker_pos).normalize_or_zero();
                            if dir.length_squared() > 0.0 {
                                worker_transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                            }
                            continue;
                        }

                        // 2. Fall back to mining nearest available gold rock
                        let mut best_node = None;
                        let mut best_dist = WORKER_AUTO_MINE_RANGE;

                        for (node_ent, node_tf, node) in &node_query {
                            if node.remaining_minerals > 0 {
                                let current_miners = node_worker_counts.get(&node_ent).copied().unwrap_or(0);
                                if current_miners >= MAX_WORKERS_PER_ROCK {
                                    continue;
                                }

                                let n_pos = node_tf.translation.truncate();
                                let dist = worker_pos.distance(n_pos);
                                if dist <= best_dist {
                                    // Base HQ Proximity check: Must be within BASE_HQ_RESOURCE_RADIUS of friendly constructed Base HQ
                                    let mut has_friendly_base = false;
                                    let mut min_friendly_base_dist = f32::MAX;
                                    let mut min_enemy_base_dist = f32::MAX;

                                    for (base_ent, base_tf, base_fac, _) in &base_query {
                                        let b_pos = base_tf.translation.truncate();
                                        let b_dist = n_pos.distance(b_pos);
                                        let is_constructed = building_query.get(base_ent).map(|(_, _, b, ..)| b.is_constructed).unwrap_or(true);
                                        if *base_fac == *faction {
                                            if is_constructed && b_dist <= BASE_HQ_RESOURCE_RADIUS {
                                                has_friendly_base = true;
                                            }
                                            if b_dist < min_friendly_base_dist {
                                                min_friendly_base_dist = b_dist;
                                            }
                                        } else if base_fac.is_hostile_to(faction) {
                                            if b_dist < min_enemy_base_dist {
                                                min_enemy_base_dist = b_dist;
                                            }
                                        }
                                    }

                                    if has_friendly_base && min_enemy_base_dist >= min_friendly_base_dist {
                                        best_dist = dist;
                                        best_node = Some((node_ent, n_pos));
                                    }
                                }
                            }
                        }

                        if let Some((node_ent, n_pos)) = best_node {
                            *node_worker_counts.entry(node_ent).or_default() += 1;
                            worker.target_node = Some(node_ent);
                            worker.state = WorkerState::MovingToResource;
                            let spots = node_occupied_spots.entry(node_ent).or_insert([false; MINING_SPOTS_PER_ROCK]);
                            let spot_idx = find_closest_open_mining_spot(n_pos, worker_pos, spots);
                            spots[spot_idx] = true;
                            worker.mining_spot_index = Some(spot_idx);
                            let target_pos = get_mining_spot_position(n_pos, spot_idx);
                            let dir = (target_pos - worker_pos).normalize_or_zero();
                            if dir.length_squared() > 0.0 {
                                worker_transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                            }
                        } else {
                            // 3. Fall back to repairing damaged friendly constructed buildings
                            let mut best_repair_bldg: Option<(Entity, Vec2)> = None;
                            let mut best_repair_dist = f32::MAX;
                            for (b_ent, b_tf, b, hp, ..) in building_query.iter() {
                                if b.is_constructed && hp.current < hp.max {
                                    let b_pos = b_tf.translation.truncate();
                                    let dist = worker_pos.distance(b_pos);
                                    if dist < best_repair_dist {
                                        best_repair_dist = dist;
                                        best_repair_bldg = Some((b_ent, b_pos));
                                    }
                                }
                            }

                            if let Some((b_ent, b_pos)) = best_repair_bldg {
                                worker.target_building = Some(b_ent);
                                worker.target_node = None;
                                worker.mining_spot_index = None;
                                worker.state = WorkerState::MovingToRepair;
                                let dir = (b_pos - worker_pos).normalize_or_zero();
                                if dir.length_squared() > 0.0 {
                                    worker_transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
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

                if let Ok((_, b_tf, b, _, b_rad, b_fac, ..)) = building_query.get(b_ent) {
                    if *b_fac != *faction || b.is_constructed {
                        if let Some(c) = building_worker_counts.get_mut(&b_ent) {
                            *c = c.saturating_sub(1);
                        }
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = worker_transform.translation.truncate();
                    let b_pos = b_tf.translation.truncate();
                    let dist = w_pos.distance(b_pos);
                    let build_range = b_rad.0 + 26.0;

                    if dist <= build_range {
                        worker.state = WorkerState::Building;
                    } else {
                        let dir = (b_pos - w_pos).normalize_or_zero();
                        let step = dir * move_speed.0 * dt;
                        worker_transform.translation.x += step.x;
                        worker_transform.translation.y += step.y;
                        let angle = dir.y.atan2(dir.x);
                        worker_transform.rotation = Quat::from_rotation_z(angle);
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

                if let Ok((_, b_tf, mut b, mut hp, b_rad, b_fac, supply_depot_opt)) = building_query.get_mut(b_ent) {
                    if *b_fac != *faction || b.is_constructed {
                        if let Some(c) = building_worker_counts.get_mut(&b_ent) {
                            *c = c.saturating_sub(1);
                        }
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = worker_transform.translation.truncate();
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
                        worker_transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                    }

                    b.build_timer += dt;
                    hp.current = (hp.max * b.progress()).max(15.0);

                    if b.build_timer >= b.build_duration {
                        b.is_constructed = true;
                        b.build_timer = b.build_duration;
                        hp.current = hp.max;

                        if supply_depot_opt.is_some() {
                            economy.add_max_supply(*faction, 8);
                        }

                        sound_events.send(SoundEffect::BuildPlaced);

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

                if let Ok((_, b_tf, b, hp, b_rad, b_fac, ..)) = building_query.get(b_ent) {
                    if *b_fac != *faction || !b.is_constructed || hp.current >= hp.max {
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = worker_transform.translation.truncate();
                    let b_pos = b_tf.translation.truncate();
                    let dist = w_pos.distance(b_pos);
                    let repair_range = b_rad.0 + 26.0;

                    if dist <= repair_range {
                        worker.state = WorkerState::Repairing;
                    } else {
                        let dir = (b_pos - w_pos).normalize_or_zero();
                        let step = dir * move_speed.0 * dt;
                        worker_transform.translation.x += step.x;
                        worker_transform.translation.y += step.y;
                        let angle = dir.y.atan2(dir.x);
                        worker_transform.rotation = Quat::from_rotation_z(angle);
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

                if let Ok((_, b_tf, b, mut hp, b_rad, b_fac, ..)) = building_query.get_mut(b_ent) {
                    if *b_fac != *faction || !b.is_constructed || hp.current >= hp.max {
                        worker.target_building = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = worker_transform.translation.truncate();
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
                        worker_transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
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
                worker.carried_minerals = 0;

                let Some(node_entity) = worker.target_node else {
                    worker.mining_spot_index = None;
                    worker.state = WorkerState::Idle;
                    continue;
                };

                let Ok((_, node_transform, node)) = node_query.get_mut(node_entity) else {
                    // Node is gone or despawned
                    worker.mining_spot_index = None;
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if node.remaining_minerals == 0 {
                    // Node depleted, find another closest node
                    if let Some(spot) = worker.mining_spot_index {
                        if let Some(spots) = node_occupied_spots.get_mut(&node_entity) {
                            spots[spot] = false;
                        }
                    }
                    worker.mining_spot_index = None;
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                    continue;
                }

                let worker_pos = worker_transform.translation.truncate();
                let node_pos = node_transform.translation.truncate();

                let spot_idx = match worker.mining_spot_index {
                    Some(idx) => idx,
                    None => {
                        let spots = node_occupied_spots.entry(node_entity).or_insert([false; MINING_SPOTS_PER_ROCK]);
                        let chosen = find_closest_open_mining_spot(node_pos, worker_pos, spots);
                        spots[chosen] = true;
                        worker.mining_spot_index = Some(chosen);
                        chosen
                    }
                };

                let spot_pos = get_mining_spot_position(node_pos, spot_idx);
                let dist = worker_pos.distance(spot_pos);

                if dist <= (move_speed.0 * dt).max(6.0) {
                    // Arrived at designated mining spot, begin mining
                    worker_transform.translation.x = spot_pos.x;
                    worker_transform.translation.y = spot_pos.y;
                    worker.state = WorkerState::Mining;
                    worker.harvest_timer = 0.0;
                    worker.carried_minerals = 0;
                    let face_dir = (node_pos - spot_pos).normalize_or_zero();
                    if face_dir.length_squared() > 0.0 {
                        worker_transform.rotation = Quat::from_rotation_z(face_dir.y.atan2(face_dir.x));
                    }
                } else {
                    // Move towards spot
                    let dir = (spot_pos - worker_pos).normalize_or_zero();
                    let step = dir * move_speed.0 * dt;
                    worker_transform.translation.x += step.x;
                    worker_transform.translation.y += step.y;

                    // Face spot
                    if dir.length_squared() > 0.0 {
                        let angle = dir.y.atan2(dir.x);
                        worker_transform.rotation = Quat::from_rotation_z(angle);
                    }
                }
            }

            WorkerState::Mining => {
                let Some(node_entity) = worker.target_node else {
                    worker.mining_spot_index = None;
                    worker.state = WorkerState::Idle;
                    continue;
                };

                let Ok((_, node_transform, mut node)) = node_query.get_mut(node_entity) else {
                    if let Some(spot) = worker.mining_spot_index {
                        if let Some(spots) = node_occupied_spots.get_mut(&node_entity) {
                            spots[spot] = false;
                        }
                    }
                    worker.mining_spot_index = None;
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
                    if let Some(spot) = worker.mining_spot_index {
                        if let Some(spots) = node_occupied_spots.get_mut(&node_entity) {
                            spots[spot] = false;
                        }
                    }
                    worker.mining_spot_index = None;
                    if *faction == net_client.my_faction {
                        sound_events.send(SoundEffect::LaserMining);
                    }

                    // Find nearest friendly Base HQ
                    let worker_pos = worker_transform.translation.truncate();
                    let mut nearest_base = None;
                    let mut nearest_dist = f32::MAX;

                    for (base_entity, base_transform, base_faction, _) in &base_query {
                        let is_constructed = building_query.get(base_entity).map(|(_, _, b, ..)| b.is_constructed).unwrap_or(true);
                        if *base_faction == *faction && is_constructed {
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
                    if let Ok((_, base_transform, base_faction, _)) = base_query.get(base_entity) {
                        let is_constructed = building_query.get(base_entity).map(|(_, _, b, ..)| b.is_constructed).unwrap_or(true);
                        if *base_faction == *faction && is_constructed {
                            target_base_pos = Some(base_transform.translation.truncate());
                        }
                    }
                }

                if target_base_pos.is_none() {
                    let mut nearest_base = None;
                    let mut nearest_dist = f32::MAX;
                    for (b_ent, b_trans, b_fac, _) in &base_query {
                        let is_constructed = building_query.get(b_ent).map(|(_, _, b, ..)| b.is_constructed).unwrap_or(true);
                        if *b_fac == *faction && is_constructed {
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
                    if let Some(target) = worker.target_node {
                        if let Some(c) = node_worker_counts.get_mut(&target) {
                            *c = c.saturating_sub(1);
                        }
                        if let Some(spot) = worker.mining_spot_index {
                            if let Some(spots) = node_occupied_spots.get_mut(&target) {
                                spots[spot] = false;
                            }
                        }
                    }
                    worker.mining_spot_index = None;
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

                    // If original mineral patch still has minerals and friendly base nearby, return to it!
                    if let Some(node_entity) = worker.target_node {
                        let node_valid = if let Ok((_, node_tf, node)) = node_query.get(node_entity) {
                            if node.remaining_minerals > 0 {
                                let n_pos = node_tf.translation.truncate();
                                base_query.iter().any(|(b_ent, b_tf, b_fac, _)| {
                                    let is_constructed = building_query.get(b_ent).map(|(_, _, b, ..)| b.is_constructed).unwrap_or(true);
                                    *b_fac == *faction && is_constructed && n_pos.distance(b_tf.translation.truncate()) <= BASE_HQ_RESOURCE_RADIUS
                                })
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        if node_valid {
                            worker.state = WorkerState::MovingToResource;
                            worker.mining_spot_index = None;
                            if let Ok((_, node_tf, ..)) = node_query.get(node_entity) {
                                let n_pos = node_tf.translation.truncate();
                                let spots = node_occupied_spots.entry(node_entity).or_insert([false; MINING_SPOTS_PER_ROCK]);
                                let spot_idx = find_closest_open_mining_spot(n_pos, worker_pos, spots);
                                spots[spot_idx] = true;
                                worker.mining_spot_index = Some(spot_idx);
                                let spot_pos = get_mining_spot_position(n_pos, spot_idx);
                                let dir = (spot_pos - worker_pos).normalize_or_zero();
                                if dir.length_squared() > 0.0 {
                                    worker_transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                                }
                            }
                            continue;
                        }

                        // Node is depleted or lacks friendly Base HQ: become Idle
                        if let Some(c) = node_worker_counts.get_mut(&node_entity) {
                            *c = c.saturating_sub(1);
                        }
                        if let Some(spot) = worker.mining_spot_index {
                            if let Some(spots) = node_occupied_spots.get_mut(&node_entity) {
                                spots[spot] = false;
                            }
                        }
                        worker.mining_spot_index = None;
                        worker.target_node = None;
                        worker.state = WorkerState::Idle;
                    } else {
                        worker.mining_spot_index = None;
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

/// Determines if a worker's mining visuals (pickaxe animation, sparks, carried gold nugget)
/// should be rendered based on player faction and fog of war visibility.
pub fn should_render_mining_visuals(
    faction: Faction,
    worker_pos: Vec2,
    my_faction: Faction,
    fog: &FogOfWarGrid,
    config: &WorldGridConfig,
) -> bool {
    // Shroud hostile workers outside active friendly vision
    if faction != my_faction && faction != Faction::Neutral
        && fog.get_state_at_world_pos(worker_pos, config) != FogState::Visible
    {
        return false;
    }
    true
}

/// Renders the pulsating golden mining laser and carried gold nugget
fn draw_mining_visuals(
    time: Res<Time>,
    mut gizmos: Gizmos,
    fog: Res<FogOfWarGrid>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    net_client: Res<NetClient>,
    worker_query: Query<(&Transform, &Worker, &Faction)>,
    _node_query: Query<(Entity, &Transform, &ResourceNode)>,
) {
    let t = time.elapsed_secs();
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);

    for (worker_transform, worker, faction) in &worker_query {
        let worker_pos = worker_transform.translation.truncate();

        if !should_render_mining_visuals(*faction, worker_pos, net_client.my_faction, &fog, config) {
            continue;
        }

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
        } else if worker.state == WorkerState::Building {
            // Welding Torch & Electric Blue/White Arc Visuals
            let torch_len = 16.0;
            let torch_base = worker_pos + forward * 6.0;
            let torch_tip = torch_base + forward * torch_len;

            // Worker arms holding welding torch
            let left_shoulder = worker_pos + forward * 4.0 - right_side * 4.5;
            let right_shoulder = worker_pos + forward * 5.0 + right_side * 4.5;
            gizmos.line_2d(left_shoulder, torch_base, Color::srgb(0.95, 0.75, 0.20));
            gizmos.line_2d(right_shoulder, torch_base, Color::srgb(0.95, 0.75, 0.20));

            // Torch body & nozzle
            gizmos.line_2d(torch_base, torch_tip, Color::srgb(0.35, 0.38, 0.45));
            gizmos.circle_2d(torch_tip, 2.0, Color::srgb(0.80, 0.40, 0.15)); // Brass nozzle

            // Electric blue/white arc flash (pulsates rapidly)
            let arc_pulse = ((t * 28.0).sin() * 0.5 + 0.5).powi(2);
            let arc_radius = 3.5 + arc_pulse * 3.5;
            gizmos.circle_2d(torch_tip, arc_radius, Color::srgba(0.30, 0.85, 1.0, 0.85)); // Cyan glow
            gizmos.circle_2d(torch_tip, arc_radius * 0.5, Color::srgb(1.0, 1.0, 1.0)); // White hot center

            // Welding sparks spraying outward
            let spark_count = 5;
            for s in 0..spark_count {
                let s_angle = rot + ((s as f32) - 2.0) * 0.45 + (t * 50.0 + s as f32 * 1.7).sin() * 0.3;
                let s_dist = 6.0 + ((t * 40.0 + s as f32 * 3.1).cos().abs()) * 14.0;
                let spark_pos = torch_tip + Vec2::new(s_angle.cos(), s_angle.sin()) * s_dist;
                let spark_col = if s % 2 == 0 {
                    Color::srgb(1.0, 0.90, 0.40) // Yellow hot
                } else {
                    Color::srgb(0.40, 0.85, 1.0) // Electric blue
                };
                gizmos.circle_2d(spark_pos, 1.5, spark_col);
            }
        } else if worker.state == WorkerState::Repairing {
            // Repair Welding Torch & Emerald Green/Cyan Arc Visuals
            let torch_len = 16.0;
            let torch_base = worker_pos + forward * 6.0;
            let torch_tip = torch_base + forward * torch_len;

            // Worker arms holding welding torch
            let left_shoulder = worker_pos + forward * 4.0 - right_side * 4.5;
            let right_shoulder = worker_pos + forward * 5.0 + right_side * 4.5;
            gizmos.line_2d(left_shoulder, torch_base, Color::srgb(0.95, 0.75, 0.20));
            gizmos.line_2d(right_shoulder, torch_base, Color::srgb(0.95, 0.75, 0.20));

            // Torch body & nozzle
            gizmos.line_2d(torch_base, torch_tip, Color::srgb(0.35, 0.38, 0.45));
            gizmos.circle_2d(torch_tip, 2.0, Color::srgb(0.80, 0.40, 0.15)); // Brass nozzle

            // Emerald green arc flash (pulsates rapidly)
            let arc_pulse = ((t * 28.0).sin() * 0.5 + 0.5).powi(2);
            let arc_radius = 3.5 + arc_pulse * 3.5;
            gizmos.circle_2d(torch_tip, arc_radius, Color::srgba(0.20, 0.95, 0.45, 0.85)); // Emerald glow
            gizmos.circle_2d(torch_tip, arc_radius * 0.5, Color::srgb(1.0, 1.0, 1.0)); // White hot center

            // Repair welding sparks spraying outward
            let spark_count = 5;
            for s in 0..spark_count {
                let s_angle = rot + ((s as f32) - 2.0) * 0.45 + (t * 50.0 + s as f32 * 1.7).sin() * 0.3;
                let s_dist = 6.0 + ((t * 40.0 + s as f32 * 3.1).cos().abs()) * 14.0;
                let spark_pos = torch_tip + Vec2::new(s_angle.cos(), s_angle.sin()) * s_dist;
                let spark_col = if s % 2 == 0 {
                    Color::srgb(0.25, 0.95, 0.50) // Emerald green spark
                } else {
                    Color::srgb(0.80, 0.95, 0.30) // Yellow-green spark
                };
                gizmos.circle_2d(spark_pos, 1.5, spark_col);
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

        // 2. Draw Carried Gold Nugget on Worker (strictly when actively hauling gold back to Base HQ)
        if worker.state == WorkerState::MovingToBase && worker.carried_minerals > 0 {
            // Center the chunky nugget on the worker's back oriented along heading
            let nugget_center = worker_pos - forward * 7.0;
            let gold_bright = Color::srgb(1.0, 0.84, 0.18);
            let gold_highlight = Color::srgb(1.0, 0.98, 0.65);
            let gold_shadow = Color::srgb(0.75, 0.52, 0.10);

            // Chunky faceted golden nugget oriented with worker heading
            let raw_offsets = [
                Vec2::new(-2.0, 7.0),
                Vec2::new(4.5, 5.0),
                Vec2::new(6.0, -1.5),
                Vec2::new(2.5, -6.5),
                Vec2::new(-4.0, -5.0),
                Vec2::new(-6.0, 1.5),
            ];
            let pts: Vec<Vec2> = raw_offsets.iter().map(|o| {
                nugget_center + right_side * o.x + forward * o.y
            }).collect();

            for i in 0..pts.len() {
                let next = (i + 1) % pts.len();
                gizmos.line_2d(pts[i], pts[next], gold_bright);
            }

            // Chiseled facets
            let apex = nugget_center + right_side * -0.5 + forward * 1.5;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friendly_and_neutral_workers_always_render_mining_visuals() {
        let fog = FogOfWarGrid::default();
        let config = WorldGridConfig::default();
        let pos = Vec2::new(0.0, 500.0);

        // Friendly worker renders regardless of fog
        assert!(should_render_mining_visuals(Faction::Player1, pos, Faction::Player1, &fog, &config));

        // Neutral entity renders
        assert!(should_render_mining_visuals(Faction::Neutral, pos, Faction::Player1, &fog, &config));
    }

    #[test]
    fn test_hostile_worker_mining_visuals_culled_in_fog_of_war() {
        let mut fog = FogOfWarGrid::default();
        let config = WorldGridConfig::default();
        let hostile_pos = Vec2::new(0.0, 500.0);

        let (cx, cy) = fog.world_to_grid(hostile_pos, &config).expect("Valid grid coords");

        // 1. Unexplored fog (cell = 0) -> Shrouded
        fog.set_state(cx, cy, FogState::Unexplored);
        assert!(!should_render_mining_visuals(Faction::Player2, hostile_pos, Faction::Player1, &fog, &config));
        assert!(!should_render_mining_visuals(Faction::HostileAi, hostile_pos, Faction::Player1, &fog, &config));

        // 2. Explored fog / Shroud of war (cell = 1) -> Shrouded
        fog.set_state(cx, cy, FogState::Explored);
        assert!(!should_render_mining_visuals(Faction::Player2, hostile_pos, Faction::Player1, &fog, &config));
        assert!(!should_render_mining_visuals(Faction::HostileAi, hostile_pos, Faction::Player1, &fog, &config));

        // 3. Actively visible under friendly vision (cell = 2) -> Rendered
        fog.set_state(cx, cy, FogState::Visible);
        assert!(should_render_mining_visuals(Faction::Player2, hostile_pos, Faction::Player1, &fog, &config));
        assert!(should_render_mining_visuals(Faction::HostileAi, hostile_pos, Faction::Player1, &fog, &config));
    }
}
