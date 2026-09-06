use bevy::prelude::*;
use shared::components::*;
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::particles::ParticleEvent;
use super::TargetSnapshot;

/// Siege Tank Combat & Artillery System
pub fn siege_tank_combat_system(
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
            &mut SiegeTank,
            &mut Transform,
            &Faction,
            &Radius,
            &MoveSpeed,
            Option<&MoveTarget>,
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

    for (tank_ent, mut tank, mut tank_tf, faction, radius, move_speed, move_target_opt, stance_opt) in &mut queries.p1() {
        tank.attack_timer += dt;

        // Handle transformation timer
        if tank.mode == TankMode::TransformingToSiege {
            tank.transform_timer -= dt;
            if tank.transform_timer <= 0.0 {
                tank.mode = TankMode::Siege;
                tank.attack_range = 380.0;
                tank.attack_damage = 70.0;
                tank.attack_cooldown = 2.2;
                tank.transform_timer = 0.0;
            }
        } else if tank.mode == TankMode::TransformingToTank {
            tank.transform_timer -= dt;
            if tank.transform_timer <= 0.0 {
                tank.mode = TankMode::Tank;
                tank.attack_range = 240.0;
                tank.attack_damage = 35.0;
                tank.attack_cooldown = 1.3;
                tank.transform_timer = 0.0;
            }
        }

        let is_hold_pos = stance_opt.map(|s| *s == TacticalStance::HoldPosition).unwrap_or(false)
            || tank.mode == TankMode::Siege
            || tank.mode == TankMode::TransformingToSiege
            || tank.mode == TankMode::TransformingToTank;

        let is_siege = tank.mode == TankMode::Siege;
        let tank_pos = tank_tf.translation.truncate();
        let is_attack_move = move_target_opt.as_ref().map(|m| m.is_attack_move).unwrap_or(false);

        // If tank is in mobile tank mode and has a pure ground move order, ignore combat and move!
        if move_target_opt.is_some() && !is_attack_move && tank.mode == TankMode::Tank {
            tank.target = None;
            continue;
        }

        let current_target_snapshot = tank.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead && faction.is_hostile_to(&t.faction))
        });

        if let Some(target_snap) = current_target_snapshot {
            let target_pos = target_snap.pos;
            let dir = (target_pos - tank_pos).normalize_or_zero();
            tank.turret_angle = dir.y.atan2(dir.x);

            let dist = tank_pos.distance(target_pos);
            let effective_range = tank.attack_range + target_snap.radius;

            if dist <= effective_range {
                if !is_online
                    && tank.attack_timer >= tank.attack_cooldown {
                        tank.attack_timer = 0.0;
                        sound_events.send(SoundEffect::SiegeTankShot);

                        let muzzle_dist = if is_siege { radius.0 * 2.2 } else { radius.0 * 1.5 };
                        let muzzle_start = tank_pos + dir * muzzle_dist;

                        particle_events.send(ParticleEvent::MuzzleSmoke {
                            pos: muzzle_start,
                            dir,
                        });

                        if is_siege {
                            particle_events.send(ParticleEvent::Shockwave {
                                pos: muzzle_start,
                                radius: 25.0,
                                color: Color::srgba(1.0, 0.6, 0.2, 0.8),
                            });
                        }

                        commands.spawn((
                            Projectile {
                                origin: muzzle_start,
                                target_entity: Some(target_snap.entity),
                                target_pos,
                                speed: if is_siege { 600.0 } else { 680.0 },
                                damage: tank.attack_damage,
                                splash_radius: if is_siege { 45.0 } else { 0.0 },
                                faction: *faction,
                                lifetime: 0.0,
                                max_lifetime: 0.9,
                            },
                            Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.0),
                        ));

                        commands.spawn((
                            MuzzleFlash {
                                lifetime: 0.0,
                                max_lifetime: if is_siege { 0.16 } else { 0.12 },
                                color: if is_siege { Color::srgb(1.0, 0.4, 0.1) } else { Color::srgb(1.0, 0.7, 0.2) },
                            },
                            Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.5),
                        ));
                    }
            } else if !is_hold_pos && tank.mode == TankMode::Tank {
                let stop_dist = (effective_range * 0.90).max(20.0);
                let travel_needed = (dist - stop_dist).max(0.0);
                let step = dir * (move_speed.0 * dt).min(travel_needed);
                tank_tf.translation.x += step.x;
                tank_tf.translation.y += step.y;
            } else if is_siege {
                // Siege mode cannot chase; if target out of range, clear target
                tank.target = None;
            }
        } else {
            // No target: scan for enemies
            tank.target = None;

            let max_scan_range = if is_attack_move {
                (tank.attack_range * 1.25).max(300.0)
            } else {
                tank.attack_range
            };

            let mut best = None;
            let mut best_dist = max_scan_range;
            for t in &targets {
                if t.entity != tank_ent && faction.is_hostile_to(&t.faction) && !t.is_dead {
                    let d = t.pos.distance(tank_pos);
                    let effective_r = max_scan_range + t.radius;
                    if d <= effective_r && d < best_dist {
                        best_dist = d;
                        best = Some((t.entity, t.pos, t.radius));
                    }
                }
            }
            if let Some((best_ent, best_pos, _best_rad)) = best {
                tank.target = Some(best_ent);
                let dir = (best_pos - tank_pos).normalize_or_zero();
                tank.turret_angle = dir.y.atan2(dir.x);

                if !is_online && tank.attack_timer >= tank.attack_cooldown {
                    tank.attack_timer = 0.0;
                    sound_events.send(SoundEffect::SiegeTankShot);

                    let muzzle_dist = if is_siege { radius.0 * 2.2 } else { radius.0 * 1.5 };
                    let muzzle_start = tank_pos + dir * muzzle_dist;

                    particle_events.send(ParticleEvent::MuzzleSmoke {
                        pos: muzzle_start,
                        dir,
                    });

                    if is_siege {
                        particle_events.send(ParticleEvent::Shockwave {
                            pos: muzzle_start,
                            radius: 25.0,
                            color: Color::srgba(1.0, 0.6, 0.2, 0.8),
                        });
                    }

                    commands.spawn((
                        Projectile {
                            origin: muzzle_start,
                            target_entity: Some(best_ent),
                            target_pos: best_pos,
                            speed: if is_siege { 600.0 } else { 680.0 },
                            damage: tank.attack_damage,
                            splash_radius: if is_siege { 45.0 } else { 0.0 },
                            faction: *faction,
                            lifetime: 0.0,
                            max_lifetime: 0.9,
                        },
                        Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.0),
                    ));

                    commands.spawn((
                        MuzzleFlash {
                            lifetime: 0.0,
                            max_lifetime: if is_siege { 0.16 } else { 0.12 },
                            color: if is_siege { Color::srgb(1.0, 0.4, 0.1) } else { Color::srgb(1.0, 0.7, 0.2) },
                        },
                        Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.5),
                    ));
                }
            }
        }
    }
}

