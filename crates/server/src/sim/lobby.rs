use bevy::prelude::*;
use shared::components::*;
use shared::protocol::{FactionColor, GameMode, ServerMessage};

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::spawner::spawn_match_entities;
use crate::session::{Matchmaker, PlayerSession, Room};

pub fn handle_join_lobby(
    commands: &mut Commands,
    net_channels: &Res<ServerNetworkChannels>,
    matchmaker: &mut ResMut<Matchmaker>,
    peer_id: u64,
    player_name: String,
    mode: GameMode,
    room_code: Option<String>,
    faction_color: Option<FactionColor>,
) {
    let color = faction_color.unwrap_or(FactionColor::Blue);

    match mode {
        GameMode::SoloVsAi => {
            if !matchmaker.can_start_solo() {
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                    peer_id,
                    msg: ServerMessage::ErrorMessage {
                        reason: "Server Solo capacity full (10/10 matches). Please try again shortly.".to_string(),
                    },
                });
                return;
            }

            let room_id = matchmaker.next_room_id;
            matchmaker.next_room_id += 1;

            matchmaker.players.insert(
                peer_id,
                PlayerSession {
                    peer_id,
                    name: player_name.clone(),
                    room_id,
                    faction: Faction::Player1,
                    color,
                },
            );

            let room = Room::new(room_id, None, mode, Some(peer_id), None);
            let (p1_cur_sup, p1_max_sup) = room.economy.get_supply(Faction::Player1);
            let (ai_cur_sup, ai_max_sup) = room.economy.get_supply(Faction::HostileAi);
            let p1_minerals = room.economy.get_minerals(Faction::Player1);
            let p2_minerals = room.economy.get_minerals(Faction::HostileAi);

            matchmaker.rooms.insert(room_id, room);

            let initial_entities = spawn_match_entities(
                commands,
                matchmaker,
                room_id,
                GameMode::SoloVsAi,
                peer_id,
                None,
            );

            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                peer_id,
                msg: ServerMessage::LobbyJoined {
                    player_id: peer_id,
                    assigned_faction: Faction::Player1,
                    room_id,
                    room_code: None,
                    is_game_ready: true,
                },
            });

            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                peer_id,
                msg: ServerMessage::GameStarted {
                    p1_pos: shared::map::P1_BASE_POS,
                    p2_pos: shared::map::P2_BASE_POS,
                    wave_initial_delay: 40.0,
                },
            });

            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                peer_id,
                msg: ServerMessage::InitialWorldState {
                    entities: initial_entities,
                    p1_minerals,
                    p1_supply: p1_cur_sup,
                    p1_max_supply: p1_max_sup,
                    p2_minerals,
                    p2_supply: ai_cur_sup,
                    p2_max_supply: ai_max_sup,
                },
            });

            // Send welcome system notice
            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                peer_id,
                msg: ServerMessage::ChatMessageReceived {
                    sender_name: "SYSTEM".to_string(),
                    faction: Faction::Neutral,
                    color: FactionColor::Amber,
                    text: format!("Commander {} deployed to Sector 4. Defend your Base HQ against hostile assault waves!", player_name),
                    is_system: true,
                },
            });

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
        GameMode::Multiplayer1v1 => {
            if let Some(waiting_p1) = matchmaker.waiting_1v1_peer {
                if !matchmaker.can_start_pvp() {
                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                        peer_id,
                        msg: ServerMessage::ErrorMessage {
                            reason: "Server PvP capacity full (10/10 matches). Waiting for a match slot...".to_string(),
                        },
                    });
                    return;
                }
                matchmaker.waiting_1v1_peer.take();
                // Pair both players!
                let room_id = matchmaker.next_room_id;
                matchmaker.next_room_id += 1;

                let p1_name = matchmaker
                    .players
                    .get(&waiting_p1)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Commander 1".to_string());

                let p1_color = matchmaker
                    .players
                    .get(&waiting_p1)
                    .map(|p| p.color)
                    .unwrap_or(FactionColor::Blue);

                let p2_color = if color == p1_color { FactionColor::Red } else { color };

                matchmaker.players.insert(
                    waiting_p1,
                    PlayerSession {
                        peer_id: waiting_p1,
                        name: p1_name.clone(),
                        room_id,
                        faction: Faction::Player1,
                        color: p1_color,
                    },
                );

                matchmaker.players.insert(
                    peer_id,
                    PlayerSession {
                        peer_id,
                        name: player_name.clone(),
                        room_id,
                        faction: Faction::Player2,
                        color: p2_color,
                    },
                );

                let mut room = Room::new(room_id, None, mode, Some(waiting_p1), Some(peer_id));
                room.countdown_timer = 3.0;

                let (p1_cur_sup, p1_max_sup) = room.economy.get_supply(Faction::Player1);
                let (p2_cur_sup, p2_max_sup) = room.economy.get_supply(Faction::Player2);
                let p1_minerals = room.economy.get_minerals(Faction::Player1);
                let p2_minerals = room.economy.get_minerals(Faction::Player2);

                matchmaker.rooms.insert(room_id, room);

                let initial_entities = spawn_match_entities(
                    commands,
                    matchmaker,
                    room_id,
                    GameMode::Multiplayer1v1,
                    waiting_p1,
                    Some(peer_id),
                );

                for (p_id, faction, opp_name, opp_color) in [
                    (waiting_p1, Faction::Player1, player_name.clone(), p2_color),
                    (peer_id, Faction::Player2, p1_name.clone(), p1_color),
                ] {
                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                        peer_id: p_id,
                        msg: ServerMessage::MatchFound {
                            opponent_name: opp_name,
                            opponent_color: opp_color,
                            countdown_seconds: 3.0,
                        },
                    });

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                        peer_id: p_id,
                        msg: ServerMessage::LobbyJoined {
                            player_id: p_id,
                            assigned_faction: faction,
                            room_id,
                            room_code: None,
                            is_game_ready: true,
                        },
                    });

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                        peer_id: p_id,
                        msg: ServerMessage::GameStarted {
                            p1_pos: shared::map::P1_BASE_POS,
                            p2_pos: shared::map::P2_BASE_POS,
                            wave_initial_delay: 0.0,
                        },
                    });

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                        peer_id: p_id,
                        msg: ServerMessage::InitialWorldState {
                            entities: initial_entities.clone(),
                            p1_minerals,
                            p1_supply: p1_cur_sup,
                            p1_max_supply: p1_max_sup,
                            p2_minerals,
                            p2_supply: p2_cur_sup,
                            p2_max_supply: p2_max_sup,
                        },
                    });
                }

                // Announce match start in room chat
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                    peer_ids: vec![waiting_p1, peer_id],
                    msg: ServerMessage::ChatMessageReceived {
                        sender_name: "SYSTEM".to_string(),
                        faction: Faction::Neutral,
                        color: FactionColor::Amber,
                        text: format!("1v1 Match started! [{}] vs [{}]", p1_name, player_name),
                        is_system: true,
                    },
                });

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
            } else {
                // First player in 1v1 queue -> wait for opponent
                matchmaker.waiting_1v1_peer = Some(peer_id);
                matchmaker.players.insert(
                    peer_id,
                    PlayerSession {
                        peer_id,
                        name: player_name,
                        room_id: 0,
                        faction: Faction::Player1,
                        color,
                    },
                );

                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                    peer_id,
                    msg: ServerMessage::LobbyJoined {
                        player_id: peer_id,
                        assigned_faction: Faction::Player1,
                        room_id: 0,
                        room_code: None,
                        is_game_ready: false,
                    },
                });

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
        }
        GameMode::CustomPrivate => {
            if let Some(code) = room_code {
                // Player is attempting to join an existing private room with a 4-digit code
                if let Some(target_room_id) = matchmaker.find_room_by_code(&code) {
                    let waiting_p1 = matchmaker.rooms.get(&target_room_id).and_then(|r| r.p1_peer).unwrap_or(0);
                    if let Some(room) = matchmaker.rooms.get_mut(&target_room_id) {
                        room.p2_peer = Some(peer_id);
                        room.is_active = true;
                        room.countdown_timer = 3.0;
                    }

                    let p1_name = matchmaker
                        .players
                        .get(&waiting_p1)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "Commander 1".to_string());

                    let p1_color = matchmaker
                        .players
                        .get(&waiting_p1)
                        .map(|p| p.color)
                        .unwrap_or(FactionColor::Blue);

                    let p2_color = if color == p1_color { FactionColor::Red } else { color };

                    matchmaker.players.insert(
                        peer_id,
                        PlayerSession {
                            peer_id,
                            name: player_name.clone(),
                            room_id: target_room_id,
                            faction: Faction::Player2,
                            color: p2_color,
                        },
                    );

                    let initial_entities = spawn_match_entities(
                        commands,
                        matchmaker,
                        target_room_id,
                        GameMode::Multiplayer1v1,
                        waiting_p1,
                        Some(peer_id),
                    );

                    let (p1_cur_sup, p1_max_sup, p1_minerals, p2_cur_sup, p2_max_sup, p2_minerals) = {
                        let room = matchmaker.rooms.get(&target_room_id).unwrap();
                        let (p1_c, p1_m) = room.economy.get_supply(Faction::Player1);
                        let (p2_c, p2_m) = room.economy.get_supply(Faction::Player2);
                        (
                            p1_c,
                            p1_m,
                            room.economy.get_minerals(Faction::Player1),
                            p2_c,
                            p2_m,
                            room.economy.get_minerals(Faction::Player2),
                        )
                    };

                    for (p_id, faction, opp_name, opp_color) in [
                        (waiting_p1, Faction::Player1, player_name.clone(), p2_color),
                        (peer_id, Faction::Player2, p1_name.clone(), p1_color),
                    ] {
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                            peer_id: p_id,
                            msg: ServerMessage::MatchFound {
                                opponent_name: opp_name,
                                opponent_color: opp_color,
                                countdown_seconds: 3.0,
                            },
                        });

                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                            peer_id: p_id,
                            msg: ServerMessage::LobbyJoined {
                                player_id: p_id,
                                assigned_faction: faction,
                                room_id: target_room_id,
                                room_code: Some(code.clone()),
                                is_game_ready: true,
                            },
                        });

                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                            peer_id: p_id,
                            msg: ServerMessage::GameStarted {
                                p1_pos: shared::map::P1_BASE_POS,
                                p2_pos: shared::map::P2_BASE_POS,
                                wave_initial_delay: 0.0,
                            },
                        });

                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                            peer_id: p_id,
                            msg: ServerMessage::InitialWorldState {
                                entities: initial_entities.clone(),
                                p1_minerals,
                                p1_supply: p1_cur_sup,
                                p1_max_supply: p1_max_sup,
                                p2_minerals,
                                p2_supply: p2_cur_sup,
                                p2_max_supply: p2_max_sup,
                            },
                        });
                    }

                    // System chat notice
                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                        peer_ids: vec![waiting_p1, peer_id],
                        msg: ServerMessage::ChatMessageReceived {
                            sender_name: "SYSTEM".to_string(),
                            faction: Faction::Neutral,
                            color: FactionColor::Amber,
                            text: format!("Private match started! [{}] vs [{}] (Room Code: {})", p1_name, player_name, code.to_uppercase()),
                            is_system: true,
                        },
                    });

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
                } else {
                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                        peer_id,
                        msg: ServerMessage::ErrorMessage {
                            reason: format!("No active private lobby found with code '{}'.", code.to_uppercase()),
                        },
                    });
                }
            } else {
                // Host creates a new private lobby room with generated 4-digit code
                let room_id = matchmaker.next_room_id;
                matchmaker.next_room_id += 1;
                let generated_code = matchmaker.generate_room_code();

                matchmaker.players.insert(
                    peer_id,
                    PlayerSession {
                        peer_id,
                        name: player_name.clone(),
                        room_id,
                        faction: Faction::Player1,
                        color,
                    },
                );

                let mut room = Room::new(
                    room_id,
                    Some(generated_code.clone()),
                    GameMode::CustomPrivate,
                    Some(peer_id),
                    None,
                );
                room.is_active = false;

                matchmaker.rooms.insert(room_id, room);

                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                    peer_id,
                    msg: ServerMessage::LobbyJoined {
                        player_id: peer_id,
                        assigned_faction: Faction::Player1,
                        room_id,
                        room_code: Some(generated_code.clone()),
                        is_game_ready: false,
                    },
                });

                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                    peer_id,
                    msg: ServerMessage::ChatMessageReceived {
                        sender_name: "SYSTEM".to_string(),
                        faction: Faction::Neutral,
                        color: FactionColor::Amber,
                        text: format!("Private Room created! Share code [{}] with your opponent to join.", generated_code),
                        is_system: true,
                    },
                });
            }
        }
    }
}

