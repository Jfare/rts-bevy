use std::time::{SystemTime, UNIX_EPOCH};
use bevy::prelude::*;
use shared::protocol::{PingType, ServerMessage};

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;

pub fn handle_chat(
    net_channels: &ServerNetworkChannels,
    matchmaker: &Matchmaker,
    peer_id: u64,
    text: String,
) {
    if let Some(player) = matchmaker.players.get(&peer_id) {
        let peers = matchmaker.get_room_peers(player.room_id);
        if !peers.is_empty() {
            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                peer_ids: peers,
                msg: ServerMessage::ChatMessageReceived {
                    sender_name: player.name.clone(),
                    faction: player.faction,
                    color: player.color,
                    text,
                    is_system: false,
                },
            });
        }
    }
}

pub fn handle_tactical_ping(
    net_channels: &ServerNetworkChannels,
    matchmaker: &Matchmaker,
    peer_id: u64,
    position: Vec2,
    ping_type: PingType,
) {
    if let Some(player) = matchmaker.players.get(&peer_id) {
        let peers = matchmaker.get_room_peers(player.room_id);
        if !peers.is_empty() {
            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                peer_ids: peers,
                msg: ServerMessage::TacticalPingReceived {
                    sender_name: player.name.clone(),
                    faction: player.faction,
                    color: player.color,
                    position,
                    ping_type,
                },
            });
        }
    }
}

pub fn handle_ping(
    net_channels: &ServerNetworkChannels,
    peer_id: u64,
    timestamp: u64,
) {
    let server_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
        peer_id,
        msg: ServerMessage::Pong {
            client_timestamp: timestamp,
            server_time,
        },
    });
}

