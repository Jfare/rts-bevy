use bevy::prelude::*;
use shared::components::*;
use shared::protocol::{GameMode, ServerMessage};

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;

/// Checks for Victory or Defeat per room when a Base HQ is destroyed
pub fn server_match_outcome_system(
    mut matchmaker: ResMut<Matchmaker>,
    net_channels: Res<ServerNetworkChannels>,
    hq_query: Query<(&Faction, &RoomId), With<BaseHQ>>,
) {
    let mut ended_rooms = Vec::new();

    for (room_id, room) in matchmaker.rooms.iter() {
        if !room.is_active {
            continue;
        }

        let mut has_p1_hq = false;
        let mut has_enemy_hq = false;

        for (faction, hq_room) in &hq_query {
            if hq_room.0 == *room_id {
                if *faction == Faction::Player1 {
                    has_p1_hq = true;
                } else {
                    has_enemy_hq = true;
                }
            }
        }

        if !has_p1_hq && has_enemy_hq {
            let win_fac = if room.mode == GameMode::SoloVsAi {
                Faction::HostileAi
            } else {
                Faction::Player2
            };
            ended_rooms.push((*room_id, win_fac, room.match_time));
        } else if has_p1_hq && !has_enemy_hq {
            ended_rooms.push((*room_id, Faction::Player1, room.match_time));
        }
    }

    for &(room_id, winning_faction, duration) in &ended_rooms {
        if let Some(room) = matchmaker.rooms.get_mut(&room_id) {
            room.is_active = false;
        }

        info!(
            "🏆 [GameServer] Room #{} finished! Winner: {:?} (Duration: {:.1}s)",
            room_id, winning_faction, duration
        );

        let peers = matchmaker.get_room_peers(room_id);
        if !peers.is_empty() {
            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                peer_ids: peers,
                msg: ServerMessage::MatchEnded {
                    winning_faction,
                    duration_seconds: duration,
                },
            });
        }
    }

    if !ended_rooms.is_empty() {
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

