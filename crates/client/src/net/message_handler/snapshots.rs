use std::collections::HashMap;
use bevy::prelude::*;
use shared::components::{Faction, Health, Worker, WorkerState};
use shared::economy::PlayerEconomy;
use shared::protocol::{EntitySnapshot, GameMode};

use crate::audio_sfx::SoundEffect;
use crate::net::NetStatus;
use super::EntityNetQuery;

pub fn handle_tick_snapshot_batch(
    commands: &mut Commands,
    economy: &mut PlayerEconomy,
    net_status: NetStatus,
    current_mode: GameMode,
    my_faction: Faction,
    sound_events: &mut EventWriter<SoundEffect>,
    entity_query: &mut EntityNetQuery,
    snapshots: Vec<EntitySnapshot>,
    p1_minerals: u32,
    p1_supply: u32,
    p1_max_supply: u32,
    p2_minerals: u32,
    p2_supply: u32,
    p2_max_supply: u32,
) {
    // Authoritatively synchronize economy bank & supply from server tick snapshot
    economy.set_minerals(Faction::Player1, p1_minerals);
    economy.set_supply(Faction::Player1, p1_supply, p1_max_supply);

    let p2_faction = if current_mode == GameMode::SoloVsAi {
        Faction::HostileAi
    } else {
        Faction::Player2
    };
    economy.set_minerals(p2_faction, p2_minerals);
    economy.set_supply(p2_faction, p2_supply, p2_max_supply);

    // Index existing entities by Net ID for Health, Mining, and deadband position reconciliation
    let mut entity_map: HashMap<
        u32,
        (Entity, Mut<Transform>, Mut<Health>, Option<Mut<Worker>>, bool, Faction),
    > = HashMap::new();

    for (entity, net_entity, faction, transform, health, worker_opt, _, _, move_target_opt, ..) in
        entity_query.iter_mut()
    {
        let has_move = move_target_opt.is_some();
        entity_map.insert(net_entity.net_id, (entity, transform, health, worker_opt, has_move, *faction));
    }

    // Sync health, worker mining state, and deadband positions from server snapshot
    if net_status == NetStatus::InGame {
        for snap in snapshots {
            if let Some((entity, mut tf, mut hp, mut worker_opt, has_move, faction)) = entity_map.remove(&snap.net_id) {
                if snap.current_hp <= 0.0 {
                    commands.entity(entity).despawn_recursive();
                    continue;
                }

                hp.current = snap.current_hp;
                hp.max = snap.max_hp;

                if let Some(ref mut worker) = worker_opt {
                    if let Some(ws) = snap.worker_state {
                        worker.state = ws;
                    } else if snap.is_mining {
                        worker.state = WorkerState::Mining;
                    }
                    if faction == my_faction && worker.carried_minerals == 0 && snap.carried_minerals > 0 {
                        sound_events.send(SoundEffect::LaserMining);
                    }
                    worker.carried_minerals = snap.carried_minerals;
                }

                let cur_pos = tf.translation.truncate();
                let dist = cur_pos.distance(snap.position);

                if dist > 2.0 {
                    let target_3d = Vec3::new(snap.position.x, snap.position.y, tf.translation.z);
                    if dist > 25.0 {
                        tf.translation = target_3d;
                    } else if !has_move {
                        tf.translation = tf.translation.lerp(target_3d, 0.40);
                    } else {
                        tf.translation = tf.translation.lerp(target_3d, 0.20);
                    }
                }

                // Synchronize facing rotation from authoritative server snapshot when not actively steering
                if !has_move {
                    tf.rotation = Quat::from_rotation_z(snap.rotation);
                }
            }
        }
    }
}

pub fn handle_entity_damaged(
    entity_query: &mut EntityNetQuery,
    target_net_id: u32,
    current_hp: f32,
    max_hp: f32,
) {
    for (_, net_entity, _, _, mut hp, ..) in entity_query.iter_mut() {
        if net_entity.net_id == target_net_id {
            hp.current = current_hp;
            hp.max = max_hp;
        }
    }
}

pub fn handle_entity_died(
    commands: &mut Commands,
    sound_events: &mut EventWriter<SoundEffect>,
    entity_query: &mut EntityNetQuery,
    net_id: u32,
) {
    for (entity, net_entity, ..) in entity_query.iter_mut() {
        if net_entity.net_id == net_id {
            commands.entity(entity).despawn_recursive();
            sound_events.send(SoundEffect::Explosion);
            break;
        }
    }
}
