use bevy::prelude::*;
use shared::components::*;
use shared::economy::PlayerEconomy;
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::particles::ParticleEvent;
use crate::stats::MatchStats;

/// Moves flying tracer projectiles and applies direct & splash damage on impact
pub fn projectile_movement_and_impact_system(
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
pub fn muzzle_flash_system(
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
pub fn death_and_elimination_system(
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
pub fn draw_combat_gizmos(
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

