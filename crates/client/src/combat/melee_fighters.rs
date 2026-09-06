use bevy::prelude::*;
use shared::components::*;
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::particles::ParticleEvent;
use crate::stats::MatchStats;
use super::TargetSnapshot;

/// Melee Fighter Combat State Machine with dynamic sword strikes and sparks
pub fn melee_fighter_combat_system(
    time: Res<Time>,
    outcome_opt: Option<Res<MatchOutcome>>,
    net_client_opt: Option<Res<NetClient>>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
    mut particle_events: EventWriter<ParticleEvent>,
    mut queries: ParamSet<(
        Query<(Entity, &Transform, &Radius, &Faction, &Health)>,
        Query<(
            Entity,
            &mut MeleeFighter,
            &mut Transform,
            &MoveSpeed,
            &Faction,
            &Radius,
            Option<&mut MoveTarget>,
            Option<&TacticalStance>,
        )>,
        Query<(Entity, &mut Health)>,
    )>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory) || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat) {
        return;
    }

    let dt = time.delta_secs();
    let is_online = net_client_opt.as_ref().map(|n| n.status == NetStatus::InGame).unwrap_or(false);

    // 1. Snapshot potential targets
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

    let mut damages_to_apply: Vec<(Entity, f32, Faction)> = Vec::new();

    // 2. Update all melee fighters
    for (melee_ent, mut melee, mut transform, move_speed, faction, radius, mut move_target_opt, stance_opt) in
        &mut queries.p1()
    {
        // Advance timers
        melee.attack_timer += dt;
        melee.scan_timer += dt;
        if melee.swing_timer > 0.0 {
            melee.swing_timer = (melee.swing_timer - dt).max(0.0);
        }

        let melee_pos = transform.translation.truncate();
        let is_hold_pos = stance_opt.map(|s| *s == TacticalStance::HoldPosition).unwrap_or(false)
            || melee.state == SoldierState::HoldingPosition;

        // If unit has a pure ground move order, ignore combat and advance
        let is_attack_move = move_target_opt
            .as_ref()
            .map(|m| m.is_attack_move)
            .unwrap_or(false);
        if move_target_opt.is_some() && !is_attack_move {
            melee.target = None;
            melee.state = SoldierState::MovingToGround;
            continue;
        }

        // Validate active target
        let target_snapshot = melee.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead && faction.is_hostile_to(&t.faction))
        });

        if let Some(target_snap) = target_snapshot {
            let target_pos = target_snap.pos;
            let dir = (target_pos - melee_pos).normalize_or_zero();
            let dist = melee_pos.distance(target_pos);
            let effective_range = melee.attack_range + radius.0 + target_snap.radius;

            if dir.length_squared() > 0.001 {
                let angle = dir.y.atan2(dir.x);
                transform.rotation = Quat::from_rotation_z(angle);
            }

            if dist <= effective_range {
                // In melee striking range
                melee.state = SoldierState::Attacking;

                if !is_online && melee.attack_timer >= melee.attack_cooldown {
                    melee.attack_timer = 0.0;
                    melee.swing_timer = 0.18; // Trigger sword slash visual swing

                    damages_to_apply.push((target_snap.entity, melee.attack_damage, *faction));
                    sound_events.send(SoundEffect::SwordSlash);

                    let impact_pos = melee_pos + dir * (radius.0 + 8.0);
                    particle_events.send(ParticleEvent::Sparks {
                        pos: impact_pos,
                        dir,
                        count: 8,
                    });
                }
            } else if !is_hold_pos {
                // Out of range: chase into melee contact
                melee.state = SoldierState::ChasingTarget;
                let stop_dist = (effective_range * 0.85).max(10.0);
                let travel_needed = (dist - stop_dist).max(0.0);
                let step = dir * (move_speed.0 * dt).min(travel_needed);
                transform.translation.x += step.x;
                transform.translation.y += step.y;
            } else {
                melee.target = None;
                melee.state = SoldierState::HoldingPosition;
            }
        } else {
            // No target: scan for nearest hostile enemy
            melee.target = None;

            if melee.scan_timer >= 0.15 {
                melee.scan_timer = 0.0;

                let max_scan_range = if is_attack_move {
                    (melee.aggro_radius * 1.35).max(300.0)
                } else {
                    melee.aggro_radius
                };

                let mut best_target = None;
                let mut best_dist = max_scan_range;

                for t in &targets {
                    if t.entity != melee_ent && faction.is_hostile_to(&t.faction) && !t.is_dead {
                        let d = t.pos.distance(melee_pos);
                        let eff_r = max_scan_range + t.radius;
                        if d <= eff_r && d < best_dist {
                            best_dist = d;
                            best_target = Some(t.entity);
                        }
                    }
                }

                if let Some(best_ent) = best_target {
                    melee.target = Some(best_ent);
                    if let Some(ref mut mt) = move_target_opt {
                        if mt.is_attack_move {
                            mt.waypoints.clear();
                        }
                    }
                }
            }

            if is_attack_move {
                melee.state = SoldierState::AttackMoving;
            } else if !is_hold_pos {
                melee.state = SoldierState::Idle;
            }
        }
    }

    // 3. Apply damage in offline local play
    if !is_online {
        let mut health_query = queries.p2();
        for (target_ent, dmg, attacker_faction) in damages_to_apply {
            if let Ok((_, mut hp)) = health_query.get_mut(target_ent) {
                hp.take_damage(dmg);
                if attacker_faction == Faction::Player1 {
                    stats.damage_dealt += dmg;
                }
            }
        }
    }
}

