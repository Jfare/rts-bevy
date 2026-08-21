use bevy::prelude::*;
use shared::components::{
    BaseHQ, Building, Faction, MoveSpeed, MoveTarget, Radius, ResourceNode, SiegeTank,
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
                ),
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
    mut query: Query<(
        Entity,
        &mut Transform,
        &mut MoveTarget,
        &MoveSpeed,
        Option<&Stimpack>,
        Option<&SiegeTank>,
    )>,
) {
    let dt = time.delta_secs();

    for (entity, mut transform, mut move_target, move_speed, stim_opt, tank_opt) in &mut query {
        // Immobilize Siege Tanks when in Siege Mode or Transforming
        if let Some(tank) = tank_opt {
            if tank.mode != TankMode::Tank {
                continue;
            }
        }

        let current_pos = transform.translation.truncate();
        let goal_pos = move_target.current_goal();
        let delta = goal_pos - current_pos;
        let dist = delta.length();

        // If close to intermediate waypoint, advance to next waypoint
        if dist <= 14.0 && move_target.current_waypoint_idx < (move_target.waypoints.len() - 1) {
            move_target.advance_waypoint();
            continue;
        }

        // Final destination reached
        if dist <= 8.0 && move_target.current_waypoint_idx >= (move_target.waypoints.len() - 1) {
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
    is_active_worker: bool,
}

/// Soft elastic separation between overlapping units and obstacle collision against buildings/minerals
fn unit_separation_and_collision_system(
    time: Res<Time>,
    mut unit_query: Query<(Entity, &mut Transform, &Radius, &Faction, Option<&Worker>), With<Unit>>,
    building_query: Query<(&Transform, &Radius, &Faction, Option<&BaseHQ>), (With<Building>, Without<Unit>)>,
    resource_query: Query<(&Transform, &Radius), (With<ResourceNode>, Without<Unit>)>,
) {
    let dt = time.delta_secs().min(0.05);

    // 1. Snapshot all unit positions
    let mut snapshots = Vec::with_capacity(unit_query.iter().len());
    for (entity, transform, radius, _faction, worker_opt) in &unit_query {
        let is_active_worker = worker_opt
            .map(|w| w.state != WorkerState::Idle)
            .unwrap_or(false);

        snapshots.push(UnitPosSnapshot {
            entity,
            pos: transform.translation.truncate(),
            radius: radius.0,
            is_active_worker,
        });
    }

    // 2. Compute separation forces between overlapping units (skipping active mining/harvesting workers)
    let mut push_deltas: Vec<Vec2> = vec![Vec2::ZERO; snapshots.len()];

    for i in 0..snapshots.len() {
        for j in (i + 1)..snapshots.len() {
            let u1 = &snapshots[i];
            let u2 = &snapshots[j];

            if u1.is_active_worker && u2.is_active_worker {
                continue;
            }

            let delta = u1.pos - u2.pos;
            let dist = delta.length();
            let min_dist = u1.radius + u2.radius;

            if dist < min_dist {
                let overlap = min_dist - dist;
                let dir = if dist > 0.001 {
                    delta / dist
                } else {
                    let angle = ((u1.entity.index() + u2.entity.index()) as f32) * 1.5;
                    Vec2::new(angle.cos(), angle.sin())
                };

                let push = dir * overlap * 2.0 * dt;
                if !u1.is_active_worker {
                    push_deltas[i] += push;
                }
                if !u2.is_active_worker {
                    push_deltas[j] -= push;
                }
            }
        }
    }

    // 3. Apply unit pushes and resolve collision with buildings & mineral nodes
    for (i, (_entity, mut transform, radius, faction, worker_opt)) in unit_query.iter_mut().enumerate() {
        let is_active_worker = worker_opt
            .map(|w| w.state != WorkerState::Idle)
            .unwrap_or(false);

        if i < push_deltas.len() {
            transform.translation.x += push_deltas[i].x;
            transform.translation.y += push_deltas[i].y;
        }

        let mut unit_pos = transform.translation.truncate();
        let u_radius = radius.0;

        // Push away from buildings (ignoring friendly BaseHQ for active returning workers)
        for (b_trans, b_radius, b_faction, base_hq_opt) in &building_query {
            if is_active_worker && base_hq_opt.is_some() && b_faction == faction {
                continue;
            }

            let b_pos = b_trans.translation.truncate();
            let d = unit_pos.distance(b_pos);
            let min_b_dist = u_radius + b_radius.0 + 2.0;

            if d < min_b_dist && d > 0.001 {
                let push_dir = (unit_pos - b_pos) / d;
                let push_amount = min_b_dist - d;
                unit_pos += push_dir * push_amount;
                transform.translation.x = unit_pos.x;
                transform.translation.y = unit_pos.y;
            }
        }

        // Push away from mineral nodes (skipping active workers assigned to harvest)
        if !is_active_worker {
            for (r_trans, r_radius) in &resource_query {
                let r_pos = r_trans.translation.truncate();
                let d = unit_pos.distance(r_pos);
                let min_r_dist = u_radius + r_radius.0 + 2.0;

                if d < min_r_dist && d > 0.001 {
                    let push_dir = (unit_pos - r_pos) / d;
                    let push_amount = min_r_dist - d;
                    unit_pos += push_dir * push_amount;
                    transform.translation.x = unit_pos.x;
                    transform.translation.y = unit_pos.y;
                }
            }
        }
    }
}
