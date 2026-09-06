use bevy::prelude::*;
use shared::components::*;
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::particles::ParticleEvent;

/// Defensive Gun Turret Combat System
pub fn turret_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    outcome_opt: Option<Res<MatchOutcome>>,
    net_client_opt: Option<Res<NetClient>>,
    mut sound_events: EventWriter<SoundEffect>,
    mut particle_events: EventWriter<ParticleEvent>,
    mut turret_query: Query<(Entity, &mut GunTurret, &Transform, &Faction, &Building)>,
    target_query: Query<(Entity, &Transform, &Radius, &Faction, &Health)>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory) || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat) {
        return;
    }

    let dt = time.delta_secs();
    let is_online = net_client_opt.as_ref().map(|n| n.status == NetStatus::InGame).unwrap_or(false);

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

            if !is_online
                && turret.attack_timer >= turret.attack_cooldown {
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

