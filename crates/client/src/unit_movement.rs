use bevy::prelude::*;
use shared::components::{
    AppState, BaseHQ, Building, Faction, MatchOutcome, MoveSpeed, MoveTarget, Radius, ResourceNode, SiegeTank,
    Soldier, SoldierState, Stimpack, TacticalStance, TankMode, Unit, Worker, WorkerState,
};
use shared::grid::NavGrid;

pub struct UnitMovementPlugin;

impl Plugin for UnitMovementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavGrid>()
            .add_systems(
                Update,
                (
                    update_nav_grid_system,
                    unit_movement_system,
                    update_tactical_stances_and_abilities_system,
                    unit_separation_and_collision_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Dynamically updates the A* Navigation Grid with building and mineral obstacles
fn update_nav_grid_system(
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

/// Moves units smoothly along A* waypoints towards their MoveTarget destination with orientation
fn unit_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    outcome_opt: Option<Res<MatchOutcome>>,
    mut query: Query<(
        Entity,
        &mut Transform,
        &mut MoveTarget,
        &MoveSpeed,
        Option<&Stimpack>,
        Option<&SiegeTank>,
        Option<&Soldier>,
    )>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory) || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat) {
        return;
    }

    let dt = time.delta_secs();

    for (entity, mut transform, mut move_target, move_speed, stim_opt, tank_opt, soldier_opt) in &mut query {
        // Immobilize Siege Tanks when in Siege Mode or Transforming
        if let Some(tank) = tank_opt {
            if tank.mode != TankMode::Tank {
                continue;
            }
        }

        // If an attack-moving unit is currently engaging / fighting an enemy target, pause waypoint marching
        if move_target.is_attack_move {
            if let Some(soldier) = soldier_opt {
                if soldier.target.is_some()
                    || soldier.state == SoldierState::Attacking
                    || soldier.state == SoldierState::ChasingTarget
                {
                    continue;
                }
            }
            if let Some(tank) = tank_opt {
                if tank.target.is_some() {
                    continue;
                }
            }
        }

        let current_pos = transform.translation.truncate();
        let goal_pos = move_target.current_goal();
        let delta = goal_pos - current_pos;
        let dist = delta.length();

        // Track stall / blockage against obstacles or friendly units
        if move_target.last_pos.x.is_nan() {
            move_target.last_pos = current_pos;
            move_target.stall_timer = 0.0;
        } else {
            let moved_dist = current_pos.distance(move_target.last_pos);
            if moved_dist < (move_speed.0 * 0.15 * dt).max(0.2) {
                move_target.stall_timer += dt;
            } else {
                move_target.stall_timer = 0.0;
                move_target.last_pos = current_pos;
            }
        }

        let is_final_waypoint = move_target.current_waypoint_idx >= (move_target.waypoints.len() - 1);

        // If close to intermediate waypoint, advance to next waypoint
        if !is_final_waypoint && dist <= 20.0 {
            move_target.advance_waypoint();
            move_target.stall_timer = 0.0;
            move_target.last_pos = current_pos;
            continue;
        }

        // Final destination reached or cleanly settled against obstacle
        if is_final_waypoint
            && (dist <= 12.0 || (dist <= 32.0 && move_target.stall_timer > 0.20) || move_target.stall_timer > 0.50) {
                commands.entity(entity).remove::<MoveTarget>();
                continue;
            }

        let direction = delta.normalize_or_zero();
        let speed_mult = stim_opt
            .map(|s| if s.is_active { 1.5 } else { 1.0 })
            .unwrap_or(1.0);

        let move_amount = (move_speed.0 * speed_mult * dt).min(dist);
        transform.translation.x += direction.x * move_amount;
        transform.translation.y += direction.y * move_amount;

        // Rotate facing direction smoothly towards movement vector
        if direction.length_squared() > 0.001 {
            let target_angle = direction.y.atan2(direction.x);
            let current_angle = transform.rotation.to_euler(EulerRot::ZYX).0;
            let new_angle = current_angle + (target_angle - current_angle) * (dt * 14.0).min(1.0);
            transform.rotation = Quat::from_rotation_z(new_angle);
        }
    }
}

/// Updates active ability durations (Stimpack, Siege Mode) and patrol cycling
fn update_tactical_stances_and_abilities_system(
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

    // 1. Update Stimpack timers
    for mut stim in &mut stim_query {
        if stim.is_active {
            stim.timer -= dt;
            if stim.timer <= 0.0 {
                stim.is_active = false;
                stim.timer = 0.0;
            }
        }
    }

    // 2. Update Siege Tank transformation transitions
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

    // 3. Update Patrol stance cycling when idle
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

/// Snapshot struct for spatial queries without component borrowing conflicts
struct UnitPosSnapshot {
    entity: Entity,
    pos: Vec2,
    radius: f32,
    faction: Faction,
    is_active_worker: bool,
    is_moving: bool,
}

/// Hard circle-circle separation between units and hard obstacle collision against buildings/minerals
fn unit_separation_and_collision_system(
    mut unit_query: Query<(Entity, &mut Transform, &Radius, &Faction, Option<&Worker>, Option<&MoveTarget>), With<Unit>>,
    building_query: Query<(&Transform, &Radius, &Faction, Option<&BaseHQ>), (With<Building>, Without<Unit>)>,
    resource_query: Query<(&Transform, &Radius), (With<ResourceNode>, Without<Unit>)>,
) {
    let mut snapshots = Vec::with_capacity(unit_query.iter().len());
    for (entity, transform, radius, faction, worker_opt, move_opt) in &unit_query {
        let is_active_worker = worker_opt
            .map(|w| w.state != WorkerState::Idle)
            .unwrap_or(false);

        snapshots.push(UnitPosSnapshot {
            entity,
            pos: transform.translation.truncate(),
            radius: radius.0,
            faction: *faction,
            is_active_worker,
            is_moving: move_opt.is_some(),
        });
    }

    // 1. Hard Unit-to-Unit Circle Collision (2 solver iterations for physical solidity)
    for _iter in 0..2 {
        let mut deltas = vec![Vec2::ZERO; snapshots.len()];
        for i in 0..snapshots.len() {
            for j in (i + 1)..snapshots.len() {
                let u1_active = snapshots[i].is_active_worker;
                let u2_active = snapshots[j].is_active_worker;

                // Active harvesting workers pass through each other to avoid mineral patch lockup
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

    // 2. Hard Obstacle Collision (Buildings & Resource Nodes)
    for snap in &mut snapshots {
        // Push away from buildings (ignoring friendly BaseHQ for active returning workers)
        for (b_trans, b_radius, b_faction, base_hq_opt) in &building_query {
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

        // Push away from mineral nodes (skipping active workers assigned to harvest)
        if !snap.is_active_worker {
            for (r_trans, r_radius) in &resource_query {
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

    // 3. Write back resolved positions to entities
    for (i, (_entity, mut transform, ..)) in unit_query.iter_mut().enumerate() {
        if i < snapshots.len() {
            transform.translation.x = snapshots[i].pos.x;
            transform.translation.y = snapshots[i].pos.y;
        }
    }
}
