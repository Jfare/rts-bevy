use bevy::prelude::*;
use shared::components::*;
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::particles::ParticleEvent;
use super::TargetSnapshot;

/// Ranged Fighter Combat State Machine with Hold Position support
pub fn ranged_fighter_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    outcome_opt: Option<Res<MatchOutcome>>,
    net_client_opt: Option<Res<NetClient>>,
    mut sound_events: EventWriter<SoundEffect>,
    mut particle_events: EventWriter<ParticleEvent>,
    mut queries: ParamSet<(
        Query<(Entity, &Transform, &Radius, &Faction, &Health)>,
        Query<(
            Entity,
            &mut Soldier,
            &mut Transform,
            &MoveSpeed,
            &Faction,
            &Radius,
            Option<&mut MoveTarget>,
            Option<&TacticalStance>,
        )>,
    )>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory) || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat) {
        return;
    }

    let dt = time.delta_secs();
    let is_online = net_client_opt.as_ref().map(|n| n.status == NetStatus::InGame).unwrap_or(false);

    let targets: Vec<TargetSnapshot> = queries
        .p0()
        .iter()
        .map(|(entity, transform, radius, faction, health)| TargetSnapshot {
            entity,
            pos: transform.translation.truncate(),
            radius: radius.0,
            faction: *faction,
            is_dead: health.is_dead(),
        })
        .collect();

    // 2. Update all ranged fighters using the snapshot
    for (soldier_entity, mut soldier, mut soldier_transform, move_speed, faction, radius, move_target_opt, stance_opt) in
        &mut queries.p1()
    {
        soldier.attack_timer += dt;
        soldier.scan_timer += dt;
        let soldier_pos = soldier_transform.translation.truncate();

        let is_hold_pos = stance_opt.map(|s| *s == TacticalStance::HoldPosition).unwrap_or(false)
            || soldier.state == SoldierState::HoldingPosition;

        let is_attack_move = move_target_opt.as_ref().map(|m| m.is_attack_move).unwrap_or(false);

        // If unit has an active pure ground move / retreat order, do NOT auto-acquire enemies or stop to attack:
        if move_target_opt.is_some() && !is_attack_move {
            soldier.target = None;
            soldier.state = SoldierState::MovingToGround;
            continue;
        }

        // Validate existing target
        let current_target_snapshot = soldier.target.and_then(|target_entity| {
            targets
                .iter()
                .find(|t| t.entity == target_entity && !t.is_dead && faction.is_hostile_to(&t.faction))
        });

        if let Some(target_snapshot) = current_target_snapshot {
            let target_pos = target_snapshot.pos;
            let dist = target_pos.distance(soldier_pos);
            let effective_range = soldier.attack_range + target_snapshot.radius;
            let dir = (target_pos - soldier_pos).normalize_or_zero();

            if dir.length_squared() > 0.001 {
                let angle = dir.y.atan2(dir.x);
                soldier_transform.rotation = Quat::from_rotation_z(angle);
            }

            if dist <= effective_range {
                soldier.state = SoldierState::Attacking;

                if soldier.attack_timer >= soldier.attack_cooldown {
                    soldier.attack_timer = 0.0;

                    // In online multiplayer, the dedicated server fires authoritative projectiles
                    if !is_online {
                        let muzzle_start = soldier_pos + dir * (radius.0 + 8.0);

                        commands.spawn((
                            Projectile {
                                origin: muzzle_start,
                                target_entity: Some(target_snapshot.entity),
                                target_pos,
                                speed: 780.0,
                                damage: soldier.attack_damage,
                                splash_radius: 0.0,
                                faction: *faction,
                                lifetime: 0.0,
                                max_lifetime: 0.65,
                            },
                            Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.0),
                        ));

                        commands.spawn((
                            MuzzleFlash {
                                lifetime: 0.0,
                                max_lifetime: 0.07,
                                color: Color::srgb(1.0, 0.85, 0.35),
                            },
                            Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.1),
                        ));

                        particle_events.send(ParticleEvent::MuzzleSmoke {
                            pos: muzzle_start,
                            dir,
                        });

                        sound_events.send(SoundEffect::Gunshot);
                    }
                }
            } else if is_hold_pos {
                soldier.state = SoldierState::HoldingPosition;
            } else {
                soldier.state = SoldierState::ChasingTarget;
                let stop_dist = (effective_range * 0.90).max(10.0);
                let travel_needed = (dist - stop_dist).max(0.0);
                let step = dir * (move_speed.0 * dt).min(travel_needed);
                soldier_transform.translation.x += step.x;
                soldier_transform.translation.y += step.y;
            }
        } else {
            // Target is invalid/dead: scan for new hostile targets
            soldier.target = None;

            let max_scan_range = if is_attack_move {
                soldier.aggro_radius
            } else {
                soldier.attack_range
            };

            let mut closest_target = None;
            let mut min_dist = max_scan_range;

            for t in &targets {
                if t.entity != soldier_entity && faction.is_hostile_to(&t.faction) && !t.is_dead {
                    let d = t.pos.distance(soldier_pos);
                    let effective_range = max_scan_range + t.radius;
                    if d <= effective_range && d < min_dist {
                        min_dist = d;
                        closest_target = Some(t);
                    }
                }
            }

            if let Some(target) = closest_target {
                soldier.target = Some(target.entity);
                let target_pos = target.pos;
                let dist = target_pos.distance(soldier_pos);
                let effective_range = soldier.attack_range + target.radius;
                let dir = (target_pos - soldier_pos).normalize_or_zero();

                if dir.length_squared() > 0.001 {
                    let angle = dir.y.atan2(dir.x);
                    soldier_transform.rotation = Quat::from_rotation_z(angle);
                }

                if dist <= effective_range {
                    soldier.state = SoldierState::Attacking;

                    if soldier.attack_timer >= soldier.attack_cooldown {
                        soldier.attack_timer = 0.0;

                        if !is_online {
                            let muzzle_start = soldier_pos + dir * (radius.0 + 8.0);

                            commands.spawn((
                                Projectile {
                                    origin: muzzle_start,
                                    target_entity: Some(target.entity),
                                    target_pos,
                                    speed: 780.0,
                                    damage: soldier.attack_damage,
                                    splash_radius: 0.0,
                                    faction: *faction,
                                    lifetime: 0.0,
                                    max_lifetime: 0.65,
                                },
                                Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.0),
                            ));

                            commands.spawn((
                                MuzzleFlash {
                                    lifetime: 0.0,
                                    max_lifetime: 0.07,
                                    color: Color::srgb(1.0, 0.85, 0.35),
                                    },
                                Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.1),
                            ));

                            particle_events.send(ParticleEvent::MuzzleSmoke {
                                pos: muzzle_start,
                                dir,
                            });

                            sound_events.send(SoundEffect::Gunshot);
                        }
                    }
                } else if is_hold_pos {
                    soldier.state = SoldierState::HoldingPosition;
                } else {
                    soldier.state = SoldierState::ChasingTarget;
                    let stop_dist = (effective_range * 0.90).max(10.0);
                    let travel_needed = (dist - stop_dist).max(0.0);
                    let step = dir * (move_speed.0 * dt).min(travel_needed);
                    soldier_transform.translation.x += step.x;
                    soldier_transform.translation.y += step.y;
                }
            } else if is_attack_move {
                soldier.state = SoldierState::AttackMoving;
            } else if is_hold_pos {
                soldier.state = SoldierState::HoldingPosition;
            } else {
                soldier.state = SoldierState::Idle;
            }
        }
    }
}
