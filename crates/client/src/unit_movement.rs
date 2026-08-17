use bevy::prelude::*;
use shared::components::{BaseHQ, Building, Faction, MoveSpeed, MoveTarget, Radius, ResourceNode, Unit, Worker, WorkerState};

pub struct UnitMovementPlugin;

impl Plugin for UnitMovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                unit_movement_system,
                unit_separation_and_collision_system,
            ),
        );
    }
}

/// Moves units smoothly towards their MoveTarget destination with orientation
fn unit_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Transform, &MoveTarget, &MoveSpeed, &Radius)>,
) {
    let dt = time.delta_secs();

    for (entity, mut transform, move_target, move_speed, _radius) in &mut query {
        let current_pos = transform.translation.truncate();
        let target_pos = move_target.destination;
        let delta = target_pos - current_pos;
        let dist = delta.length();

        if dist <= 6.0 {
            // Arrived at destination
            commands.entity(entity).remove::<MoveTarget>();
            continue;
        }

        let direction = delta / dist;
        let move_amount = (move_speed.0 * dt).min(dist);
        transform.translation.x += direction.x * move_amount;
        transform.translation.y += direction.y * move_amount;

        // Rotate facing direction smoothly towards movement vector
        let target_angle = direction.y.atan2(direction.x);
        let current_angle = transform.rotation.to_euler(EulerRot::ZYX).0;
        let new_angle = current_angle + (target_angle - current_angle) * (dt * 14.0).min(1.0);
        transform.rotation = Quat::from_rotation_z(new_angle);
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

            // If both are active workers mining/returning cargo, allow them to pass through each other smoothly
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
                    // Small offset if exactly superimposed
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
                // Active workers delivering cargo need to approach their friendly BaseHQ without collision pushback
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
