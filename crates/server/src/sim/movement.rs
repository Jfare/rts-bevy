use bevy::prelude::*;
use shared::components::*;
use shared::grid::NavGrid;

use crate::session::Matchmaker;

/// Server A* navigation grid obstacle updates
pub fn update_server_nav_grid_system(
    mut nav_grid: ResMut<NavGrid>,
    buildings: Query<(&Transform, &Radius, &Building)>,
    resources: Query<(&Transform, &Radius), With<ResourceNode>>,
) {
    nav_grid.clear();
    shared::map::mark_static_obstacles(&mut nav_grid);
    for (tf, radius, building) in &buildings {
        let pos = tf.translation.truncate();
        let r = if building.name.contains("Base HQ") {
            radius.0 + 8.0
        } else {
            radius.0 + 4.0
        };
        nav_grid.mark_circle(pos, r);
    }
    for (tf, radius) in &resources {
        let pos = tf.translation.truncate();
        nav_grid.mark_circle(pos, radius.0 + 4.0);
    }
}

/// Server ability timers (Stimpack, Siege Mode transitions) and Patrol cycling
pub fn server_abilities_and_stances_system(
    mut commands: Commands,
    time: Res<Time>,
    nav_grid: Res<NavGrid>,
    mut stim_query: Query<&mut Stimpack>,
    mut tank_query: Query<&mut SiegeTank>,
    mut stance_query: Query<(
        Entity,
        &Transform,
        &mut TacticalStance,
        Option<&MoveTarget>,
        Option<&mut Soldier>,
    ), With<Unit>>,
) {
    let dt = time.delta_secs();

    // 1. Stimpack timers
    for mut stim in &mut stim_query {
        if stim.is_active {
            stim.timer -= dt;
            if stim.timer <= 0.0 {
                stim.is_active = false;
                stim.timer = 0.0;
            }
        }
    }

    // 2. Siege Tank transformations
    for mut tank in &mut tank_query {
        match tank.mode {
            TankMode::TransformingToSiege => {
                tank.transform_timer -= dt;
                if tank.transform_timer <= 0.0 {
                    tank.mode = TankMode::Siege;
                    tank.transform_timer = 0.0;
                    tank.attack_range = 380.0;
                    tank.attack_damage = 70.0;
                    tank.attack_cooldown = 2.2;
                    tank.splash_radius = 45.0;
                }
            }
            TankMode::TransformingToTank => {
                tank.transform_timer -= dt;
                if tank.transform_timer <= 0.0 {
                    tank.mode = TankMode::Tank;
                    tank.transform_timer = 0.0;
                    tank.attack_range = 240.0;
                    tank.attack_damage = 35.0;
                    tank.attack_cooldown = 1.6;
                    tank.splash_radius = 0.0;
                }
            }
            _ => {}
        }
    }

    // 3. Patrol cycle
    for (entity, transform, mut stance, move_target_opt, mut soldier_opt) in &mut stance_query {
        if let TacticalStance::Patrol {
            origin,
            target,
            ref mut heading_to_target,
        } = *stance
        {
            if move_target_opt.is_none() {
                let current_pos = transform.translation.truncate();
                let next_dest = if *heading_to_target {
                    if current_pos.distance(target) <= 24.0 {
                        *heading_to_target = false;
                        origin
                    } else {
                        target
                    }
                } else if current_pos.distance(origin) <= 24.0 {
                    *heading_to_target = true;
                    target
                } else {
                    origin
                };

                let waypoints = nav_grid.find_path(current_pos, next_dest);
                commands.entity(entity).insert(MoveTarget::with_waypoints(
                    next_dest,
                    true,
                    waypoints,
                ));

                if let Some(ref mut soldier) = soldier_opt {
                    soldier.state = SoldierState::AttackMoving;
                }
            }
        }
    }
}

/// Authoritative unit steering and movement along waypoints
pub fn server_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    matchmaker: Res<Matchmaker>,
    mut query: Query<(
        Entity,
        &mut Transform,
        &MoveSpeed,
        &mut Velocity,
        &mut MoveTarget,
        &RoomId,
        Option<&Stimpack>,
        Option<&SiegeTank>,
        Option<&Soldier>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, speed, mut velocity, mut move_target, room_id, stim_opt, tank_opt, soldier_opt) in &mut query {
        let is_room_active = matchmaker.rooms.get(&room_id.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active {
            velocity.0 = Vec2::ZERO;
            continue;
        }

        if let Some(tank) = tank_opt {
            if tank.mode != TankMode::Tank {
                velocity.0 = Vec2::ZERO;
                continue;
            }
        }

        // If an attack-moving unit is currently fighting/engaging an enemy, pause marching
        if move_target.is_attack_move {
            if let Some(soldier) = soldier_opt {
                if soldier.target.is_some()
                    || soldier.state == SoldierState::Attacking
                    || soldier.state == SoldierState::ChasingTarget
                {
                    velocity.0 = Vec2::ZERO;
                    continue;
                }
            }
            if let Some(tank) = tank_opt {
                if tank.target.is_some() {
                    velocity.0 = Vec2::ZERO;
                    continue;
                }
            }
        }

        let current_pos = transform.translation.truncate();
        let goal_pos = move_target.current_goal();
        let diff = goal_pos - current_pos;
        let dist = diff.length();

        // Track stall / blockage against obstacles or friendly units
        if move_target.last_pos.x.is_nan() {
            move_target.last_pos = current_pos;
            move_target.stall_timer = 0.0;
        } else {
            let moved_dist = current_pos.distance(move_target.last_pos);
            if moved_dist < (speed.0 * 0.15 * dt).max(0.2) {
                move_target.stall_timer += dt;
            } else {
                move_target.stall_timer = 0.0;
                move_target.last_pos = current_pos;
            }
        }

        let is_final_waypoint = move_target.current_waypoint_idx >= (move_target.waypoints.len() - 1);

        // Advance waypoint if close to intermediate
        if !is_final_waypoint && dist <= 20.0 {
            move_target.advance_waypoint();
            move_target.stall_timer = 0.0;
            move_target.last_pos = current_pos;
            continue;
        }

        // Final arrival or stall clean removal
        if is_final_waypoint
            && (dist <= 12.0 || (dist <= 32.0 && move_target.stall_timer > 0.20) || move_target.stall_timer > 0.50) {
                velocity.0 = Vec2::ZERO;
                commands.entity(entity).remove::<MoveTarget>();
                continue;
            }

        let dir = diff.normalize_or_zero();
        let speed_mult = stim_opt
            .map(|s| if s.is_active { 1.5 } else { 1.0 })
            .unwrap_or(1.0);
        velocity.0 = dir * speed.0 * speed_mult;
        transform.translation.x += velocity.0.x * dt;
        transform.translation.y += velocity.0.y * dt;

        let angle = dir.y.atan2(dir.x);
        transform.rotation = Quat::from_rotation_z(angle);
    }
}

struct ServerUnitPosSnapshot {
    entity: Entity,
    pos: Vec2,
    radius: f32,
    faction: Faction,
    room_id: u32,
    is_active_worker: bool,
    is_moving: bool,
}

/// Dedicated server hard circle-circle unit collision and obstacle resolution
pub fn server_unit_separation_and_collision_system(
    mut unit_query: Query<(Entity, &mut Transform, &Radius, &Faction, &RoomId, Option<&Worker>, Option<&MoveTarget>), With<Unit>>,
    building_query: Query<(&Transform, &Radius, &Faction, &RoomId, Option<&BaseHQ>), (With<Building>, Without<Unit>)>,
    resource_query: Query<(&Transform, &Radius, &RoomId), (With<ResourceNode>, Without<Unit>)>,
) {
    let mut snapshots = Vec::with_capacity(unit_query.iter().len());
    for (entity, transform, radius, faction, room_id, worker_opt, move_opt) in &unit_query {
        let is_active_worker = worker_opt
            .map(|w| w.state != WorkerState::Idle)
            .unwrap_or(false);

        snapshots.push(ServerUnitPosSnapshot {
            entity,
            pos: transform.translation.truncate(),
            radius: radius.0,
            faction: *faction,
            room_id: room_id.0,
            is_active_worker,
            is_moving: move_opt.is_some(),
        });
    }

    // 1. Hard Unit-to-Unit Circle Collision per Room (2 solver iterations)
    for _iter in 0..2 {
        let mut deltas = vec![Vec2::ZERO; snapshots.len()];
        for i in 0..snapshots.len() {
            for j in (i + 1)..snapshots.len() {
                if snapshots[i].room_id != snapshots[j].room_id {
                    continue;
                }

                let u1_active = snapshots[i].is_active_worker;
                let u2_active = snapshots[j].is_active_worker;

                if u1_active && u2_active {
                    continue;
                }

                let p1 = snapshots[i].pos;
                let p2 = snapshots[j].pos;
                let delta = p1 - p2;
                let dist = delta.length();
                let min_dist = snapshots[i].radius + snapshots[j].radius;

                if dist < min_dist {
                    let overlap = min_dist - dist;
                    let dir = if dist > 0.001 {
                        delta / dist
                    } else {
                        let angle = ((snapshots[i].entity.index() + snapshots[j].entity.index()) as f32) * 1.5;
                        Vec2::new(angle.cos(), angle.sin())
                    };

                    let (w1, w2) = match (snapshots[i].is_moving, snapshots[j].is_moving) {
                        (true, false) => (0.75, 0.25),
                        (false, true) => (0.25, 0.75),
                        _ => (0.5, 0.5),
                    };

                    if !u1_active {
                        deltas[i] += dir * (overlap * w1);
                    }
                    if !u2_active {
                        deltas[j] -= dir * (overlap * w2);
                    }
                }
            }
        }
        for k in 0..snapshots.len() {
            snapshots[k].pos += deltas[k];
        }
    }

    // 2. Hard Obstacle Collision (Buildings & Mineral Nodes per Room)
    for snap in &mut snapshots {
        for (b_trans, b_radius, b_faction, b_room, base_hq_opt) in &building_query {
            if b_room.0 != snap.room_id {
                continue;
            }
            if snap.is_active_worker && base_hq_opt.is_some() && b_faction == &snap.faction {
                continue;
            }

            let b_pos = b_trans.translation.truncate();
            let d = snap.pos.distance(b_pos);
            let min_b_dist = snap.radius + b_radius.0;

            if d < min_b_dist {
                let push_dir = if d > 0.001 {
                    (snap.pos - b_pos) / d
                } else {
                    Vec2::new(0.0, 1.0)
                };
                snap.pos = b_pos + push_dir * min_b_dist;
            }
        }

        if !snap.is_active_worker {
            for (r_trans, r_radius, r_room) in &resource_query {
                if r_room.0 != snap.room_id {
                    continue;
                }
                let r_pos = r_trans.translation.truncate();
                let d = snap.pos.distance(r_pos);
                let min_r_dist = snap.radius + r_radius.0;

                if d < min_r_dist {
                    let push_dir = if d > 0.001 {
                        (snap.pos - r_pos) / d
                    } else {
                        Vec2::new(0.0, 1.0)
                    };
                    snap.pos = r_pos + push_dir * min_r_dist;
                }
            }
        }

        // Push away from static map obstacles (rocks, cliff bluffs)
        for obs in shared::map::STATIC_MAP_OBSTACLES {
            let d = snap.pos.distance(obs.position);
            let min_obs_dist = snap.radius + obs.radius;

            if d < min_obs_dist {
                let push_dir = if d > 0.001 {
                    (snap.pos - obs.position) / d
                } else {
                    Vec2::new(0.0, 1.0)
                };
                snap.pos = obs.position + push_dir * min_obs_dist;
            }
        }
    }

    // 3. Write back resolved positions
    for (i, (_entity, mut transform, ..)) in unit_query.iter_mut().enumerate() {
        if i < snapshots.len() {
            transform.translation.x = snapshots[i].pos.x;
            transform.translation.y = snapshots[i].pos.y;
        }
    }
}

