use bevy::prelude::*;
use shared::components::{Building, MoveSpeed, MoveTarget, Radius, ResourceNode, Unit};

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
}

/// Soft elastic separation between overlapping units and obstacle collision against buildings/minerals
fn unit_separation_and_collision_system(
    time: Res<Time>,
    mut unit_query: Query<(Entity, &mut Transform, &Radius), With<Unit>>,
    building_query: Query<(&Transform, &Radius), (With<Building>, Without<Unit>)>,
    resource_query: Query<(&Transform, &Radius), (With<ResourceNode>, Without<Unit>)>,
) {
    let dt = time.delta_secs().min(0.05);

    // 1. Snapshot all unit positions
    let mut snapshots = Vec::with_capacity(unit_query.iter().len());
    for (entity, transform, radius) in &unit_query {
        snapshots.push(UnitPosSnapshot {
            entity,
            pos: transform.translation.truncate(),
            radius: radius.0,
        });
    }

    // 2. Compute separation forces between overlapping units
    let mut push_deltas: Vec<Vec2> = vec![Vec2::ZERO; snapshots.len()];

    for i in 0..snapshots.len() {
        for j in (i + 1)..snapshots.len() {
            let u1 = &snapshots[i];
            let u2 = &snapshots[j];

            let delta = u1.pos - u2.pos;
            let dist = delta.length();
            let min_dist = u1.radius + u2.radius;

            if dist < min_dist {
                let overlap = min_dist - dist;
                let dir = if dist > 0.001 {
                    delta / dist
                } else {
                    // Random small offset if exactly superimposed
                    let angle = ((u1.entity.index() + u2.entity.index()) as f32) * 1.5;
                    Vec2::new(angle.cos(), angle.sin())
                };

                let push = dir * overlap * 0.5 * (dt * 16.0).min(1.0);
                push_deltas[i] += push;
                push_deltas[j] -= push;
            }
        }
    }

    // 3. Apply unit pushes and resolve collision with buildings & mineral nodes
    for (i, (_entity, mut transform, radius)) in unit_query.iter_mut().enumerate() {
        if i < push_deltas.len() {
            transform.translation.x += push_deltas[i].x;
            transform.translation.y += push_deltas[i].y;
        }

        let mut unit_pos = transform.translation.truncate();
        let u_radius = radius.0;

        // Push away from buildings
        for (b_trans, b_radius) in &building_query {
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

        // Push away from mineral nodes
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
