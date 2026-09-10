use bevy::prelude::*;
use shared::components::*;
use shared::grid::NavGrid;
use shared::protocol::ServerMessage;

use crate::net_server::{IncomingNetEvent, OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;
use super::commands::{combat, economy, movement, social, NodeQuery, ProdQuery, UnitQuery};
use super::lobby::handle_join_lobby;

/// Reads and executes client network commands
pub fn handle_incoming_network_events(
    mut commands: Commands,
    net_channels: Res<ServerNetworkChannels>,
    mut matchmaker: ResMut<Matchmaker>,
    nav_grid: Res<NavGrid>,
    room_entities: Query<(Entity, &RoomId)>,
    mut unit_query: UnitQuery,
    node_query: NodeQuery,
    mut prod_query: ProdQuery,
) {
    while let Ok(event) = net_channels.rx_incoming.try_recv() {
        match event {
            IncomingNetEvent::PeerConnected { peer_id, addr } => {
                info!("🎮 [GameServer] Peer #{} connected from {}", peer_id, addr);
                matchmaker.connected_peers.insert(peer_id);
                broadcast_lobby_stats(&net_channels, &matchmaker);
            }
            IncomingNetEvent::PeerDisconnected { peer_id } => {
                info!("🎮 [GameServer] Peer #{} disconnected", peer_id);
                matchmaker.connected_peers.remove(&peer_id);
                if matchmaker.waiting_1v1_peer == Some(peer_id) {
                    matchmaker.waiting_1v1_peer = None;
                }
                handle_leave_active_match(
                    &mut commands,
                    &net_channels,
                    &mut matchmaker,
                    &room_entities,
                    peer_id,
                );
                broadcast_lobby_stats(&net_channels, &matchmaker);
            }
            IncomingNetEvent::MessageReceived { peer_id, msg } => {
                match msg {
                    shared::protocol::ClientMessage::JoinLobby {
                        player_name,
                        mode,
                        room_code,
                        faction_color,
                    } => {
                        handle_join_lobby(
                            &mut commands,
                            &net_channels,
                            &mut matchmaker,
                            peer_id,
                            player_name,
                            mode,
                            room_code,
                            faction_color,
                        );
                    }
                    shared::protocol::ClientMessage::CancelQueue => {
                        if matchmaker.cancel_queue(peer_id) {
                            info!("🚪 [GameServer] Peer #{} cancelled 1v1 queue", peer_id);
                            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                                peer_id,
                                msg: ServerMessage::QueueCancelled,
                            });
                            broadcast_lobby_stats(&net_channels, &matchmaker);
                        }
                    }
                    shared::protocol::ClientMessage::ForfeitMatch => {
                        info!("🏳️ [GameServer] Peer #{} forfeited active match", peer_id);
                        handle_leave_active_match(
                            &mut commands,
                            &net_channels,
                            &mut matchmaker,
                            &room_entities,
                            peer_id,
                        );
                        broadcast_lobby_stats(&net_channels, &matchmaker);
                    }
                    shared::protocol::ClientMessage::RequestLobbyStats => {
                        let (q, a1, m1, aso, mso, tot) = matchmaker.get_telemetry();
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                            peer_id,
                            msg: ServerMessage::LobbyStats {
                                queue_1v1: q,
                                active_1v1_matches: a1,
                                max_1v1_matches: m1,
                                active_solo_matches: aso,
                                max_solo_matches: mso,
                                total_online: tot,
                            },
                        });
                    }
                    shared::protocol::ClientMessage::RequestMove {
                        unit_net_ids,
                        target_position,
                        is_attack_move,
                    } => {
                        movement::handle_move(
                            &mut commands,
                            &nav_grid,
                            &net_channels,
                            &matchmaker,
                            &mut unit_query,
                            peer_id,
                            &unit_net_ids,
                            target_position,
                            is_attack_move,
                        );
                    }
                    shared::protocol::ClientMessage::RequestPatrol {
                        unit_net_ids,
                        target_position,
                    } => {
                        movement::handle_patrol(
                            &mut commands,
                            &nav_grid,
                            &net_channels,
                            &matchmaker,
                            &mut unit_query,
                            peer_id,
                            &unit_net_ids,
                            target_position,
                        );
                    }
                    shared::protocol::ClientMessage::RequestAttackTarget {
                        unit_net_ids,
                        target_net_id,
                    } => {
                        combat::handle_attack_target(
                            &mut commands,
                            &net_channels,
                            &matchmaker,
                            &mut unit_query,
                            peer_id,
                            &unit_net_ids,
                            target_net_id,
                        );
                    }
                    shared::protocol::ClientMessage::RequestHarvest {
                        worker_net_ids,
                        resource_net_id,
                    } => {
                        economy::handle_harvest(
                            &mut commands,
                            &net_channels,
                            &matchmaker,
                            &mut unit_query,
                            &node_query,
                            peer_id,
                            &worker_net_ids,
                            resource_net_id,
                        );
                    }
                    shared::protocol::ClientMessage::RequestStop { unit_net_ids } => {
                        movement::handle_stop(
                            &mut commands,
                            &net_channels,
                            &matchmaker,
                            &mut unit_query,
                            peer_id,
                            &unit_net_ids,
                        );
                    }
                    shared::protocol::ClientMessage::RequestHoldPosition { unit_net_ids } => {
                        movement::handle_hold_position(
                            &mut commands,
                            &net_channels,
                            &matchmaker,
                            &mut unit_query,
                            peer_id,
                            &unit_net_ids,
                        );
                    }
                    shared::protocol::ClientMessage::RequestBuild {
                        building_kind,
                        position,
                    } => {
                        economy::handle_build(
                            &mut commands,
                            &net_channels,
                            &mut matchmaker,
                            peer_id,
                            building_kind,
                            position,
                        );
                    }
                    shared::protocol::ClientMessage::RequestTrainUnit {
                        building_net_id,
                        unit_kind,
                    } => {
                        economy::handle_train_unit(
                            &net_channels,
                            &mut matchmaker,
                            &mut prod_query,
                            peer_id,
                            building_net_id,
                            unit_kind,
                        );
                    }
                    shared::protocol::ClientMessage::RequestSetRallyPoint {
                        building_net_id,
                        rally_position,
                    } => {
                        economy::handle_set_rally_point(
                            &mut prod_query,
                            building_net_id,
                            rally_position,
                        );
                    }
                    shared::protocol::ClientMessage::SendChatMessage { text } => {
                        social::handle_chat(&net_channels, &matchmaker, peer_id, text);
                    }
                    shared::protocol::ClientMessage::SendTacticalPing { position, ping_type } => {
                        social::handle_tactical_ping(
                            &net_channels,
                            &matchmaker,
                            peer_id,
                            position,
                            ping_type,
                        );
                    }
                    shared::protocol::ClientMessage::Ping { timestamp } => {
                        social::handle_ping(&net_channels, peer_id, timestamp);
                    }
                }
            }
        }
    }
}

/// Helper to recalculate telemetry and broadcast LobbyStats to all peers
fn broadcast_lobby_stats(
    net_channels: &ServerNetworkChannels,
    matchmaker: &Matchmaker,
) {
    let (q, a1, m1, aso, mso, tot) = matchmaker.get_telemetry();
    crate::net_server::update_global_telemetry(q, a1, aso, tot);
    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
        msg: ServerMessage::LobbyStats {
            queue_1v1: q,
            active_1v1_matches: a1,
            max_1v1_matches: m1,
            active_solo_matches: aso,
            max_solo_matches: mso,
            total_online: tot,
        },
    });
}

/// Helper to handle a player leaving or forfeiting an active match
fn handle_leave_active_match(
    commands: &mut Commands,
    net_channels: &ServerNetworkChannels,
    matchmaker: &mut Matchmaker,
    room_entities: &Query<(Entity, &RoomId)>,
    peer_id: u64,
) {
    if let Some(player) = matchmaker.players.remove(&peer_id) {
        let room_id = player.room_id;
        let remaining_peers: Vec<u64> = matchmaker
            .get_room_peers(room_id)
            .into_iter()
            .filter(|p| *p != peer_id)
            .collect();
        let match_time = matchmaker.rooms.get(&room_id).map(|r| r.match_time).unwrap_or(0.0);
        if let Some(room) = matchmaker.rooms.get_mut(&room_id) {
            room.is_active = false;
        }
        if !remaining_peers.is_empty() {
            let winning_faction = if player.faction == Faction::Player1 {
                Faction::Player2
            } else {
                Faction::Player1
            };
            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                peer_ids: remaining_peers,
                msg: ServerMessage::MatchEnded {
                    winning_faction,
                    duration_seconds: match_time,
                },
            });
        }
        // Despawn all entities belonging to this room from the ECS World
        for (e, r) in room_entities {
            if r.0 == room_id {
                commands.entity(e).despawn_recursive();
            }
        }
        matchmaker.remove_room(room_id);
    }
}
