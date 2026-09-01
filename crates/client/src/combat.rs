use bevy::prelude::*;
use shared::components::*;
use shared::economy::PlayerEconomy;
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::particles::ParticleEvent;
use crate::stats::MatchStats;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchOutcome>()
            .add_systems(
                Update,
                (
                    soldier_combat_system,
                    turret_combat_system,
                    siege_tank_combat_system,
                    projectile_movement_and_impact_system,
                    muzzle_flash_system,
                    death_and_elimination_system,
                    draw_combat_gizmos,
                ),
            );
    }
}

/// Target snapshot used for disjoint scanning and combat logic
struct TargetSnapshot {
    entity: Entity,
    pos: Vec2,
    radius: f32,
    faction: Faction,
    is_dead: bool,
}

/// Marine Combat State Machine with Stimpack and Hold Position support
fn soldier_combat_system(
    mut commands: Commands,
    time: Res<Time>,
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
            Option<&Stimpack>,
            Option<&TacticalStance>,
        )>,
    )>,
) {
    if net_client_opt.as_ref().map(|n| n.status == NetStatus::InGame).unwrap_or(false) {
        return;
    }
    let dt = time.delta_secs();

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

    // 2. Update all soldiers using the snapshot
    for (soldier_entity, mut soldier, mut soldier_transform, move_speed, faction, radius, move_target_opt, stim_opt, stance_opt) in
        &mut queries.p1()
    {
        soldier.attack_timer += dt;
        soldier.scan_timer += dt;
        let soldier_pos = soldier_transform.translation.truncate();

        let is_hold_pos = stance_opt.map(|s| *s == TacticalStance::HoldPosition).unwrap_or(false)
            || soldier.state == SoldierState::HoldingPosition;

        let current_target_snapshot = soldier.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead)
        });

        if current_target_snapshot.is_none() {
            soldier.target = None;
            if soldier.state == SoldierState::Attacking || soldier.state == SoldierState::ChasingTarget {
                soldier.state = if is_hold_pos {
                    SoldierState::HoldingPosition
                } else if move_target_opt.as_ref().map(|m| m.is_attack_move).unwrap_or(false) {
                    SoldierState::AttackMoving
                } else {
                    SoldierState::Idle
                };
            }

            let mut nearest_enemy = None;
            let mut nearest_dist = soldier.aggro_radius;

            for t in &targets {
                if t.entity != soldier_entity && faction.is_hostile_to(&t.faction) && !t.is_dead {
                    let dist = soldier_pos.distance(t.pos);
                    if dist < nearest_dist {
                        nearest_dist = dist;
                        nearest_enemy = Some(t.entity);
                    }
                }
            }

            if let Some(enemy_ent) = nearest_enemy {
                soldier.target = Some(enemy_ent);
                soldier.state = SoldierState::ChasingTarget;
                if move_target_opt.is_some() {
                    commands.entity(soldier_entity).remove::<MoveTarget>();
                }
            }
        }

        if let Some(target_ent) = soldier.target {
            if let Some(target_snap) = targets.iter().find(|t| t.entity == target_ent && !t.is_dead) {
                let target_pos = target_snap.pos;
                let dist = soldier_pos.distance(target_pos);
                let effective_range = soldier.attack_range + target_snap.radius;

                let dir = (target_pos - soldier_pos).normalize_or_zero();
                if dir.length_squared() > 0.0 {
                    let angle = dir.y.atan2(dir.x);
                    soldier_transform.rotation = Quat::from_rotation_z(angle);
                }

                let cooldown_mult = stim_opt
                    .map(|s| if s.is_active { 0.5 } else { 1.0 })
                    .unwrap_or(1.0);

                if dist <= effective_range {
                    soldier.state = SoldierState::Attacking;
                    if move_target_opt.is_some() {
                        commands.entity(soldier_entity).remove::<MoveTarget>();
                    }

                    if soldier.attack_timer >= (soldier.attack_cooldown * cooldown_mult) {
                        soldier.attack_timer = 0.0;
                        sound_events.send(SoundEffect::Gunshot);

                        let muzzle_offset = dir * (radius.0 + 8.0);
                        let projectile_start = soldier_pos + muzzle_offset;

                        particle_events.send(ParticleEvent::MuzzleSmoke {
                            pos: projectile_start,
                            dir,
                        });

                        commands.spawn((
                            Projectile {
                                origin: projectile_start,
                                target_entity: Some(target_ent),
                                target_pos,
                                speed: 780.0,
                                damage: soldier.attack_damage,
                                splash_radius: 0.0,
                                faction: *faction,
                                lifetime: 0.0,
                                max_lifetime: 0.7,
                            },
                            Transform::from_xyz(projectile_start.x, projectile_start.y, 3.0),
                        ));

                        commands.spawn((
                            MuzzleFlash {
                                lifetime: 0.0,
                                max_lifetime: 0.08,
                                color: Color::srgb(1.0, 0.9, 0.3),
                            },
                            Transform::from_xyz(projectile_start.x, projectile_start.y, 3.5),
                        ));
                    }
                } else if !is_hold_pos {
                    soldier.state = SoldierState::ChasingTarget;
                    let speed_mult = stim_opt
                        .map(|s| if s.is_active { 1.5 } else { 1.0 })
                        .unwrap_or(1.0);
                    let stop_dist = (effective_range * 0.90).max(10.0);
                    let travel_needed = (dist - stop_dist).max(0.0);
                    let step = dir * (move_speed.0 * speed_mult * dt).min(travel_needed);
                    soldier_transform.translation.x += step.x;
                    soldier_transform.translation.y += step.y;
                }
            }
        }
    }
}

/// Defensive Gun Turret Combat System
fn turret_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    net_client_opt: Option<Res<NetClient>>,
    mut sound_events: EventWriter<SoundEffect>,
    mut particle_events: EventWriter<ParticleEvent>,
    mut turret_query: Query<(Entity, &mut GunTurret, &Transform, &Faction, &Building)>,
    target_query: Query<(Entity, &Transform, &Radius, &Faction, &Health)>,
) {
    if net_client_opt.as_ref().map(|n| n.status == NetStatus::InGame).unwrap_or(false) {
        return;
    }
    let dt = time.delta_secs();

    for (turret_ent, mut turret, tf, faction, building) in &mut turret_query {
        if !building.is_constructed {
            continue;
        }
        turret.attack_timer += dt;
        let turret_pos = tf.translation.truncate();

        let target_valid = turret.target.and_then(|t_ent| {
            if let Ok((ent, target_tf, radius, t_fac, hp)) = target_query.get(t_ent) {
                if !hp.is_dead() && faction.is_hostile_to(t_fac) && target_tf.translation.truncate().distance(turret_pos) <= (turret.attack_range + radius.0) {
                    return Some((ent, target_tf.translation.truncate()));
                }
            }
            None
        });

        let active_target = match target_valid {
            Some(t) => Some(t),
            None => {
                turret.target = None;
                let mut best = None;
                let mut best_dist = turret.attack_range;
                for (ent, t_tf, radius, t_fac, hp) in &target_query {
                    if ent != turret_ent && faction.is_hostile_to(t_fac) && !hp.is_dead() {
                        let d = t_tf.translation.truncate().distance(turret_pos);
                        if d <= (turret.attack_range + radius.0) && d < best_dist {
                            best_dist = d;
                            best = Some((ent, t_tf.translation.truncate()));
                        }
                    }
                }
                if let Some((best_ent, _)) = best {
                    turret.target = Some(best_ent);
                }
                best
            }
        };

        if let Some((target_ent, target_pos)) = active_target {
            let dir = (target_pos - turret_pos).normalize_or_zero();
            turret.barrel_angle = dir.y.atan2(dir.x);

            if turret.attack_timer >= turret.attack_cooldown {
                turret.attack_timer = 0.0;
                sound_events.send(SoundEffect::Gunshot);

                let muzzle_start = turret_pos + dir * 28.0;
                particle_events.send(ParticleEvent::MuzzleSmoke {
                    pos: muzzle_start,
                    dir,
                });

                commands.spawn((
                    Projectile {
                        origin: muzzle_start,
                        target_entity: Some(target_ent),
                        target_pos,
                        speed: 850.0,
                        damage: turret.attack_damage,
                        splash_radius: 0.0,
                        faction: *faction,
                        lifetime: 0.0,
                        max_lifetime: 0.6,
                    },
                    Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.0),
                ));

                commands.spawn((
                    MuzzleFlash {
                        lifetime: 0.0,
                        max_lifetime: 0.09,
                        color: Color::srgb(1.0, 0.85, 0.2),
                    },
                    Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.5),
                ));
            }
        }
    }
}

/// Siege Tank Combat & Artillery System
fn siege_tank_combat_system(
    mut commands: Commands,
    time: Res<Time>,
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
    if net_client_opt.as_ref().map(|n| n.status == NetStatus::InGame).unwrap_or(false) {
        return;
    }
    let dt = time.delta_secs();

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

        let target_valid = tank.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead && faction.is_hostile_to(&t.faction) && t.pos.distance(tank_pos) <= (tank.attack_range + t.radius))
                .map(|t| (t.entity, t.pos, t.radius))
        });

        let active_target = match target_valid {
            Some(t) => Some(t),
            None => {
                tank.target = None;
                let mut best = None;
                let mut best_dist = tank.attack_range;
                for t in &targets {
                    if t.entity != tank_ent && faction.is_hostile_to(&t.faction) && !t.is_dead {
                        let d = t.pos.distance(tank_pos);
                        if d <= (tank.attack_range + t.radius) && d < best_dist {
                            best_dist = d;
                            best = Some((t.entity, t.pos, t.radius));
                        }
                    }
                }
                if let Some((best_ent, _, _)) = best {
                    tank.target = Some(best_ent);
                }
                best
            }
        };

        if let Some((target_ent, target_pos, target_rad)) = active_target {
            let dir = (target_pos - tank_pos).normalize_or_zero();
            tank.turret_angle = dir.y.atan2(dir.x);

            let dist = tank_pos.distance(target_pos);
            let effective_range = tank.attack_range + target_rad;

            if dist <= effective_range {
                if move_target_opt.is_some() {
                    commands.entity(tank_ent).remove::<MoveTarget>();
                }

                if tank.attack_timer >= tank.attack_cooldown {
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
                            target_entity: Some(target_ent),
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
            }
        }
    }
}

/// Moves flying tracer projectiles and applies direct & splash damage on impact
fn projectile_movement_and_impact_system(
    mut commands: Commands,
    time: Res<Time>,
    net_client_opt: Option<Res<NetClient>>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
    mut particle_events: EventWriter<ParticleEvent>,
    mut projectile_query: Query<(Entity, &mut Projectile, &mut Transform)>,
    mut health_query: Query<(Entity, &Transform, &Faction, &mut Health), Without<Projectile>>,
) {
    let dt = time.delta_secs();
    let is_online = net_client_opt.as_ref().map(|n| n.status == NetStatus::InGame).unwrap_or(false);

    for (proj_entity, mut projectile, mut transform) in &mut projectile_query {
        projectile.lifetime += dt;
        if projectile.lifetime >= projectile.max_lifetime {
            commands.entity(proj_entity).despawn();
            continue;
        }

        let current_pos = transform.translation.truncate();
        let target_pos = projectile.target_pos;
        let dist = current_pos.distance(target_pos);
        let step_dist = projectile.speed * dt;

        if dist <= step_dist || dist <= 14.0 {
            let impact_pos = target_pos;

            // Apply direct damage only in offline local play (in online play, server authoritatively applies damage)
            if !is_online {
                if let Some(target_ent) = projectile.target_entity {
                    if let Ok((_, _, _, mut health)) = health_query.get_mut(target_ent) {
                        health.take_damage(projectile.damage);
                        if projectile.faction == Faction::Player1 {
                            stats.damage_dealt += projectile.damage;
                        }
                    }
                }
            }

            // Splash damage
            if projectile.splash_radius > 0.0 {
                let splash_r = projectile.splash_radius;
                let proj_faction = projectile.faction;
                let splash_dmg = projectile.damage * 0.65;

                particle_events.send(ParticleEvent::Explosion {
                    pos: impact_pos,
                    is_heavy: true,
                });
                sound_events.send(SoundEffect::Explosion);

                if !is_online {
                    for (ent, tf, faction, mut hp) in &mut health_query {
                        if Some(ent) != projectile.target_entity && proj_faction.is_hostile_to(faction) {
                            let d = tf.translation.truncate().distance(impact_pos);
                            if d <= splash_r {
                                let falloff = 1.0 - (d / splash_r) * 0.5;
                                let dmg = splash_dmg * falloff;
                                hp.take_damage(dmg);
                                if projectile.faction == Faction::Player1 {
                                    stats.damage_dealt += dmg;
                                }
                            }
                        }
                    }
                }
            } else {
                let dir = (target_pos - projectile.origin).normalize_or_zero();
                particle_events.send(ParticleEvent::Sparks {
                    pos: impact_pos,
                    dir,
                    count: 6,
                });
            }

            commands.entity(proj_entity).despawn();
        } else {
            let dir = (target_pos - current_pos).normalize_or_zero();
            transform.translation.x += dir.x * step_dist;
            transform.translation.y += dir.y * step_dist;
        }
    }
}

/// Updates timers for muzzle flash effects and cleans them up
fn muzzle_flash_system(
    mut commands: Commands,
    time: Res<Time>,
    mut flash_query: Query<(Entity, &mut MuzzleFlash)>,
) {
    let dt = time.delta_secs();
    for (entity, mut flash) in &mut flash_query {
        flash.lifetime += dt;
        if flash.lifetime >= flash.max_lifetime {
            commands.entity(entity).despawn();
        }
    }
}

/// Eliminates entities when health hits 0 and evaluates Victory/Defeat
fn death_and_elimination_system(
    mut commands: Commands,
    net_client_opt: Option<Res<NetClient>>,
    mut outcome: ResMut<MatchOutcome>,
    mut economy: ResMut<PlayerEconomy>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
    mut particle_events: EventWriter<ParticleEvent>,
    query: Query<(
        Entity,
        &Transform,
        &Health,
        &Faction,
        Option<&Unit>,
        Option<&BaseHQ>,
    )>,
) {
    if net_client_opt.as_ref().map(|n| n.status == NetStatus::InGame).unwrap_or(false) {
        return;
    }
    for (entity, transform, health, faction, unit_opt, base_hq_opt) in &query {
        if health.is_dead() {
            let pos = transform.translation.truncate();
            let is_hq = base_hq_opt.is_some();

            sound_events.send(SoundEffect::Explosion);
            particle_events.send(ParticleEvent::Explosion {
                pos,
                is_heavy: is_hq,
            });

            if let Some(unit) = unit_opt {
                economy.unregister_supply(*faction, unit.supply_cost);
                info!("💀 [{:?}] {} destroyed!", faction, unit.name);
            }

            if *faction == Faction::Player1 {
                stats.units_lost += 1;
            } else if *faction == Faction::HostileAi {
                if unit_opt.is_some() {
                    stats.enemy_units_killed += 1;
                }
                if is_hq {
                    stats.enemy_buildings_destroyed += 1;
                }
            }

            if is_hq {
                if *faction == Faction::HostileAi {
                    *outcome = MatchOutcome::Victory;
                    sound_events.send(SoundEffect::Victory);
                    info!("🏆 [MATCH RESULT] VICTORY! Hostile Base HQ destroyed!");
                } else if *faction == Faction::Player1 {
                    *outcome = MatchOutcome::Defeat;
                    sound_events.send(SoundEffect::Defeat);
                    info!("💥 [MATCH RESULT] DEFEAT! Player Base HQ destroyed!");
                }
            }

            commands.entity(entity).despawn_recursive();
        }
    }
}

/// Renders combat visual effects (tracers, muzzle flashes)
fn draw_combat_gizmos(
    mut gizmos: Gizmos,
    projectiles: Query<(&Transform, &Projectile)>,
    flashes: Query<(&Transform, &MuzzleFlash)>,
) {
    // 1. Draw Projectile Tracers
    for (transform, proj) in &projectiles {
        let current_pos = transform.translation.truncate();
        let dir = (proj.target_pos - proj.origin).normalize_or_zero();
        let tracer_len = 16.0;
        let start_tail = current_pos - dir * tracer_len;

        let [r, g, b, _] = proj.faction.color_rgba();
        let color = if proj.splash_radius > 0.0 {
            Color::srgb(1.0, 0.45, 0.15) // Heavy siege artillery
        } else {
            Color::srgb(r, g, b).lighter(0.3)
        };

        gizmos.line_2d(start_tail, current_pos, color);
        gizmos.circle_2d(current_pos, if proj.splash_radius > 0.0 { 4.5 } else { 2.5 }, Color::WHITE);
    }

    // 2. Draw Muzzle Flashes
    for (transform, flash) in &flashes {
        let pos = transform.translation.truncate();
        gizmos.circle_2d(pos, 5.0, flash.color);
        gizmos.circle_2d(pos, 2.5, Color::WHITE);
    }
}
