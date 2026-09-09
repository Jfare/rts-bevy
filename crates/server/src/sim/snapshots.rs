use bevy::prelude::*;
use shared::components::*;
use shared::protocol::{EntitySnapshot, GameMode, ServerMessage};

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;
use super::ServerTickTimer;

/// 30 Hz position, rotation, and health snapshot broadcast partitioned per active room
pub fn server_tick_snapshot_system(
    time: Res<Time>,
    mut tick_timer: ResMut<ServerTickTimer>,
    net_channels: Res<ServerNetworkChannels>,
    matchmaker: Res<Matchmaker>,
    entities_query: Query<(
        &NetEntity,
        &Transform,
        &Health,
        &RoomId,
        Option<&Worker>,
    )>,
    node_query: Query<(&NetEntity, &Transform), With<ResourceNode>>,
    mut tick_counter: Local<u32>,
) {
    tick_timer.0.tick(time.delta());
    if tick_timer.0.just_finished() {
        *tick_counter += 1;

        for (room_id, room) in &matchmaker.rooms {
            if !room.is_active {
                continue;
            }

            let peers = matchmaker.get_room_peers(*room_id);
            if peers.is_empty() {
                continue;
            }

            let mut room_snapshots = Vec::new();

            for (net_entity, transform, health, ent_room, worker_opt) in &entities_query {
                if ent_room.0 != *room_id {
                    continue;
                }

                let is_mining = worker_opt
                    .map(|w| w.state == WorkerState::Mining)
                    .unwrap_or(false);

                let laser_target = if is_mining {
                    worker_opt.and_then(|w| {
                        w.target_node.and_then(|node_e| {
                            node_query.get(node_e).ok().map(|(_, tf)| tf.translation.truncate())
                        })
                    })
                } else {
                    None
                };

                let rotation = transform.rotation.to_euler(EulerRot::XYZ).2;

                room_snapshots.push(EntitySnapshot {
                    net_id: net_entity.net_id,
                    position: transform.translation.truncate(),
                    rotation,
                    current_hp: health.current,
                    max_hp: health.max,
                    is_mining,
                    laser_target,
                });
            }

            let (p1_cur_sup, p1_max_sup) = room.economy.get_supply(Faction::Player1);
            let p2_faction = if room.mode == GameMode::SoloVsAi {
                Faction::HostileAi
            } else {
                Faction::Player2
            };
            let (p2_cur_sup, p2_max_sup) = room.economy.get_supply(p2_faction);

            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                peer_ids: peers,
                msg: ServerMessage::TickSnapshotBatch {
                    tick: *tick_counter,
                    snapshots: room_snapshots,
                    p1_minerals: room.economy.get_minerals(Faction::Player1),
                    p1_supply: p1_cur_sup,
                    p1_max_supply: p1_max_sup,
                    p2_minerals: room.economy.get_minerals(p2_faction),
                    p2_supply: p2_cur_sup,
                    p2_max_supply: p2_max_sup,
                    next_wave_seconds: room.time_until_next_wave,
                    current_wave: room.current_wave,
                },
            });
        }
    }
}

