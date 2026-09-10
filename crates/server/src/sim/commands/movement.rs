use bevy::prelude::*;
use shared::components::*;
use shared::grid::NavGrid;
use shared::protocol::ServerMessage;

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;
use super::{get_player_and_room, UnitQuery};

pub fn handle_move(
    commands: &mut Commands,
    nav_grid: &NavGrid,
    net_channels: &ServerNetworkChannels,
    matchmaker: &Matchmaker,
    unit_query: &mut UnitQuery,
    peer_id: u64,
    unit_net_ids: &[u32],
    target_position: Vec2,
    is_attack_move: bool,
) {
    let (player_faction, player_room) = get_player_and_room(matchmaker, peer_id);
    let peers = matchmaker.get_room_peers(player_room);
    let unit_count = unit_net_ids.len();
    let mut destinations = Vec::new();
    let mut valid_net_ids = Vec::new();

    for (i, &u_net_id) in unit_net_ids.iter().enumerate() {
        let formation_offset = if unit_count > 1 {
            let angle = (i as f32) * 2.39996;
            let dist = 24.0 * (i as f32).sqrt();
            Vec2::new(angle.cos(), angle.sin()) * dist
        } else {
            Vec2::ZERO
        };
        let dest = target_position + formation_offset;

        for (e, tf, net_entity, faction, unit_room, move_target_opt, soldier_opt, worker_opt, melee_opt, _, stance_opt) in
            unit_query.iter_mut()
        {
            if net_entity.net_id == u_net_id
                && *faction == player_faction
                && unit_room.0 == player_room
            {
                if let Some(mut soldier) = soldier_opt {
                    soldier.target = None;
                    soldier.state = if is_attack_move {
                        SoldierState::AttackMoving
                    } else {
                        SoldierState::MovingToGround
                    };
                }
                if let Some(mut melee) = melee_opt {
                    melee.target = None;
                    melee.state = if is_attack_move {
                        SoldierState::AttackMoving
                    } else {
                        SoldierState::MovingToGround
                    };
                }
                if let Some(mut worker) = worker_opt {
                    worker.state = WorkerState::Idle;
                    worker.target_node = None;
                }
                if let Some(mut stance) = stance_opt {
                    *stance = TacticalStance::Aggressive;
                }

                let unit_pos = tf.translation.truncate();
                let waypoints = nav_grid.find_path(unit_pos, dest);

                if let Some(mut mt) = move_target_opt {
                    mt.destination = dest;
                    mt.is_attack_move = is_attack_move;
                    mt.waypoints = waypoints;
                    mt.current_waypoint_idx = 0;
                } else {
                    commands.entity(e).insert(MoveTarget::with_waypoints(
                        dest,
                        is_attack_move,
                        waypoints,
                    ));
                }

                valid_net_ids.push(u_net_id);
                destinations.push(dest);
                break;
            }
        }
    }

    if !peers.is_empty() && !valid_net_ids.is_empty() {
        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
            peer_ids: peers,
            msg: ServerMessage::UnitsOrderedMove {
                unit_net_ids: valid_net_ids.clone(),
                destinations,
                is_attack_move,
            },
        });
    }
}

pub fn handle_patrol(
    commands: &mut Commands,
    nav_grid: &NavGrid,
    net_channels: &ServerNetworkChannels,
    matchmaker: &Matchmaker,
    unit_query: &mut UnitQuery,
    peer_id: u64,
    unit_net_ids: &[u32],
    target_position: Vec2,
) {
    let (player_faction, player_room) = get_player_and_room(matchmaker, peer_id);
    let peers = matchmaker.get_room_peers(player_room);
    let unit_count = unit_net_ids.len();
    let mut destinations = Vec::new();
    let mut valid_net_ids = Vec::new();

    for (i, &u_net_id) in unit_net_ids.iter().enumerate() {
        let formation_offset = if unit_count > 1 {
            let angle = (i as f32) * 2.39996;
            let dist = 24.0 * (i as f32).sqrt();
            Vec2::new(angle.cos(), angle.sin()) * dist
        } else {
            Vec2::ZERO
        };
        let dest = target_position + formation_offset;

        for (e, tf, net_entity, faction, unit_room, move_target_opt, soldier_opt, _, _, _, stance_opt) in
            unit_query.iter_mut()
        {
            if net_entity.net_id == u_net_id
                && *faction == player_faction
                && unit_room.0 == player_room
            {
                let unit_pos = tf.translation.truncate();
                let waypoints = nav_grid.find_path(unit_pos, dest);

                if let Some(mut stance) = stance_opt {
                    *stance = TacticalStance::Patrol {
                        origin: unit_pos,
                        target: dest,
                        heading_to_target: true,
                    };
                } else {
                    commands.entity(e).insert(TacticalStance::Patrol {
                        origin: unit_pos,
                        target: dest,
                        heading_to_target: true,
                    });
                }

                if let Some(mut soldier) = soldier_opt {
                    soldier.target = None;
                    soldier.state = SoldierState::AttackMoving;
                }

                if let Some(mut mt) = move_target_opt {
                    mt.destination = dest;
                    mt.is_attack_move = true;
                    mt.waypoints = waypoints;
                    mt.current_waypoint_idx = 0;
                } else {
                    commands.entity(e).insert(MoveTarget::with_waypoints(
                        dest,
                        true,
                        waypoints,
                    ));
                }

                valid_net_ids.push(u_net_id);
                destinations.push(dest);
                break;
            }
        }
    }

    if !peers.is_empty() && !valid_net_ids.is_empty() {
        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
            peer_ids: peers,
            msg: ServerMessage::UnitsOrderedPatrol {
                unit_net_ids: valid_net_ids,
                destinations,
            },
        });
    }
}

pub fn handle_stop(
    commands: &mut Commands,
    net_channels: &ServerNetworkChannels,
    matchmaker: &Matchmaker,
    unit_query: &mut UnitQuery,
    peer_id: u64,
    unit_net_ids: &[u32],
) {
    let (player_faction, player_room) = get_player_and_room(matchmaker, peer_id);
    let peers = matchmaker.get_room_peers(player_room);
    let mut valid_net_ids = Vec::new();

    for (e, _, net_entity, faction, unit_room, _, soldier_opt, worker_opt, melee_opt, _, stance_opt) in
        unit_query.iter_mut()
    {
        if unit_net_ids.contains(&net_entity.net_id)
            && *faction == player_faction
            && unit_room.0 == player_room
        {
            commands.entity(e).remove::<MoveTarget>();
            if let Some(mut soldier) = soldier_opt {
                soldier.target = None;
                soldier.state = SoldierState::Idle;
            }
            if let Some(mut melee) = melee_opt {
                melee.target = None;
                melee.state = SoldierState::Idle;
            }
            if let Some(mut worker) = worker_opt {
                worker.state = WorkerState::Idle;
            }
            if let Some(mut stance) = stance_opt {
                *stance = TacticalStance::Aggressive;
            }
            valid_net_ids.push(net_entity.net_id);
        }
    }

    if !peers.is_empty() && !valid_net_ids.is_empty() {
        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
            peer_ids: peers,
            msg: ServerMessage::UnitsOrderedStop {
                unit_net_ids: valid_net_ids,
            },
        });
    }
}

pub fn handle_hold_position(
    commands: &mut Commands,
    net_channels: &ServerNetworkChannels,
    matchmaker: &Matchmaker,
    unit_query: &mut UnitQuery,
    peer_id: u64,
    unit_net_ids: &[u32],
) {
    let (player_faction, player_room) = get_player_and_room(matchmaker, peer_id);
    let peers = matchmaker.get_room_peers(player_room);
    let mut valid_net_ids = Vec::new();

    for (e, _, net_entity, faction, unit_room, _, soldier_opt, _, mut melee_opt, _, stance_opt) in
        unit_query.iter_mut()
    {
        if unit_net_ids.contains(&net_entity.net_id)
            && *faction == player_faction
            && unit_room.0 == player_room
        {
            commands.entity(e).remove::<MoveTarget>();
            if let Some(mut soldier) = soldier_opt {
                soldier.target = None;
                soldier.state = SoldierState::HoldingPosition;
            }
            if let Some(ref mut melee) = melee_opt {
                melee.target = None;
                melee.state = SoldierState::HoldingPosition;
            }
            if let Some(mut stance) = stance_opt {
                *stance = TacticalStance::HoldPosition;
            } else {
                commands.entity(e).insert(TacticalStance::HoldPosition);
            }
            valid_net_ids.push(net_entity.net_id);
        }
    }

    if !peers.is_empty() && !valid_net_ids.is_empty() {
        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
            peer_ids: peers,
            msg: ServerMessage::UnitsOrderedHoldPosition {
                unit_net_ids: valid_net_ids,
            },
        });
    }
}

