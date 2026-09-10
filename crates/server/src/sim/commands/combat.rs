use bevy::prelude::*;
use shared::components::*;
use shared::protocol::ServerMessage;

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;
use super::{get_player_and_room, UnitQuery};

pub fn handle_attack_target(
    commands: &mut Commands,
    net_channels: &ServerNetworkChannels,
    matchmaker: &Matchmaker,
    unit_query: &mut UnitQuery,
    peer_id: u64,
    unit_net_ids: &[u32],
    target_net_id: u32,
) {
    let (player_faction, player_room) = get_player_and_room(matchmaker, peer_id);
    let peers = matchmaker.get_room_peers(player_room);
    let target_entity = unit_query
        .iter()
        .find(|(_, _, net_entity, _, unit_room, ..)| {
            net_entity.net_id == target_net_id && unit_room.0 == player_room
        })
        .map(|(e, ..)| e);

    if let Some(target) = target_entity {
        let mut valid_net_ids = Vec::new();
        for (e, _, net_entity, faction, unit_room, _, soldier_opt, _, melee_opt, _, _) in
            unit_query.iter_mut()
        {
            if unit_net_ids.contains(&net_entity.net_id)
                && *faction == player_faction
                && unit_room.0 == player_room
            {
                commands.entity(e).remove::<MoveTarget>();
                if let Some(mut soldier) = soldier_opt {
                    soldier.target = Some(target);
                    soldier.state = SoldierState::ChasingTarget;
                }
                if let Some(mut melee) = melee_opt {
                    melee.target = Some(target);
                    melee.state = SoldierState::ChasingTarget;
                }
                valid_net_ids.push(net_entity.net_id);
            }
        }

        if !peers.is_empty() && !valid_net_ids.is_empty() {
            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                peer_ids: peers,
                msg: ServerMessage::UnitsOrderedAttackTarget {
                    unit_net_ids: valid_net_ids,
                    target_net_id,
                },
            });
        }
    }
}

