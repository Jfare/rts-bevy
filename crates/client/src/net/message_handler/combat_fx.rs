use bevy::prelude::*;
use shared::components::{Faction, MuzzleFlash, Projectile};

use crate::audio_sfx::SoundEffect;
use crate::particles::ParticleEvent;
use super::EntityNetQuery;

pub fn handle_projectile_fired(
    commands: &mut Commands,
    sound_events: &mut EventWriter<SoundEffect>,
    particle_events: &mut EventWriter<ParticleEvent>,
    entity_query: &mut EntityNetQuery,
    attacker_net_id: u32,
    target_net_id: u32,
    origin: Vec2,
    target_pos: Vec2,
    damage: f32,
) {
    // Find target's client position if present
    let target_client_pos = entity_query
        .iter()
        .find(|(_, net, ..)| net.net_id == target_net_id)
        .map(|(_, _, _, tf, ..)| tf.translation.truncate())
        .unwrap_or(target_pos);

    // Find attacker on client to ensure muzzle origin and barrel orientation are visually accurate
    let mut attacker_found = false;
    for (_e, net_entity, fac, mut tf, _hp, _worker, soldier_opt, mut melee_opt, _, _, rad_opt, mut turret_opt, ..) in
        entity_query.iter_mut()
    {
        if net_entity.net_id == attacker_net_id {
            attacker_found = true;
            let attacker_pos = tf.translation.truncate();
            let diff = target_client_pos - attacker_pos;
            let dir = diff.normalize_or_zero();
            let angle = dir.y.atan2(dir.x);
            let attacker_fac = *fac;
            let rad = rad_opt.map(|r| r.0).unwrap_or(16.0);

            // If attacker is a Melee Fighter: trigger dynamic sword swing animation, sparks, and sword slash audio!
            if let Some(ref mut melee) = melee_opt {
                if dir.length_squared() > 0.001 {
                    tf.rotation = Quat::from_rotation_z(angle);
                }
                melee.swing_timer = 0.18;
                sound_events.send(SoundEffect::SwordSlash);
                particle_events.send(ParticleEvent::Sparks {
                    pos: target_client_pos,
                    dir,
                    count: 6,
                });
                break;
            }

            let is_turret = turret_opt.is_some();

            // Orient attacker / turret towards target
            if dir.length_squared() > 0.001 {
                if soldier_opt.is_some() {
                    tf.rotation = Quat::from_rotation_z(angle);
                }
                if let Some(ref mut turret) = turret_opt {
                    turret.barrel_angle = angle;
                }
            }

            // Calculate muzzle start point aligned with visual barrel
            let muzzle_start = if is_turret {
                attacker_pos + dir * 28.0
            } else {
                attacker_pos + dir * (rad + 8.0)
            };

            let to_target = target_client_pos - muzzle_start;
            let dist = to_target.length();
            let speed = if is_turret { 850.0 } else { 780.0 };
            let lifetime = if dist > 0.0 { dist / speed } else { 0.1 };

            commands.spawn((
                Projectile {
                    origin: muzzle_start,
                    target_entity: None,
                    target_pos: target_client_pos,
                    speed,
                    damage,
                    splash_radius: 0.0,
                    faction: attacker_fac,
                    lifetime: 0.0,
                    max_lifetime: lifetime,
                },
                Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.0),
            ));

            commands.spawn((
                MuzzleFlash {
                    lifetime: 0.0,
                    max_lifetime: 0.08,
                    color: Color::srgb(1.0, 0.9, 0.3),
                },
                Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.5),
            ));

            particle_events.send(ParticleEvent::MuzzleSmoke {
                pos: muzzle_start,
                dir,
            });

            sound_events.send(SoundEffect::Gunshot);
            break;
        }
    }

    if !attacker_found {
        let diff = target_pos - origin;
        let dist = diff.length();
        let speed = 780.0;
        let lifetime = if dist > 0.0 { dist / speed } else { 0.1 };

        commands.spawn((
            Projectile {
                origin,
                target_entity: None,
                target_pos,
                speed,
                damage,
                splash_radius: 0.0,
                faction: Faction::Neutral,
                lifetime: 0.0,
                max_lifetime: lifetime,
            },
            Transform::from_xyz(origin.x, origin.y, 3.0),
        ));

        commands.spawn((
            MuzzleFlash {
                lifetime: 0.0,
                max_lifetime: 0.07,
                color: Color::srgb(1.0, 0.85, 0.35),
            },
            Transform::from_xyz(origin.x, origin.y, 3.1),
        ));

        sound_events.send(SoundEffect::Gunshot);
    }
}
