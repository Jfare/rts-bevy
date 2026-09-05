use bevy::prelude::*;
use crate::game_session::{spawn_match_entities, Matchmaker, PlayerSession, Room};
use crate::net_server::{IncomingNetEvent, OutgoingNetEvent, ServerNetworkChannels};
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::{BuildingKind, NavGrid};
use shared::protocol::{
    EntitySnapshot, FactionColor, GameMode, ServerMessage, UnitKind,
};
#[cfg(test)]
use shared::protocol::{ClientMessage, PingType};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ServerSimulationPlugin;

impl Plugin for ServerSimulationPlugin {
    fn build(&self, app: &mut App) {
        let mut economy = PlayerEconomy::new();
        economy.register_supply(Faction::Player1, 8);
        economy.register_supply(Faction::Player2, 8);
        economy.register_supply(Faction::HostileAi, 4);

        app.insert_resource(Matchmaker::new())
            .insert_resource(economy)
            .init_resource::<NavGrid>()
            .insert_resource(ServerTickTimer(Timer::from_seconds(
                1.0 / 30.0,
                TimerMode::Repeating,
            )))
            .insert_resource(ServerStatsTimer(Timer::from_seconds(
                2.0,
                TimerMode::Repeating,
            )))
            .add_systems(
                Update,
                (
                    server_room_tick_system,
                    handle_incoming_network_events,
                    update_server_nav_grid_system,
                    server_combat_system,
                    server_turret_combat_system,
                    server_siege_tank_combat_system,
                    server_movement_system,
                    server_abilities_and_stances_system,
                    server_unit_separation_and_collision_system,
                    server_mining_system,
                    server_production_system,
                    server_solo_wave_spawner_system,
                    server_match_outcome_system,
                    server_tick_snapshot_system,
                    server_lobby_stats_broadcast_system,
                ).chain(),
            );
    }
}

#[derive(Resource)]
pub struct ServerTickTimer(pub Timer);

#[derive(Resource)]
pub struct ServerStatsTimer(pub Timer);

fn server_room_tick_system(
    time: Res<Time>,
    mut matchmaker: ResMut<Matchmaker>,
) {
    let dt = time.delta_secs();
    for room in matchmaker.rooms.values_mut() {
        if room.is_active {
            if room.countdown_timer > 0.0 {
                room.countdown_timer = (room.countdown_timer - dt).max(0.0);
            } else {
                room.match_time += dt;
            }
        }
    }
}

fn server_lobby_stats_broadcast_system(
    time: Res<Time>,
    mut timer: ResMut<ServerStatsTimer>,
    matchmaker: Res<Matchmaker>,
    net_channels: Res<ServerNetworkChannels>,
) {
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
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

/// Reads and executes client network commands
fn handle_incoming_network_events(
    mut commands: Commands,
    net_channels: Res<ServerNetworkChannels>,
    mut matchmaker: ResMut<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    nav_grid: Res<NavGrid>,
    room_entities: Query<(Entity, &RoomId)>,
    mut unit_query: Query<(
        Entity,
        &Transform,
        &NetEntity,
        &Faction,
        &RoomId,
        Option<&mut MoveTarget>,
        Option<&mut Soldier>,
        Option<&mut Worker>,
        Option<&mut Stimpack>,
        Option<&mut SiegeTank>,
        Option<&mut Health>,
        Option<&mut TacticalStance>,
    ), Without<ResourceNode>>,
    node_query: Query<(Entity, &NetEntity, &Transform, &RoomId), With<ResourceNode>>,
    mut prod_query: Query<(Entity, &NetEntity, &Faction, &RoomId, &mut ProductionBuilding)>,
) {
    while let Ok(event) = net_channels.rx_incoming.try_recv() {
        match event {
            IncomingNetEvent::PeerConnected { peer_id, addr } => {
                info!("🎮 [GameServer] Peer #{} connected from {}", peer_id, addr);
            }
            IncomingNetEvent::PeerDisconnected { peer_id } => {
                info!("🎮 [GameServer] Peer #{} disconnected", peer_id);
                if matchmaker.waiting_1v1_peer == Some(peer_id) {
                    matchmaker.waiting_1v1_peer = None;
                }
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
                    for (e, r) in &room_entities {
                        if r.0 == room_id {
                            commands.entity(e).despawn_recursive();
                        }
                    }
                    matchmaker.remove_room(room_id);
                }
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
                            &mut economy,
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
                    shared::protocol::ClientMessage::ForfeitMatch => {
                        info!("🏳️ [GameServer] Peer #{} forfeited active match", peer_id);
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
                            for (e, r) in &room_entities {
                                if r.0 == room_id {
                                    commands.entity(e).despawn_recursive();
                                }
                            }
                            matchmaker.remove_room(room_id);
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
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

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

                            for (e, tf, net_entity, faction, unit_room, move_target_opt, soldier_opt, worker_opt, _, tank_opt, _, stance_opt) in
                                &mut unit_query
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
                                    if let Some(mut tank) = tank_opt {
                                        tank.target = None;
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
                                    unit_net_ids: valid_net_ids,
                                    destinations,
                                    is_attack_move,
                                },
                            });
                        }
                    }
                    shared::protocol::ClientMessage::RequestPatrol {
                        unit_net_ids,
                        target_position,
                    } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

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

                            for (e, tf, net_entity, faction, unit_room, move_target_opt, soldier_opt, _, _, _, _, stance_opt) in
                                &mut unit_query
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
                    shared::protocol::ClientMessage::RequestAttackTarget {
                        unit_net_ids,
                        target_net_id,
                    } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

                        let peers = matchmaker.get_room_peers(player_room);
                        let target_entity = unit_query
                            .iter()
                            .find(|(_, _, net_entity, _, unit_room, _, _, _, _, _, _, _)| {
                                net_entity.net_id == target_net_id && unit_room.0 == player_room
                            })
                            .map(|(e, _, _, _, _, _, _, _, _, _, _, _)| e);

                        if let Some(target) = target_entity {
                            let mut valid_net_ids = Vec::new();
                            for (e, _, net_entity, faction, unit_room, _, soldier_opt, _, _, tank_opt, _, _) in
                                &mut unit_query
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
                                    if let Some(mut tank) = tank_opt {
                                        tank.target = Some(target);
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
                    shared::protocol::ClientMessage::RequestHarvest {
                        worker_net_ids,
                        resource_net_id,
                    } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

                        let peers = matchmaker.get_room_peers(player_room);
                        let target_node = node_query
                            .iter()
                            .find(|(_, net_entity, _, node_room)| {
                                net_entity.net_id == resource_net_id && node_room.0 == player_room
                            })
                            .map(|(e, _, _, _)| e);

                        if let Some(node_e) = target_node {
                            let mut valid_net_ids = Vec::new();
                            for (e, _, net_entity, faction, unit_room, _, _, worker_opt, _, _, _, _) in
                                &mut unit_query
                            {
                                if worker_net_ids.contains(&net_entity.net_id)
                                    && *faction == player_faction
                                    && unit_room.0 == player_room
                                {
                                    commands.entity(e).remove::<MoveTarget>();
                                    if let Some(mut worker) = worker_opt {
                                        worker.target_node = Some(node_e);
                                        worker.state = WorkerState::MovingToResource;
                                        worker.harvest_timer = 0.0;
                                    }
                                    valid_net_ids.push(net_entity.net_id);
                                }
                            }

                            if !peers.is_empty() && !valid_net_ids.is_empty() {
                                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                    peer_ids: peers,
                                    msg: ServerMessage::WorkersOrderedHarvest {
                                        worker_net_ids: valid_net_ids,
                                        resource_net_id,
                                    },
                                });
                            }
                        }

                    }
                    shared::protocol::ClientMessage::RequestStop { unit_net_ids } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

                        let peers = matchmaker.get_room_peers(player_room);
                        let mut valid_net_ids = Vec::new();
                        for (e, _, net_entity, faction, unit_room, _, soldier_opt, worker_opt, _, mut tank_opt, _, stance_opt) in
                            &mut unit_query
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
                                if let Some(ref mut tank) = tank_opt {
                                    tank.target = None;
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
                    shared::protocol::ClientMessage::RequestHoldPosition { unit_net_ids } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

                        let peers = matchmaker.get_room_peers(player_room);
                        let mut valid_net_ids = Vec::new();
                        for (e, _, net_entity, faction, unit_room, _, soldier_opt, _, _, mut tank_opt, _, stance_opt) in
                            &mut unit_query
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
                                if let Some(ref mut tank) = tank_opt {
                                    tank.target = None;
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
                    shared::protocol::ClientMessage::RequestStimpack { unit_net_ids } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

                        let peers = matchmaker.get_room_peers(player_room);
                        let mut valid_net_ids = Vec::new();
                        for (e, _, net_entity, faction, unit_room, _, _, _, stim_opt, _, health_opt, _) in
                            &mut unit_query
                        {
                            if unit_net_ids.contains(&net_entity.net_id)
                                && *faction == player_faction
                                && unit_room.0 == player_room
                            {
                                if let Some(mut health) = health_opt {
                                    if health.current > 20.0 {
                                        health.take_damage(15.0);
                                        if let Some(mut stim) = stim_opt {
                                            stim.is_active = true;
                                            stim.timer = 6.0;
                                        } else {
                                            commands.entity(e).insert(Stimpack {
                                                is_active: true,
                                                timer: 6.0,
                                                duration: 6.0,
                                            });
                                        }
                                        valid_net_ids.push(net_entity.net_id);
                                    }
                                }
                            }
                        }

                        if !peers.is_empty() && !valid_net_ids.is_empty() {
                            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                peer_ids: peers,
                                msg: ServerMessage::UnitsActivatedStimpack {
                                    unit_net_ids: valid_net_ids,
                                },
                            });
                        }
                    }
                    shared::protocol::ClientMessage::RequestToggleSiegeMode { unit_net_ids } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

                        let peers = matchmaker.get_room_peers(player_room);
                        let mut valid_net_ids = Vec::new();
                        for (e, _, net_entity, faction, unit_room, _, _, _, _, tank_opt, _, _) in
                            &mut unit_query
                        {
                            if unit_net_ids.contains(&net_entity.net_id)
                                && *faction == player_faction
                                && unit_room.0 == player_room
                            {
                                if let Some(mut tank) = tank_opt {
                                    match tank.mode {
                                        TankMode::Tank => {
                                            tank.mode = TankMode::TransformingToSiege;
                                            tank.transform_timer = 1.0;
                                            commands.entity(e).remove::<MoveTarget>();
                                            valid_net_ids.push(net_entity.net_id);
                                        }
                                        TankMode::Siege => {
                                            tank.mode = TankMode::TransformingToTank;
                                            tank.transform_timer = 1.0;
                                            valid_net_ids.push(net_entity.net_id);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }

                        if !peers.is_empty() && !valid_net_ids.is_empty() {
                            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                peer_ids: peers,
                                msg: ServerMessage::UnitsToggledSiegeMode {
                                    unit_net_ids: valid_net_ids,
                                },
                            });
                        }
                    }

                    shared::protocol::ClientMessage::RequestBuild {
                        building_kind,
                        position,
                    } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

                        let b_radius = building_kind.size().x.max(building_kind.size().y) * 0.5;
                        if shared::map::is_obstacle_blocked(position, b_radius, 4.0) {
                            continue;
                        }

                        if economy.has_minerals(player_faction, building_kind.mineral_cost()) {
                            economy.spend_minerals(player_faction, building_kind.mineral_cost());
                            let net_id = matchmaker.alloc_net_id();

                            let mut entity_cmds = commands.spawn((
                                Building::new(
                                    building_kind.name(),
                                    building_kind.size(),
                                    building_kind.build_duration(),
                                    false,
                                ),
                                Health::new(building_kind.max_health()),
                                player_faction,
                                Radius(building_kind.size().x.max(building_kind.size().y) * 0.5),
                                RoomId(player_room),
                                NetEntity {
                                    net_id,
                                    owner_peer_id: peer_id,
                                },
                                Transform::from_xyz(position.x, position.y, 1.0),
                            ));

                            match building_kind {
                                BuildingKind::BaseHQ => {
                                    entity_cmds.insert((
                                        BaseHQ {
                                            supply_provided: 10,
                                            dropoff_radius: 70.0,
                                        },
                                        ProductionBuilding {
                                            queue: Vec::new(),
                                            current_timer: 0.0,
                                            max_queue_size: 5,
                                            rally_point: position + Vec2::new(0.0, -100.0),
                                        },
                                    ));
                                }
                                BuildingKind::Barracks => {
                                    entity_cmds.insert((
                                        Barracks,
                                        ProductionBuilding {
                                            queue: Vec::new(),
                                            current_timer: 0.0,
                                            max_queue_size: 5,
                                            rally_point: position + Vec2::new(0.0, -100.0),
                                        },
                                    ));
                                }
                                BuildingKind::SupplyDepot => {
                                    entity_cmds.insert(SupplyDepot { supply_provided: 8 });
                                }
                                BuildingKind::Turret => {
                                    entity_cmds.insert(GunTurret::default());
                                }
                            }

                            let peers = matchmaker.get_room_peers(player_room);
                            if !peers.is_empty() {
                                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                    peer_ids: peers,
                                    msg: ServerMessage::BuildingSpawned {
                                        net_id,
                                        faction: player_faction,
                                        building_kind,
                                        position,
                                        max_hp: building_kind.max_health(),
                                    },
                                });
                            }
                        }
                    }
                    shared::protocol::ClientMessage::RequestTrainUnit {
                        building_net_id,
                        unit_kind,
                    } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);
                        let player_room = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.room_id)
                            .unwrap_or(0);

                        for (_, net_entity, faction, b_room, mut prod) in &mut prod_query {
                            if net_entity.net_id == building_net_id
                                && *faction == player_faction
                                && b_room.0 == player_room
                                && economy.has_minerals(player_faction, unit_kind.mineral_cost())
                                    && economy.has_supply(player_faction, unit_kind.supply_cost())
                                    && prod.queue.len() < prod.max_queue_size
                                {
                                    economy.spend_minerals(player_faction, unit_kind.mineral_cost());
                                    economy.register_supply(player_faction, unit_kind.supply_cost());

                                    prod.queue.push(QueuedUnit {
                                        name: unit_kind.name().to_string(),
                                        mineral_cost: unit_kind.mineral_cost(),
                                        supply_cost: unit_kind.supply_cost(),
                                        build_duration: unit_kind.train_duration(),
                                    });

                                    let peers = matchmaker.get_room_peers(player_room);
                                    if !peers.is_empty() {
                                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                            peer_ids: peers,
                                            msg: ServerMessage::QueueUpdated {
                                                building_net_id,
                                                queue_count: prod.queue.len(),
                                                current_progress: if prod.queue.is_empty() {
                                                    0.0
                                                } else {
                                                    prod.current_timer / prod.queue[0].build_duration
                                                },
                                            },
                                        });
                                    }
                                }
                        }
                    }
                    shared::protocol::ClientMessage::RequestSetRallyPoint {
                        building_net_id,
                        rally_position,
                    } => {
                        for (_, net_entity, _, _, mut prod) in &mut prod_query {
                            if net_entity.net_id == building_net_id {
                                prod.rally_point = rally_position;
                            }
                        }
                    }
                    shared::protocol::ClientMessage::SendChatMessage { text } => {
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
                    shared::protocol::ClientMessage::SendTacticalPing { position, ping_type } => {
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
                    shared::protocol::ClientMessage::Ping { timestamp } => {
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
                }
            }
        }
    }
}

fn handle_join_lobby(
    commands: &mut Commands,
    net_channels: &Res<ServerNetworkChannels>,
    matchmaker: &mut ResMut<Matchmaker>,
    economy: &mut ResMut<PlayerEconomy>,
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

            matchmaker.rooms.insert(
                room_id,
                Room {
                    room_id,
                    room_code: None,
                    mode,
                    p1_peer: Some(peer_id),
                    p2_peer: None,
                    is_active: true,
                    match_time: 0.0,
                    countdown_timer: 0.0,
                    current_wave: 0,
                    time_until_next_wave: 40.0,
                },
            );

            let initial_entities = spawn_match_entities(
                commands,
                matchmaker,
                room_id,
                GameMode::SoloVsAi,
                peer_id,
                None,
            );

            let (p1_cur_sup, p1_max_sup) = economy.get_supply(Faction::Player1);
            let (ai_cur_sup, ai_max_sup) = economy.get_supply(Faction::HostileAi);

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
                    p1_minerals: economy.get_minerals(Faction::Player1),
                    p1_supply: p1_cur_sup,
                    p1_max_supply: p1_max_sup,
                    p2_minerals: economy.get_minerals(Faction::HostileAi),
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

                matchmaker.rooms.insert(
                    room_id,
                    Room {
                        room_id,
                        room_code: None,
                        mode,
                        p1_peer: Some(waiting_p1),
                        p2_peer: Some(peer_id),
                        is_active: true,
                        match_time: 0.0,
                        countdown_timer: 3.0,
                        current_wave: 0,
                        time_until_next_wave: 40.0,
                    },
                );

                let initial_entities = spawn_match_entities(
                    commands,
                    matchmaker,
                    room_id,
                    GameMode::Multiplayer1v1,
                    waiting_p1,
                    Some(peer_id),
                );

                let (p1_cur_sup, p1_max_sup) = economy.get_supply(Faction::Player1);
                let (p2_cur_sup, p2_max_sup) = economy.get_supply(Faction::Player2);

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
                            p1_minerals: economy.get_minerals(Faction::Player1),
                            p1_supply: p1_cur_sup,
                            p1_max_supply: p1_max_sup,
                            p2_minerals: economy.get_minerals(Faction::Player2),
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

                    let (p1_cur_sup, p1_max_sup) = economy.get_supply(Faction::Player1);
                    let (p2_cur_sup, p2_max_sup) = economy.get_supply(Faction::Player2);

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
                                p1_minerals: economy.get_minerals(Faction::Player1),
                                p1_supply: p1_cur_sup,
                                p1_max_supply: p1_max_sup,
                                p2_minerals: economy.get_minerals(Faction::Player2),
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

                matchmaker.rooms.insert(
                    room_id,
                    Room {
                        room_id,
                        room_code: Some(generated_code.clone()),
                        mode: GameMode::CustomPrivate,
                        p1_peer: Some(peer_id),
                        p2_peer: None,
                        is_active: false,
                        match_time: 0.0,
                        countdown_timer: 0.0,
                        current_wave: 0,
                        time_until_next_wave: 40.0,
                    },
                );

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

/// Server A* navigation grid obstacle updates
fn update_server_nav_grid_system(
    mut nav_grid: ResMut<NavGrid>,
    buildings: Query<(&Transform, &Radius, &Building)>,
    resources: Query<(&Transform, &Radius), With<ResourceNode>>,
) {
    nav_grid.clear();
    shared::map::mark_static_obstacles(&mut nav_grid);
    for (tf, radius, building) in &buildings {
        let pos = tf.translation.truncate();
        let r = if building.name.contains("Base HQ") {
            radius.0 + 8.0
        } else {
            radius.0 + 4.0
        };
        nav_grid.mark_circle(pos, r);
    }
    for (tf, radius) in &resources {
        let pos = tf.translation.truncate();
        nav_grid.mark_circle(pos, radius.0 + 4.0);
    }
}

/// Server ability timers (Stimpack, Siege Mode transitions) and Patrol cycling
fn server_abilities_and_stances_system(
    mut commands: Commands,
    time: Res<Time>,
    nav_grid: Res<NavGrid>,
    mut stim_query: Query<&mut Stimpack>,
    mut tank_query: Query<&mut SiegeTank>,
    mut stance_query: Query<(
        Entity,
        &Transform,
        &mut TacticalStance,
        Option<&MoveTarget>,
        Option<&mut Soldier>,
    ), With<Unit>>,
) {
    let dt = time.delta_secs();

    // 1. Stimpack timers
    for mut stim in &mut stim_query {
        if stim.is_active {
            stim.timer -= dt;
            if stim.timer <= 0.0 {
                stim.is_active = false;
                stim.timer = 0.0;
            }
        }
    }

    // 2. Siege Tank transformations
    for mut tank in &mut tank_query {
        match tank.mode {
            TankMode::TransformingToSiege => {
                tank.transform_timer -= dt;
                if tank.transform_timer <= 0.0 {
                    tank.mode = TankMode::Siege;
                    tank.transform_timer = 0.0;
                    tank.attack_range = 380.0;
                    tank.attack_damage = 70.0;
                    tank.attack_cooldown = 2.2;
                    tank.splash_radius = 45.0;
                }
            }
            TankMode::TransformingToTank => {
                tank.transform_timer -= dt;
                if tank.transform_timer <= 0.0 {
                    tank.mode = TankMode::Tank;
                    tank.transform_timer = 0.0;
                    tank.attack_range = 240.0;
                    tank.attack_damage = 35.0;
                    tank.attack_cooldown = 1.6;
                    tank.splash_radius = 0.0;
                }
            }
            _ => {}
        }
    }

    // 3. Patrol cycle
    for (entity, transform, mut stance, move_target_opt, mut soldier_opt) in &mut stance_query {
        if let TacticalStance::Patrol {
            origin,
            target,
            ref mut heading_to_target,
        } = *stance
        {
            if move_target_opt.is_none() {
                let current_pos = transform.translation.truncate();
                let next_dest = if *heading_to_target {
                    if current_pos.distance(target) <= 24.0 {
                        *heading_to_target = false;
                        origin
                    } else {
                        target
                    }
                } else if current_pos.distance(origin) <= 24.0 {
                    *heading_to_target = true;
                    target
                } else {
                    origin
                };

                let waypoints = nav_grid.find_path(current_pos, next_dest);
                commands.entity(entity).insert(MoveTarget::with_waypoints(
                    next_dest,
                    true,
                    waypoints,
                ));

                if let Some(ref mut soldier) = soldier_opt {
                    soldier.state = SoldierState::AttackMoving;
                }
            }
        }
    }
}

/// Authoritative unit steering and movement along waypoints
fn server_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    matchmaker: Res<Matchmaker>,
    mut query: Query<(
        Entity,
        &mut Transform,
        &MoveSpeed,
        &mut Velocity,
        &mut MoveTarget,
        &RoomId,
        Option<&Stimpack>,
        Option<&SiegeTank>,
        Option<&Soldier>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, speed, mut velocity, mut move_target, room_id, stim_opt, tank_opt, soldier_opt) in &mut query {
        let is_room_active = matchmaker.rooms.get(&room_id.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active {
            velocity.0 = Vec2::ZERO;
            continue;
        }

        if let Some(tank) = tank_opt {
            if tank.mode != TankMode::Tank {
                velocity.0 = Vec2::ZERO;
                continue;
            }
        }

        // If an attack-moving unit is currently fighting/engaging an enemy, pause marching
        if move_target.is_attack_move {
            if let Some(soldier) = soldier_opt {
                if soldier.target.is_some()
                    || soldier.state == SoldierState::Attacking
                    || soldier.state == SoldierState::ChasingTarget
                {
                    velocity.0 = Vec2::ZERO;
                    continue;
                }
            }
            if let Some(tank) = tank_opt {
                if tank.target.is_some() {
                    velocity.0 = Vec2::ZERO;
                    continue;
                }
            }
        }

        let current_pos = transform.translation.truncate();
        let goal_pos = move_target.current_goal();
        let diff = goal_pos - current_pos;
        let dist = diff.length();

        // Track stall / blockage against obstacles or friendly units
        if move_target.last_pos.x.is_nan() {
            move_target.last_pos = current_pos;
            move_target.stall_timer = 0.0;
        } else {
            let moved_dist = current_pos.distance(move_target.last_pos);
            if moved_dist < (speed.0 * 0.15 * dt).max(0.2) {
                move_target.stall_timer += dt;
            } else {
                move_target.stall_timer = 0.0;
                move_target.last_pos = current_pos;
            }
        }

        let is_final_waypoint = move_target.current_waypoint_idx >= (move_target.waypoints.len() - 1);

        // Advance waypoint if close to intermediate
        if !is_final_waypoint && dist <= 20.0 {
            move_target.advance_waypoint();
            move_target.stall_timer = 0.0;
            move_target.last_pos = current_pos;
            continue;
        }

        // Final arrival or stall clean removal
        if is_final_waypoint
            && (dist <= 12.0 || (dist <= 32.0 && move_target.stall_timer > 0.20) || move_target.stall_timer > 0.50) {
                velocity.0 = Vec2::ZERO;
                commands.entity(entity).remove::<MoveTarget>();
                continue;
            }

        let dir = diff.normalize_or_zero();
        let speed_mult = stim_opt
            .map(|s| if s.is_active { 1.5 } else { 1.0 })
            .unwrap_or(1.0);
        velocity.0 = dir * speed.0 * speed_mult;
        transform.translation.x += velocity.0.x * dt;
        transform.translation.y += velocity.0.y * dt;

        let angle = dir.y.atan2(dir.x);
        transform.rotation = Quat::from_rotation_z(angle);
    }
}

struct ServerUnitPosSnapshot {
    entity: Entity,
    pos: Vec2,
    radius: f32,
    faction: Faction,
    room_id: u32,
    is_active_worker: bool,
    is_moving: bool,
}

/// Dedicated server hard circle-circle unit collision and obstacle resolution
fn server_unit_separation_and_collision_system(
    mut unit_query: Query<(Entity, &mut Transform, &Radius, &Faction, &RoomId, Option<&Worker>, Option<&MoveTarget>), With<Unit>>,
    building_query: Query<(&Transform, &Radius, &Faction, &RoomId, Option<&BaseHQ>), (With<Building>, Without<Unit>)>,
    resource_query: Query<(&Transform, &Radius, &RoomId), (With<ResourceNode>, Without<Unit>)>,
) {
    let mut snapshots = Vec::with_capacity(unit_query.iter().len());
    for (entity, transform, radius, faction, room_id, worker_opt, move_opt) in &unit_query {
        let is_active_worker = worker_opt
            .map(|w| w.state != WorkerState::Idle)
            .unwrap_or(false);

        snapshots.push(ServerUnitPosSnapshot {
            entity,
            pos: transform.translation.truncate(),
            radius: radius.0,
            faction: *faction,
            room_id: room_id.0,
            is_active_worker,
            is_moving: move_opt.is_some(),
        });
    }

    // 1. Hard Unit-to-Unit Circle Collision per Room (2 solver iterations)
    for _iter in 0..2 {
        let mut deltas = vec![Vec2::ZERO; snapshots.len()];
        for i in 0..snapshots.len() {
            for j in (i + 1)..snapshots.len() {
                if snapshots[i].room_id != snapshots[j].room_id {
                    continue;
                }

                let u1_active = snapshots[i].is_active_worker;
                let u2_active = snapshots[j].is_active_worker;

                if u1_active && u2_active {
                    continue;
                }

                let p1 = snapshots[i].pos;
                let p2 = snapshots[j].pos;
                let delta = p1 - p2;
                let dist = delta.length();
                let min_dist = snapshots[i].radius + snapshots[j].radius;

                if dist < min_dist {
                    let overlap = min_dist - dist;
                    let dir = if dist > 0.001 {
                        delta / dist
                    } else {
                        let angle = ((snapshots[i].entity.index() + snapshots[j].entity.index()) as f32) * 1.5;
                        Vec2::new(angle.cos(), angle.sin())
                    };

                    let (w1, w2) = match (snapshots[i].is_moving, snapshots[j].is_moving) {
                        (true, false) => (0.75, 0.25),
                        (false, true) => (0.25, 0.75),
                        _ => (0.5, 0.5),
                    };

                    if !u1_active {
                        deltas[i] += dir * (overlap * w1);
                    }
                    if !u2_active {
                        deltas[j] -= dir * (overlap * w2);
                    }
                }
            }
        }
        for k in 0..snapshots.len() {
            snapshots[k].pos += deltas[k];
        }
    }

    // 2. Hard Obstacle Collision (Buildings & Mineral Nodes per Room)
    for snap in &mut snapshots {
        for (b_trans, b_radius, b_faction, b_room, base_hq_opt) in &building_query {
            if b_room.0 != snap.room_id {
                continue;
            }
            if snap.is_active_worker && base_hq_opt.is_some() && b_faction == &snap.faction {
                continue;
            }

            let b_pos = b_trans.translation.truncate();
            let d = snap.pos.distance(b_pos);
            let min_b_dist = snap.radius + b_radius.0;

            if d < min_b_dist {
                let push_dir = if d > 0.001 {
                    (snap.pos - b_pos) / d
                } else {
                    Vec2::new(0.0, 1.0)
                };
                snap.pos = b_pos + push_dir * min_b_dist;
            }
        }

        if !snap.is_active_worker {
            for (r_trans, r_radius, r_room) in &resource_query {
                if r_room.0 != snap.room_id {
                    continue;
                }
                let r_pos = r_trans.translation.truncate();
                let d = snap.pos.distance(r_pos);
                let min_r_dist = snap.radius + r_radius.0;

                if d < min_r_dist {
                    let push_dir = if d > 0.001 {
                        (snap.pos - r_pos) / d
                    } else {
                        Vec2::new(0.0, 1.0)
                    };
                    snap.pos = r_pos + push_dir * min_r_dist;
                }
            }
        }

        // Push away from static map obstacles (rocks, cliff bluffs)
        for obs in shared::map::STATIC_MAP_OBSTACLES {
            let d = snap.pos.distance(obs.position);
            let min_obs_dist = snap.radius + obs.radius;

            if d < min_obs_dist {
                let push_dir = if d > 0.001 {
                    (snap.pos - obs.position) / d
                } else {
                    Vec2::new(0.0, 1.0)
                };
                snap.pos = obs.position + push_dir * min_obs_dist;
            }
        }
    }

    // 3. Write back resolved positions
    for (i, (_entity, mut transform, ..)) in unit_query.iter_mut().enumerate() {
        if i < snapshots.len() {
            transform.translation.x = snapshots[i].pos.x;
            transform.translation.y = snapshots[i].pos.y;
        }
    }
}

/// SCV mining and resource dropoff loop
fn server_mining_system(
    mut commands: Commands,
    time: Res<Time>,
    matchmaker: Res<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    mut workers: Query<(Entity, &mut Transform, &MoveSpeed, &Faction, &RoomId, &mut Worker, Option<&MoveTarget>)>,
    mut nodes: Query<(&Transform, &mut ResourceNode, &NetEntity, &RoomId), Without<Worker>>,
    bases: Query<(&Transform, &Faction, &RoomId), (With<BaseHQ>, Without<Worker>, Without<ResourceNode>)>,
) {
    let dt = time.delta_secs();
    for (worker_e, mut transform, speed, faction, worker_room, mut worker, move_target_opt) in &mut workers {
        let is_room_active = matchmaker.rooms.get(&worker_room.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active {
            continue;
        }

        if worker.state != WorkerState::Idle && move_target_opt.is_some() {
            commands.entity(worker_e).remove::<MoveTarget>();
        }

        match worker.state {
            WorkerState::Idle => {}
            WorkerState::MovingToResource => {
                let Some(node_e) = worker.target_node else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((node_tf, node, _, node_room)) = nodes.get(node_e) {
                    if node_room.0 != worker_room.0 || node.remaining_minerals == 0 {
                        worker.target_node = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    let w_pos = transform.translation.truncate();
                    let n_pos = node_tf.translation.truncate();
                    let dist = w_pos.distance(n_pos);

                    if dist <= worker.interact_distance {
                        worker.state = WorkerState::Mining;
                        worker.harvest_timer = 0.0;
                    } else {
                        let dir = (n_pos - w_pos).normalize_or_zero();
                        transform.translation.x += dir.x * speed.0 * dt;
                        transform.translation.y += dir.y * speed.0 * dt;
                        let angle = dir.y.atan2(dir.x);
                        transform.rotation = Quat::from_rotation_z(angle);
                    }
                } else {
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::Mining => {
                let Some(node_e) = worker.target_node else {
                    worker.state = WorkerState::Idle;
                    continue;
                };

                if let Ok((_, mut node, _, node_room)) = nodes.get_mut(node_e) {
                    if node_room.0 != worker_room.0 || node.remaining_minerals == 0 {
                        worker.target_node = None;
                        worker.state = WorkerState::Idle;
                        continue;
                    }

                    worker.harvest_timer += dt;
                    if worker.harvest_timer >= worker.harvest_duration {
                        let amount = node.remaining_minerals.min(worker.harvest_capacity);
                        node.remaining_minerals -= amount;
                        worker.carried_minerals = amount;
                        worker.state = WorkerState::MovingToBase;
                        worker.harvest_timer = 0.0;
                    }
                } else {
                    worker.target_node = None;
                    worker.state = WorkerState::Idle;
                }
            }
            WorkerState::MovingToBase => {
                let w_pos = transform.translation.truncate();
                let mut best_base = None;
                let mut min_dist = f32::MAX;

                for (base_tf, base_faction, base_room) in &bases {
                    if base_room.0 == worker_room.0 && base_faction == faction {
                        let b_pos = base_tf.translation.truncate();
                        let dist = w_pos.distance(b_pos);
                        if dist < min_dist {
                            min_dist = dist;
                            best_base = Some(b_pos);
                        }
                    }
                }

                if let Some(base_pos) = best_base {
                    if min_dist <= worker.base_interact_distance {
                        economy.add_minerals(*faction, worker.carried_minerals);
                        worker.carried_minerals = 0;
                        if worker.target_node.is_some() {
                            worker.state = WorkerState::MovingToResource;
                        } else {
                            worker.state = WorkerState::Idle;
                        }
                    } else {
                        let dir = (base_pos - w_pos).normalize_or_zero();
                        transform.translation.x += dir.x * speed.0 * dt;
                        transform.translation.y += dir.y * speed.0 * dt;
                        let angle = dir.y.atan2(dir.x);
                        transform.rotation = Quat::from_rotation_z(angle);
                    }
                } else {
                    worker.state = WorkerState::Idle;
                }
            }
        }
    }
}

/// Unit training queues and construction progression
fn server_production_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    mut matchmaker: ResMut<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    nav_grid: Res<NavGrid>,
    mut buildings: Query<(
        Entity,
        &NetEntity,
        &Faction,
        &RoomId,
        &Transform,
        &mut Building,
        Option<&mut ProductionBuilding>,
        Option<&SupplyDepot>,
    )>,
) {
    let dt = time.delta_secs();
    for (_entity, net_entity, faction, room_id, transform, mut building, prod_opt, supply_depot_opt) in
        &mut buildings
    {
        let is_room_active = matchmaker.rooms.get(&room_id.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active {
            continue;
        }
        // 1. Progress under-construction buildings
        if !building.is_constructed {
            building.build_timer += dt;
            if building.build_timer >= building.build_duration {
                building.is_constructed = true;
                if let Some(depot) = supply_depot_opt {
                    economy.add_max_supply(*faction, depot.supply_provided);
                }
            }
        }

        // 2. Production queue
        if building.is_constructed {
            if let Some(mut prod) = prod_opt {
                if !prod.queue.is_empty() {
                    prod.current_timer += dt;
                    let required_duration = prod.queue[0].build_duration;

                    if prod.current_timer >= required_duration {
                        prod.current_timer = 0.0;
                        let finished_unit = prod.queue.remove(0);

                        let unit_kind = if finished_unit.name.contains("SCV") {
                            UnitKind::Worker
                        } else if finished_unit.name.contains("Tank") {
                            UnitKind::Tank
                        } else {
                            UnitKind::Soldier
                        };

                        let net_id = matchmaker.alloc_net_id();
                        let spawn_pos = transform.translation.truncate() + Vec2::new(0.0, -60.0);
                        let rally = prod.rally_point;
                        let waypoints = nav_grid.find_path(spawn_pos, rally);

                        let mut unit_cmds = commands.spawn((
                            Unit {
                                name: finished_unit.name.clone(),
                                supply_cost: finished_unit.supply_cost,
                            },
                            Health::new(unit_kind.max_health()),
                            *faction,
                            *room_id,
                            NetEntity {
                                net_id,
                                owner_peer_id: net_entity.owner_peer_id,
                            },
                            MoveTarget::with_waypoints(rally, false, waypoints),
                            Transform::from_xyz(spawn_pos.x, spawn_pos.y, 2.0),
                        ));

                        match unit_kind {
                            UnitKind::Worker => {
                                unit_cmds.insert((
                                    Worker::default(),
                                    TacticalStance::default(),
                                    Radius(14.0),
                                    MoveSpeed(190.0),
                                    Velocity::default(),
                                ));
                            }
                            UnitKind::Soldier => {
                                unit_cmds.insert((
                                    Soldier {
                                        state: SoldierState::MovingToGround,
                                        attack_range: 150.0,
                                        aggro_radius: 240.0,
                                        attack_damage: 15.0,
                                        attack_cooldown: 0.85,
                                        ..default()
                                    },
                                    Stimpack::default(),
                                    TacticalStance::default(),
                                    Radius(16.0),
                                    MoveSpeed(180.0),
                                    Velocity::default(),
                                ));
                            }
                            UnitKind::Tank => {
                                unit_cmds.insert((
                                    SiegeTank::default(),
                                    TacticalStance::default(),
                                    Radius(22.0),
                                    MoveSpeed(140.0),
                                    Velocity::default(),
                                ));
                            }
                        }


                        let peers = matchmaker.get_room_peers(room_id.0);
                        if !peers.is_empty() {
                            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                peer_ids: peers.clone(),
                                msg: ServerMessage::UnitSpawned {
                                    net_id,
                                    faction: *faction,
                                    unit_kind,
                                    position: spawn_pos,
                                    max_hp: unit_kind.max_health(),
                                },
                            });

                            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                peer_ids: peers,
                                msg: ServerMessage::QueueUpdated {
                                    building_net_id: net_entity.net_id,
                                    queue_count: prod.queue.len(),
                                    current_progress: 0.0,
                                },
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Spawns escalating squads of hostile marines for active SoloVsAi matches per room
fn server_solo_wave_spawner_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    mut matchmaker: ResMut<Matchmaker>,
    nav_grid: Res<NavGrid>,
    base_query: Query<(&Transform, &Faction, &RoomId), With<BaseHQ>>,
) {
    let dt = time.delta_secs();
    let mut waves_to_spawn = Vec::new();

    for (room_id, room) in matchmaker.rooms.iter_mut() {
        if !room.is_active || room.mode != GameMode::SoloVsAi {
            continue;
        }

        room.match_time += dt;

        // Check if Hostile AI Base HQ exists in this room
        let mut ai_base_pos = None;
        let mut p1_base_pos = shared::map::P1_BASE_POS;

        for (tf, faction, b_room) in &base_query {
            if b_room.0 == *room_id {
                if *faction == Faction::HostileAi {
                    ai_base_pos = Some(tf.translation.truncate());
                } else if *faction == Faction::Player1 {
                    p1_base_pos = tf.translation.truncate();
                }
            }
        }

        let Some(base_spawn) = ai_base_pos else {
            // AI HQ destroyed in this room -> no more waves
            continue;
        };

        room.time_until_next_wave -= dt;
        if room.time_until_next_wave <= 0.0 {
            room.current_wave += 1;
            room.time_until_next_wave = 45.0;

            let count = match room.current_wave {
                1 => 3,
                2 => 6,
                3 => 10,
                w => 14 + (w - 4) * 3,
            };

            waves_to_spawn.push((*room_id, room.current_wave, count, base_spawn, p1_base_pos));
        }
    }

    for (room_id, wave_num, count, base_spawn, target_pos) in waves_to_spawn {
        info!(
            "⚔️ [Server WaveAi] Room #{}: Wave {} Incoming! Spawning {} Hostile Marines",
            room_id, wave_num, count
        );

        let peers = matchmaker.get_room_peers(room_id);

        for i in 0..count {
            let angle = (i as f32) * 2.39996;
            let dist = 32.0 * (i as f32).sqrt();
            let offset = Vec2::new(angle.cos(), angle.sin()) * dist;
            let spawn_pos = base_spawn + offset;
            let net_id = matchmaker.alloc_net_id();
            let waypoints = nav_grid.find_path(spawn_pos, target_pos);

            commands.spawn((
                Unit {
                    name: "Hostile Marine".to_string(),
                    supply_cost: 2,
                },
                Soldier {
                    state: SoldierState::AttackMoving,
                    attack_range: 150.0,
                    aggro_radius: 240.0,
                    attack_damage: 14.0,
                    attack_cooldown: 0.9,
                    ..default()
                },
                Stimpack::default(),
                TacticalStance::default(),
                Health::new(120.0),
                Radius(16.0),
                MoveSpeed(175.0),
                Velocity::default(),
                Faction::HostileAi,
                RoomId(room_id),
                NetEntity {
                    net_id,
                    owner_peer_id: 2,
                },
                MoveTarget::with_waypoints(target_pos, true, waypoints),
                Transform::from_xyz(spawn_pos.x, spawn_pos.y, 2.0),
            ));

            if !peers.is_empty() {
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                    peer_ids: peers.clone(),
                    msg: ServerMessage::UnitSpawned {
                        net_id,
                        faction: Faction::HostileAi,
                        unit_kind: UnitKind::Soldier,
                        position: spawn_pos,
                        max_hp: 120.0,
                    },
                });
            }
        }
    }
}

/// Target snapshot used for disjoint query access in server combat
struct ServerTargetSnapshot {
    entity: Entity,
    net_id: u32,
    pos: Vec2,
    radius: f32,
    faction: Faction,
    room_id: u32,
    is_dead: bool,
    supply_cost: u32,
}

/// Military combat, aggro, weapon cooldowns, and damage deduction
fn server_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    matchmaker: Res<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    mut queries: ParamSet<(
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &Transform,
            &Radius,
            &Health,
            Option<&Unit>,
        )>,
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &mut Transform,
            &MoveSpeed,
            &mut Soldier,
            Option<&Stimpack>,
            Option<&mut MoveTarget>,
        )>,
        Query<(Entity, &mut Health)>,
    )>,
) {
    let dt = time.delta_secs();

    // 1. Snapshot all targets
    let targets: Vec<ServerTargetSnapshot> = queries
        .p0()
        .iter()
        .map(|(e, net, fac, room, tf, rad, hp, unit_opt)| ServerTargetSnapshot {
            entity: e,
            net_id: net.net_id,
            pos: tf.translation.truncate(),
            radius: rad.0,
            faction: *fac,
            room_id: room.0,
            is_dead: hp.is_dead(),
            supply_cost: unit_opt.map(|u| u.supply_cost).unwrap_or(0),
        })
        .collect();

    // 2. Iterate through soldiers and execute aggro, chasing, and weapon firing
    let mut damages_to_apply: Vec<(Entity, u32, f32, Faction, u32, u32, u32)> = Vec::new();

    for (s_entity, attacker_net, attacker_faction, attacker_room, mut attacker_tf, move_speed, mut soldier, stim_opt, move_target_opt) in
        &mut queries.p1()
    {
        let is_room_active = matchmaker.rooms.get(&attacker_room.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active {
            continue;
        }

        soldier.attack_timer += dt;
        soldier.scan_timer += dt;
        let attacker_pos = attacker_tf.translation.truncate();

        let is_attack_move = move_target_opt.as_ref().map(|m| m.is_attack_move).unwrap_or(false);

        // If unit has an active pure ground move order, do NOT auto-acquire enemies or stop to attack:
        if move_target_opt.is_some() && !is_attack_move {
            soldier.target = None;
            soldier.state = SoldierState::MovingToGround;
            continue;
        }

        let target_valid = soldier.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead && t.room_id == attacker_room.0 && attacker_faction.is_hostile_to(&t.faction))
        });

        if let Some(target_snap) = target_valid {
            let target_pos = target_snap.pos;
            let dist = target_pos.distance(attacker_pos);
            let effective_range = soldier.attack_range + target_snap.radius;
            let dir = (target_pos - attacker_pos).normalize_or_zero();

            if dir.length_squared() > 0.001 {
                let angle = dir.y.atan2(dir.x);
                attacker_tf.rotation = Quat::from_rotation_z(angle);
            }

            if dist <= effective_range {
                soldier.state = SoldierState::Attacking;

                if soldier.attack_timer >= soldier.attack_cooldown {
                    soldier.attack_timer = 0.0;
                    damages_to_apply.push((
                        target_snap.entity,
                        target_snap.net_id,
                        soldier.attack_damage,
                        *attacker_faction,
                        attacker_room.0,
                        attacker_net.net_id,
                        target_snap.supply_cost,
                    ));

                    let peers = matchmaker.get_room_peers(attacker_room.0);
                    if !peers.is_empty() {
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                            peer_ids: peers,
                            msg: ServerMessage::ProjectileFired {
                                attacker_net_id: attacker_net.net_id,
                                target_net_id: target_snap.net_id,
                                origin: attacker_pos + dir * 18.0,
                                target_pos,
                                damage: soldier.attack_damage,
                            },
                        });
                    }
                }
            } else if soldier.state != SoldierState::HoldingPosition {
                soldier.state = SoldierState::ChasingTarget;
                let speed_mult = stim_opt
                    .map(|s| if s.is_active { 1.5 } else { 1.0 })
                    .unwrap_or(1.0);
                let stop_dist = (effective_range * 0.90).max(10.0);
                let travel_needed = (dist - stop_dist).max(0.0);
                let step = dir * (move_speed.0 * speed_mult * dt).min(travel_needed);
                attacker_tf.translation.x += step.x;
                attacker_tf.translation.y += step.y;
            }
        } else {
            // Target dead or none: scan for enemies in room
            soldier.target = None;

            let max_scan_range = if is_attack_move {
                soldier.aggro_radius
            } else {
                soldier.attack_range
            };

            let mut closest = None;
            let mut min_d = max_scan_range;
            for t in &targets {
                if t.entity != s_entity && t.room_id == attacker_room.0 && attacker_faction.is_hostile_to(&t.faction) && !t.is_dead {
                    let d = t.pos.distance(attacker_pos);
                    let effective_range = max_scan_range + t.radius;
                    if d <= effective_range && d < min_d {
                        min_d = d;
                        closest = Some(t);
                    }
                }
            }
            if let Some(t) = closest {
                soldier.target = Some(t.entity);
                let dist = attacker_pos.distance(t.pos);
                let effective_range = soldier.attack_range + t.radius;
                let dir = (t.pos - attacker_pos).normalize_or_zero();

                if dir.length_squared() > 0.001 {
                    let angle = dir.y.atan2(dir.x);
                    attacker_tf.rotation = Quat::from_rotation_z(angle);
                }

                if dist <= effective_range {
                    soldier.state = SoldierState::Attacking;
                    if soldier.attack_timer >= soldier.attack_cooldown {
                        soldier.attack_timer = 0.0;
                        damages_to_apply.push((
                            t.entity,
                            t.net_id,
                            soldier.attack_damage,
                            *attacker_faction,
                            attacker_room.0,
                            attacker_net.net_id,
                            t.supply_cost,
                        ));

                        let peers = matchmaker.get_room_peers(attacker_room.0);
                        if !peers.is_empty() {
                            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                peer_ids: peers,
                                msg: ServerMessage::ProjectileFired {
                                    attacker_net_id: attacker_net.net_id,
                                    target_net_id: t.net_id,
                                    origin: attacker_pos + dir * 18.0,
                                    target_pos: t.pos,
                                    damage: soldier.attack_damage,
                                },
                            });
                        }
                    }
                } else if soldier.state != SoldierState::HoldingPosition {
                    soldier.state = SoldierState::ChasingTarget;
                    let speed_mult = stim_opt
                        .map(|s| if s.is_active { 1.5 } else { 1.0 })
                        .unwrap_or(1.0);
                    let stop_dist = (effective_range * 0.90).max(10.0);
                    let travel_needed = (dist - stop_dist).max(0.0);
                    let step = dir * (move_speed.0 * speed_mult * dt).min(travel_needed);
                    attacker_tf.translation.x += step.x;
                    attacker_tf.translation.y += step.y;
                }
            } else if is_attack_move {
                soldier.state = SoldierState::AttackMoving;
            } else if soldier.state != SoldierState::HoldingPosition {
                soldier.state = SoldierState::Idle;
            }
        }
    }

    // 3. Apply damages and handle deaths
    let mut health_query = queries.p2();
    for (target_e, target_net, dmg, attacker_fac, attacker_room_id, _attacker_net, supply_cost) in damages_to_apply {
        if let Ok((_, mut hp)) = health_query.get_mut(target_e) {
            hp.take_damage(dmg);

            let peers = matchmaker.get_room_peers(attacker_room_id);
            if !peers.is_empty() {
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                    peer_ids: peers.clone(),
                    msg: ServerMessage::EntityDamaged {
                        target_net_id: target_net,
                        current_hp: hp.current,
                        max_hp: hp.max,
                    },
                });

                if hp.is_dead() {
                    if supply_cost > 0 {
                        let victim_faction = if attacker_fac == Faction::Player1 {
                            Faction::Player2
                        } else {
                            Faction::Player1
                        };
                        economy.unregister_supply(victim_faction, supply_cost);
                    }

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                        peer_ids: peers,
                        msg: ServerMessage::EntityDied {
                            net_id: target_net,
                            faction: attacker_fac,
                        },
                    });

                    commands.entity(target_e).despawn_recursive();
                }
            }
        }
    }
}

/// Dedicated Server Defensive Gun Turret Combat System
fn server_turret_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    matchmaker: Res<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    mut queries: ParamSet<(
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &Transform,
            &Radius,
            &Health,
            Option<&Unit>,
        )>,
        Query<(Entity, &NetEntity, &Faction, &RoomId, &Transform, &Building, &mut GunTurret)>,
        Query<(Entity, &mut Health)>,
    )>,
) {
    let dt = time.delta_secs();

    let targets: Vec<ServerTargetSnapshot> = queries
        .p0()
        .iter()
        .map(|(e, net, fac, room, tf, rad, hp, unit_opt)| ServerTargetSnapshot {
            entity: e,
            net_id: net.net_id,
            pos: tf.translation.truncate(),
            radius: rad.0,
            faction: *fac,
            room_id: room.0,
            is_dead: hp.is_dead(),
            supply_cost: unit_opt.map(|u| u.supply_cost).unwrap_or(0),
        })
        .collect();

    let mut damages_to_apply: Vec<(Entity, u32, f32, Faction, u32, u32, u32)> = Vec::new();

    for (_t_entity, turret_net, turret_faction, turret_room, turret_tf, building, mut turret) in
        &mut queries.p1()
    {
        let is_room_active = matchmaker.rooms.get(&turret_room.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active || !building.is_constructed {
            continue;
        }

        turret.attack_timer += dt;
        let turret_pos = turret_tf.translation.truncate();

        let target_valid = turret.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead && t.room_id == turret_room.0 && turret_faction.is_hostile_to(&t.faction) && t.pos.distance(turret_pos) <= (turret.attack_range + t.radius))
        });

        let active_target = match target_valid {
            Some(t) => Some(t),
            None => {
                turret.target = None;
                let mut closest = None;
                let mut min_d = turret.attack_range;
                for t in &targets {
                    if t.room_id == turret_room.0 && turret_faction.is_hostile_to(&t.faction) && !t.is_dead {
                        let d = t.pos.distance(turret_pos);
                        let effective_range = turret.attack_range + t.radius;
                        if d <= effective_range && d < min_d {
                            min_d = d;
                            closest = Some(t);
                        }
                    }
                }
                if let Some(t) = closest {
                    turret.target = Some(t.entity);
                }
                closest
            }
        };

        if let Some(target_snap) = active_target {
            let target_pos = target_snap.pos;
            let dir = (target_pos - turret_pos).normalize_or_zero();
            turret.barrel_angle = dir.y.atan2(dir.x);

            if turret.attack_timer >= turret.attack_cooldown {
                turret.attack_timer = 0.0;
                damages_to_apply.push((
                    target_snap.entity,
                    target_snap.net_id,
                    turret.attack_damage,
                    *turret_faction,
                    turret_room.0,
                    turret_net.net_id,
                    target_snap.supply_cost,
                ));

                let peers = matchmaker.get_room_peers(turret_room.0);
                if !peers.is_empty() {
                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                        peer_ids: peers,
                        msg: ServerMessage::ProjectileFired {
                            attacker_net_id: turret_net.net_id,
                            target_net_id: target_snap.net_id,
                            origin: turret_pos + dir * 26.0,
                            target_pos,
                            damage: turret.attack_damage,
                        },
                    });
                }
            }
        }
    }

    let mut health_query = queries.p2();
    for (target_e, target_net, dmg, attacker_fac, attacker_room_id, _attacker_net, supply_cost) in damages_to_apply {
        if let Ok((_, mut hp)) = health_query.get_mut(target_e) {
            hp.take_damage(dmg);

            let peers = matchmaker.get_room_peers(attacker_room_id);
            if !peers.is_empty() {
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                    peer_ids: peers.clone(),
                    msg: ServerMessage::EntityDamaged {
                        target_net_id: target_net,
                        current_hp: hp.current,
                        max_hp: hp.max,
                    },
                });

                if hp.is_dead() {
                    if supply_cost > 0 {
                        let victim_faction = if attacker_fac == Faction::Player1 {
                            Faction::Player2
                        } else {
                            Faction::Player1
                        };
                        economy.unregister_supply(victim_faction, supply_cost);
                    }

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                        peer_ids: peers,
                        msg: ServerMessage::EntityDied {
                            net_id: target_net,
                            faction: attacker_fac,
                        },
                    });

                    commands.entity(target_e).despawn_recursive();
                }
            }
        }
    }
}

/// Dedicated Server Siege Tank Combat & Artillery System
fn server_siege_tank_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    matchmaker: Res<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    mut queries: ParamSet<(
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &Transform,
            &Radius,
            &Health,
            Option<&Unit>,
        )>,
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &mut Transform,
            &MoveSpeed,
            &mut SiegeTank,
            Option<&mut MoveTarget>,
        )>,
        Query<(Entity, &mut Health)>,
    )>,
) {
    let dt = time.delta_secs();

    let targets: Vec<ServerTargetSnapshot> = queries
        .p0()
        .iter()
        .map(|(e, net, fac, room, tf, rad, hp, unit_opt)| ServerTargetSnapshot {
            entity: e,
            net_id: net.net_id,
            pos: tf.translation.truncate(),
            radius: rad.0,
            faction: *fac,
            room_id: room.0,
            is_dead: hp.is_dead(),
            supply_cost: unit_opt.map(|u| u.supply_cost).unwrap_or(0),
        })
        .collect();

    let mut damages_to_apply = Vec::new();

    for (tank_ent, tank_net, tank_faction, tank_room, mut tank_tf, move_speed, mut tank, move_target_opt) in
        &mut queries.p1()
    {
        let is_room_active = matchmaker.rooms.get(&tank_room.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active {
            continue;
        }

        tank.attack_timer += dt;
        let tank_pos = tank_tf.translation.truncate();
        let is_siege = tank.mode == TankMode::Siege;
        let is_attack_move = move_target_opt.as_ref().map(|m| m.is_attack_move).unwrap_or(false);

        // If tank is in mobile mode and has a pure ground move order, ignore combat and move!
        if move_target_opt.is_some() && !is_attack_move && tank.mode == TankMode::Tank {
            tank.target = None;
            continue;
        }

        let target_valid = tank.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead && t.room_id == tank_room.0 && tank_faction.is_hostile_to(&t.faction))
        });

        if let Some(target_snap) = target_valid {
            let target_pos = target_snap.pos;
            let dist = target_pos.distance(tank_pos);
            let effective_range = tank.attack_range + target_snap.radius;
            let dir = (target_pos - tank_pos).normalize_or_zero();
            tank.turret_angle = dir.y.atan2(dir.x);

            if dist <= effective_range {
                if tank.attack_timer >= tank.attack_cooldown {
                    tank.attack_timer = 0.0;
                    damages_to_apply.push((
                        target_snap.entity,
                        target_snap.net_id,
                        tank.attack_damage,
                        *tank_faction,
                        tank_room.0,
                        tank_net.net_id,
                        target_snap.supply_cost,
                    ));

                    let peers = matchmaker.get_room_peers(tank_room.0);
                    if !peers.is_empty() {
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                            peer_ids: peers,
                            msg: ServerMessage::ProjectileFired {
                                attacker_net_id: tank_net.net_id,
                                target_net_id: target_snap.net_id,
                                origin: tank_pos + dir * (if is_siege { 36.0 } else { 26.0 }),
                                target_pos,
                                damage: tank.attack_damage,
                            },
                        });
                    }
                }
            } else if tank.mode == TankMode::Tank {
                let stop_dist = (effective_range * 0.90).max(20.0);
                let travel_needed = (dist - stop_dist).max(0.0);
                let step = dir * (move_speed.0 * dt).min(travel_needed);
                tank_tf.translation.x += step.x;
                tank_tf.translation.y += step.y;
                let angle = dir.y.atan2(dir.x);
                tank_tf.rotation = Quat::from_rotation_z(angle);
            } else if is_siege {
                tank.target = None;
            }
        } else {
            // Target dead or none: scan for enemies in room
            tank.target = None;

            let max_scan_range = if is_attack_move {
                (tank.attack_range * 1.25).max(300.0)
            } else {
                tank.attack_range
            };

            let mut closest = None;
            let mut min_d = max_scan_range;
            for t in &targets {
                if t.entity != tank_ent && t.room_id == tank_room.0 && tank_faction.is_hostile_to(&t.faction) && !t.is_dead {
                    let d = t.pos.distance(tank_pos);
                    let effective_range = max_scan_range + t.radius;
                    if d <= effective_range && d < min_d {
                        min_d = d;
                        closest = Some(t);
                    }
                }
            }
            if let Some(t) = closest {
                tank.target = Some(t.entity);
                let dir = (t.pos - tank_pos).normalize_or_zero();
                tank.turret_angle = dir.y.atan2(dir.x);

                if tank.attack_timer >= tank.attack_cooldown {
                    tank.attack_timer = 0.0;
                    damages_to_apply.push((
                        t.entity,
                        t.net_id,
                        tank.attack_damage,
                        *tank_faction,
                        tank_room.0,
                        tank_net.net_id,
                        t.supply_cost,
                    ));

                    let peers = matchmaker.get_room_peers(tank_room.0);
                    if !peers.is_empty() {
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                            peer_ids: peers,
                            msg: ServerMessage::ProjectileFired {
                                attacker_net_id: tank_net.net_id,
                                target_net_id: t.net_id,
                                origin: tank_pos + dir * (if is_siege { 36.0 } else { 26.0 }),
                                target_pos: t.pos,
                                damage: tank.attack_damage,
                            },
                        });
                    }
                }
            }
        }
    }

    let mut health_query = queries.p2();
    for (target_e, target_net, dmg, attacker_fac, attacker_room_id, _attacker_net, supply_cost) in damages_to_apply {
        if let Ok((_, mut hp)) = health_query.get_mut(target_e) {
            hp.take_damage(dmg);

            let peers = matchmaker.get_room_peers(attacker_room_id);
            if !peers.is_empty() {
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                    peer_ids: peers.clone(),
                    msg: ServerMessage::EntityDamaged {
                        target_net_id: target_net,
                        current_hp: hp.current,
                        max_hp: hp.max,
                    },
                });

                if hp.is_dead() {
                    if supply_cost > 0 {
                        let victim_faction = if attacker_fac == Faction::Player1 {
                            Faction::Player2
                        } else {
                            Faction::Player1
                        };
                        economy.unregister_supply(victim_faction, supply_cost);
                    }

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                        peer_ids: peers,
                        msg: ServerMessage::EntityDied {
                            net_id: target_net,
                            faction: attacker_fac,
                        },
                    });

                    commands.entity(target_e).despawn_recursive();
                }
            }
        }
    }
}

/// Checks for Victory or Defeat per room when a Base HQ is destroyed
fn server_match_outcome_system(
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

/// 30 Hz position, rotation, and health snapshot broadcast partitioned per active room
fn server_tick_snapshot_system(
    time: Res<Time>,
    mut tick_timer: ResMut<ServerTickTimer>,
    net_channels: Res<ServerNetworkChannels>,
    matchmaker: Res<Matchmaker>,
    economy: Res<PlayerEconomy>,
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

            let (p1_cur_sup, p1_max_sup) = economy.get_supply(Faction::Player1);
            let p2_faction = if room.mode == GameMode::SoloVsAi {
                Faction::HostileAi
            } else {
                Faction::Player2
            };
            let (p2_cur_sup, p2_max_sup) = economy.get_supply(p2_faction);

            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                peer_ids: peers,
                msg: ServerMessage::TickSnapshotBatch {
                    tick: *tick_counter,
                    snapshots: room_snapshots,
                    p1_minerals: economy.get_minerals(Faction::Player1),
                    p1_supply: p1_cur_sup,
                    p1_max_supply: p1_max_sup,
                    p2_minerals: economy.get_minerals(p2_faction),
                    p2_supply: p2_cur_sup,
                    p2_max_supply: p2_max_sup,
                    next_wave_seconds: room.time_until_next_wave,
                    current_wave: room.current_wave,
                },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combat_system_strictly_isolates_rooms() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let (_tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, _rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        app.insert_resource(PlayerEconomy::new());
        app.insert_resource(Matchmaker::new());
        app.add_systems(Update, server_combat_system);

        let world = app.world_mut();

        // Room 1: P1 Marine at (0, 0) and Room 1 Enemy Marine at (50, 0)
        let r1_p1 = world.spawn((
            Unit { name: "P1 Marine".to_string(), supply_cost: 2 },
            Soldier {
                state: SoldierState::Idle,
                attack_range: 150.0,
                aggro_radius: 240.0,
                attack_damage: 15.0,
                attack_cooldown: 0.85,
                ..default()
            },
            Health::new(120.0),
            Radius(16.0),
            MoveSpeed(180.0),
            Faction::Player1,
            RoomId(1),
            NetEntity { net_id: 101, owner_peer_id: 1 },
            Transform::from_xyz(0.0, 0.0, 2.0),
        )).id();

        let r1_enemy = world.spawn((
            Unit { name: "P2 Marine".to_string(), supply_cost: 2 },
            Soldier {
                state: SoldierState::Idle,
                attack_range: 150.0,
                aggro_radius: 240.0,
                attack_damage: 15.0,
                attack_cooldown: 0.85,
                ..default()
            },
            Health::new(120.0),
            Radius(16.0),
            MoveSpeed(180.0),
            Faction::Player2,
            RoomId(1),
            NetEntity { net_id: 102, owner_peer_id: 2 },
            Transform::from_xyz(50.0, 0.0, 2.0),
        )).id();

        // Room 2: P1 Marine at (0, 0) and Room 2 Enemy Marine at (50, 0) (Identical coords!)
        let r2_p1 = world.spawn((
            Unit { name: "P1 Marine".to_string(), supply_cost: 2 },
            Soldier {
                state: SoldierState::Idle,
                attack_range: 150.0,
                aggro_radius: 240.0,
                attack_damage: 15.0,
                attack_cooldown: 0.85,
                ..default()
            },
            Health::new(120.0),
            Radius(16.0),
            MoveSpeed(180.0),
            Faction::Player1,
            RoomId(2),
            NetEntity { net_id: 201, owner_peer_id: 3 },
            Transform::from_xyz(0.0, 0.0, 2.0),
        )).id();

        let r2_enemy = world.spawn((
            Unit { name: "Hostile Marine".to_string(), supply_cost: 2 },
            Soldier {
                state: SoldierState::Idle,
                attack_range: 150.0,
                aggro_radius: 240.0,
                attack_damage: 15.0,
                attack_cooldown: 0.85,
                ..default()
            },
            Health::new(120.0),
            Radius(16.0),
            MoveSpeed(180.0),
            Faction::HostileAi,
            RoomId(2),
            NetEntity { net_id: 202, owner_peer_id: 0 },
            Transform::from_xyz(50.0, 0.0, 2.0),
        )).id();

        // Run simulation update
        app.update();

        // Assert that Room 1 P1 Soldier acquired Room 1 Enemy, and NOT Room 2 Enemy
        let s_r1 = app.world().get::<Soldier>(r1_p1).unwrap();
        assert_eq!(s_r1.target, Some(r1_enemy), "Room 1 P1 must target Room 1 Enemy");

        let s_r2 = app.world().get::<Soldier>(r2_p1).unwrap();
        assert_eq!(s_r2.target, Some(r2_enemy), "Room 2 P1 must target Room 2 Enemy");

        let s_r1_e = app.world().get::<Soldier>(r1_enemy).unwrap();
        assert_eq!(s_r1_e.target, Some(r1_p1), "Room 1 Enemy must target Room 1 P1");

        let s_r2_e = app.world().get::<Soldier>(r2_enemy).unwrap();
        assert_eq!(s_r2_e.target, Some(r2_p1), "Room 2 Enemy must target Room 2 P1");
    }

    #[test]
    fn test_match_outcome_per_room_independence() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let (_tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, _rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });

        let mut matchmaker = Matchmaker::new();
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::Multiplayer1v1,
                p1_peer: Some(101),
                p2_peer: Some(102),
                is_active: true,
                match_time: 25.0,
                countdown_timer: 0.0,
                current_wave: 0,
                time_until_next_wave: 40.0,
            },
        );
        matchmaker.rooms.insert(
            2,
            Room {
                room_id: 2,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(201),
                p2_peer: None,
                is_active: true,
                match_time: 50.0,
                countdown_timer: 0.0,
                current_wave: 2,
                time_until_next_wave: 20.0,
            },
        );
        app.insert_resource(matchmaker);
        app.add_systems(Update, server_match_outcome_system);

        let world = app.world_mut();

        // Room 1: Both P1 HQ and P2 HQ exist (Match In Progress)
        world.spawn((
            BaseHQ::default(),
            Faction::Player1,
            RoomId(1),
            Health::new(1500.0),
        ));
        world.spawn((
            BaseHQ::default(),
            Faction::Player2,
            RoomId(1),
            Health::new(1500.0),
        ));

        // Room 2: Only P1 HQ exists (AI HQ was destroyed -> P1 Victory in Room 2!)
        world.spawn((
            BaseHQ::default(),
            Faction::Player1,
            RoomId(2),
            Health::new(1500.0),
        ));

        // Run match outcome evaluation
        app.update();

        let mm = app.world().resource::<Matchmaker>();
        let r1 = mm.rooms.get(&1).unwrap();
        let r2 = mm.rooms.get(&2).unwrap();

        assert!(r1.is_active, "Room 1 should still be active (both HQs alive)");
        assert!(!r2.is_active, "Room 2 should be marked inactive (AI HQ destroyed)");
    }

    #[test]
    fn test_disconnect_cleans_up_room_entities_and_telemetry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let (tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, _rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        app.insert_resource(PlayerEconomy::new());
        app.insert_resource(NavGrid::default());

        let mut matchmaker = Matchmaker::new();
        matchmaker.players.insert(
            101,
            PlayerSession {
                peer_id: 101,
                name: "Commander".to_string(),
                room_id: 1,
                faction: Faction::Player1,
                color: FactionColor::Blue,
            },
        );
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(101),
                p2_peer: None,
                is_active: true,
                match_time: 10.0,
                countdown_timer: 0.0,
                current_wave: 1,
                time_until_next_wave: 30.0,
            },
        );
        app.insert_resource(matchmaker);
        app.add_systems(Update, handle_incoming_network_events);

        let world = app.world_mut();

        // Spawn entities in Room 1
        let e1 = world.spawn((RoomId(1), Unit { name: "Marine 1".to_string(), supply_cost: 2 }, NetEntity { net_id: 10, owner_peer_id: 101 }, Transform::default(), Faction::Player1)).id();
        let e2 = world.spawn((RoomId(1), BaseHQ::default(), NetEntity { net_id: 11, owner_peer_id: 101 }, Transform::default(), Faction::Player1)).id();

        // Spawn entity in Room 2 (different match)
        let e_other = world.spawn((RoomId(2), BaseHQ::default(), NetEntity { net_id: 20, owner_peer_id: 201 }, Transform::default(), Faction::Player1)).id();

        // Send disconnect event for peer 101
        tx_in.send(IncomingNetEvent::PeerDisconnected { peer_id: 101 }).unwrap();

        // Process event
        app.update();

        // Room 1 entities should be despawned
        assert!(app.world().get_entity(e1).is_err(), "Room 1 unit must be despawned on disconnect");
        assert!(app.world().get_entity(e2).is_err(), "Room 1 HQ must be despawned on disconnect");

        // Room 2 entity must still exist untouched
        assert!(app.world().get_entity(e_other).is_ok(), "Room 2 HQ must remain alive");

        let mm = app.world().resource::<Matchmaker>();
        assert!(!mm.rooms.contains_key(&1), "Room 1 should be removed from matchmaker");
        assert!(!mm.players.contains_key(&101), "Player 101 should be removed");
    }

    #[test]
    fn test_forfeit_cleans_up_room_entities_and_telemetry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let (tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, mut rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        app.insert_resource(PlayerEconomy::new());
        app.insert_resource(NavGrid::default());

        let mut matchmaker = Matchmaker::new();
        matchmaker.players.insert(
            101,
            PlayerSession {
                peer_id: 101,
                name: "SoloCommander".to_string(),
                room_id: 1,
                faction: Faction::Player1,
                color: FactionColor::Blue,
            },
        );
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(101),
                p2_peer: None,
                is_active: true,
                match_time: 5.0,
                countdown_timer: 0.0,
                current_wave: 1,
                time_until_next_wave: 30.0,
            },
        );
        app.insert_resource(matchmaker);
        app.add_systems(Update, handle_incoming_network_events);

        let world = app.world_mut();
        let e1 = world.spawn((RoomId(1), Unit { name: "Marine".to_string(), supply_cost: 2 }, NetEntity { net_id: 10, owner_peer_id: 101 }, Transform::default(), Faction::Player1)).id();

        tx_in.send(IncomingNetEvent::MessageReceived {
            peer_id: 101,
            msg: shared::protocol::ClientMessage::ForfeitMatch,
        }).unwrap();

        app.update();

        assert!(app.world().get_entity(e1).is_err(), "Room 1 unit must be despawned on forfeit");

        let mm = app.world().resource::<Matchmaker>();
        assert!(!mm.rooms.contains_key(&1), "Room 1 should be removed from matchmaker on forfeit");
        assert_eq!(mm.active_solo_count(), 0, "Active solo matches must drop to 0");

        let mut found_lobby_stats = false;
        while let Ok(event) = rx_out.try_recv() {
            if let OutgoingNetEvent::Broadcast { msg: ServerMessage::LobbyStats { active_solo_matches, .. } } = event {
                assert_eq!(active_solo_matches, 0);
                found_lobby_stats = true;
            }
        }
        assert!(found_lobby_stats, "LobbyStats broadcast must be emitted on forfeit");
    }

    #[test]
    fn test_tick_snapshots_are_partitioned_per_room() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let (_tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, mut rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        app.insert_resource(PlayerEconomy::new());

        let mut matchmaker = Matchmaker::new();
        matchmaker.players.insert(
            101,
            PlayerSession {
                peer_id: 101,
                name: "Player 1".to_string(),
                room_id: 1,
                faction: Faction::Player1,
                color: FactionColor::Blue,
            },
        );
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(101),
                p2_peer: None,
                is_active: true,
                match_time: 5.0,
                countdown_timer: 0.0,
                current_wave: 1,
                time_until_next_wave: 30.0,
            },
        );

        matchmaker.players.insert(
            201,
            PlayerSession {
                peer_id: 201,
                name: "Player 2".to_string(),
                room_id: 2,
                faction: Faction::Player1,
                color: FactionColor::Teal,
            },
        );
        matchmaker.rooms.insert(
            2,
            Room {
                room_id: 2,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(201),
                p2_peer: None,
                is_active: true,
                match_time: 15.0,
                countdown_timer: 0.0,
                current_wave: 3,
                time_until_next_wave: 10.0,
            },
        );
        app.insert_resource(matchmaker);
        // Add a tick timer with 0s duration to trigger immediately
        app.insert_resource(ServerTickTimer(Timer::from_seconds(0.0, TimerMode::Repeating)));
        app.add_systems(Update, server_tick_snapshot_system);

        let world = app.world_mut();

        // Spawn entity in Room 1
        world.spawn((
            RoomId(1),
            NetEntity { net_id: 1001, owner_peer_id: 101 },
            Transform::from_xyz(100.0, 100.0, 1.0),
            Health::new(100.0),
        ));

        // Spawn entity in Room 2
        world.spawn((
            RoomId(2),
            NetEntity { net_id: 2001, owner_peer_id: 201 },
            Transform::from_xyz(-200.0, -200.0, 1.0),
            Health::new(200.0),
        ));

        // Tick simulation
        app.update();

        // Verify sent outgoing network events
        let mut events = Vec::new();
        while let Ok(ev) = rx_out.try_recv() {
            events.push(ev);
        }

        assert_eq!(events.len(), 2, "Must send exactly one snapshot batch per active room");

        for ev in events {
            match ev {
                OutgoingNetEvent::BroadcastToPeers { peer_ids, msg } => {
                    match msg {
                        ServerMessage::TickSnapshotBatch { snapshots, .. } => {
                            if peer_ids.contains(&101) {
                                assert_eq!(peer_ids, vec![101]);
                                assert_eq!(snapshots.len(), 1);
                                assert_eq!(snapshots[0].net_id, 1001, "Room 1 snapshot must contain entity 1001");
                            } else if peer_ids.contains(&201) {
                                assert_eq!(peer_ids, vec![201]);
                                assert_eq!(snapshots.len(), 1);
                                assert_eq!(snapshots[0].net_id, 2001, "Room 2 snapshot must contain entity 2001");
                            } else {
                                panic!("Unexpected peer recipient: {:?}", peer_ids);
                            }
                        }
                        _ => panic!("Expected TickSnapshotBatch message"),
                    }
                }
                _ => panic!("Expected BroadcastToPeers event"),
            }
        }
    }

    #[test]
    fn test_custom_private_room_matching_by_code() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let (tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, mut rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        app.insert_resource(PlayerEconomy::new());
        app.insert_resource(NavGrid::default());
        app.insert_resource(Matchmaker::new());
        app.add_systems(Update, handle_incoming_network_events);

        // 1. Peer 101 creates a private room
        tx_in.send(IncomingNetEvent::MessageReceived {
            peer_id: 101,
            msg: ClientMessage::JoinLobby {
                player_name: "Alice".to_string(),
                mode: GameMode::CustomPrivate,
                room_code: None,
                faction_color: Some(FactionColor::Teal),
            },
        }).unwrap();

        app.update();

        // Extract the generated 4-digit code
        let mut generated_code = String::new();
        while let Ok(ev) = rx_out.try_recv() {
            if let OutgoingNetEvent::SendToPeer {
                msg: ServerMessage::LobbyJoined { room_code: Some(code), is_game_ready, .. },
                ..
            } = ev {
                assert!(!is_game_ready, "Room should wait for opponent");
                generated_code = code;
            }
        }
        assert_eq!(generated_code.len(), 4, "Room code must be 4 characters");

        // 2. Peer 102 joins with the generated code
        tx_in.send(IncomingNetEvent::MessageReceived {
            peer_id: 102,
            msg: ClientMessage::JoinLobby {
                player_name: "Bob".to_string(),
                mode: GameMode::CustomPrivate,
                room_code: Some(generated_code.clone()),
                faction_color: Some(FactionColor::Red),
            },
        }).unwrap();

        app.update();

        // Verify match started for both peers
        let mut started_peers = Vec::new();
        while let Ok(ev) = rx_out.try_recv() {
            if let OutgoingNetEvent::SendToPeer {
                peer_id,
                msg: ServerMessage::GameStarted { .. },
            } = ev {
                started_peers.push(peer_id);
            }
        }
        assert_eq!(started_peers, vec![101, 102], "Both players must receive GameStarted");

        let mm = app.world().resource::<Matchmaker>();
        let room = mm.rooms.values().find(|r| r.room_code.as_deref() == Some(&generated_code)).unwrap();
        assert!(room.is_active);
        assert_eq!(room.p1_peer, Some(101));
        assert_eq!(room.p2_peer, Some(102));
    }

    #[test]
    fn test_chat_and_ping_dispatching() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let (tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, mut rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        app.insert_resource(PlayerEconomy::new());
        app.insert_resource(NavGrid::default());

        let mut matchmaker = Matchmaker::new();
        matchmaker.players.insert(
            101,
            PlayerSession {
                peer_id: 101,
                name: "Alice".to_string(),
                room_id: 1,
                faction: Faction::Player1,
                color: FactionColor::Blue,
            },
        );
        matchmaker.players.insert(
            102,
            PlayerSession {
                peer_id: 102,
                name: "Bob".to_string(),
                room_id: 1,
                faction: Faction::Player2,
                color: FactionColor::Red,
            },
        );
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::Multiplayer1v1,
                p1_peer: Some(101),
                p2_peer: Some(102),
                is_active: true,
                match_time: 10.0,
                countdown_timer: 0.0,
                current_wave: 0,
                time_until_next_wave: 40.0,
            },
        );
        app.insert_resource(matchmaker);
        app.add_systems(Update, handle_incoming_network_events);

        // Alice sends chat message
        tx_in.send(IncomingNetEvent::MessageReceived {
            peer_id: 101,
            msg: ClientMessage::SendChatMessage { text: "GL HF!".to_string() },
        }).unwrap();

        // Bob sends tactical ping
        tx_in.send(IncomingNetEvent::MessageReceived {
            peer_id: 102,
            msg: ClientMessage::SendTacticalPing {
                position: Vec2::new(100.0, 200.0),
                ping_type: PingType::Attack,
            },
        }).unwrap();

        app.update();

        let mut received_chat = false;
        let mut received_ping = false;

        while let Ok(ev) = rx_out.try_recv() {
            if let OutgoingNetEvent::BroadcastToPeers { peer_ids, msg } = ev {
                assert_eq!(peer_ids, vec![101, 102]);
                match msg {
                    ServerMessage::ChatMessageReceived { sender_name, text, .. } => {
                        assert_eq!(sender_name, "Alice");
                        assert_eq!(text, "GL HF!");
                        received_chat = true;
                    }
                    ServerMessage::TacticalPingReceived { sender_name, position, ping_type, .. } => {
                        assert_eq!(sender_name, "Bob");
                        assert_eq!(position, Vec2::new(100.0, 200.0));
                        assert_eq!(ping_type, PingType::Attack);
                        received_ping = true;
                    }
                    _ => {}
                }
            }
        }

        assert!(received_chat, "Must broadcast ChatMessageReceived to room peers");
        assert!(received_ping, "Must broadcast TacticalPingReceived to room peers");
    }

    #[test]
    fn test_ground_move_cancels_attack_and_preserves_movement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let (_tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, _rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        let mut matchmaker = Matchmaker::new();
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(1),
                p2_peer: None,
                is_active: true,
                match_time: 1.0,
                countdown_timer: 0.0,
                current_wave: 0,
                time_until_next_wave: 40.0,
            },
        );
        app.insert_resource(matchmaker);
        app.insert_resource(PlayerEconomy::new());

        // Spawn a friendly soldier at (0, 0) with a MoveTarget to (500, 0)
        let friendly = app.world_mut().spawn((
            NetEntity { net_id: 1, owner_peer_id: 1 },
            Faction::Player1,
            RoomId(1),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Radius(16.0),
            MoveSpeed(200.0),
            Velocity::default(),
            Health::new(100.0),
            Unit { name: "Marine".to_string(), supply_cost: 1 },
            Soldier {
                attack_range: 150.0,
                aggro_radius: 240.0,
                attack_damage: 15.0,
                attack_cooldown: 1.0,
                ..default()
            },
            MoveTarget::new(Vec2::new(500.0, 0.0), false),
        )).id();

        // Spawn a hostile enemy unit right next to the friendly soldier at (50, 0) (within attack range)
        let _hostile = app.world_mut().spawn((
            NetEntity { net_id: 2, owner_peer_id: 2 },
            Faction::HostileAi,
            RoomId(1),
            Transform::from_xyz(50.0, 0.0, 0.0),
            Radius(16.0),
            Health::new(100.0),
            Unit { name: "Enemy".to_string(), supply_cost: 1 },
            Soldier::default(),
        )).id();

        app.add_systems(Update, (server_combat_system, server_movement_system));

        // Step simulation
        app.update();

        // 1. MoveTarget MUST NOT be removed by combat system!
        let friendly_entity = app.world().entity(friendly);
        assert!(friendly_entity.get::<MoveTarget>().is_some(), "MoveTarget must remain intact during ground move");
        let soldier = friendly_entity.get::<Soldier>().unwrap();
        assert_eq!(soldier.target, None, "Target must be None during ground move");
        assert_eq!(soldier.state, SoldierState::MovingToGround, "Soldier state must be MovingToGround");
    }

    #[test]
    fn test_idle_unit_auto_attacks_enemy_in_range_without_move_target() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let (_tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, _rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        let mut matchmaker = Matchmaker::new();
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(1),
                p2_peer: None,
                is_active: true,
                match_time: 1.0,
                countdown_timer: 0.0,
                current_wave: 0,
                time_until_next_wave: 40.0,
            },
        );
        app.insert_resource(matchmaker);
        app.insert_resource(PlayerEconomy::new());

        // Spawn an idle friendly soldier at (0, 0) with NO MoveTarget
        let friendly = app.world_mut().spawn((
            NetEntity { net_id: 1, owner_peer_id: 1 },
            Faction::Player1,
            RoomId(1),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Radius(16.0),
            MoveSpeed(200.0),
            Health::new(100.0),
            Unit { name: "Marine".to_string(), supply_cost: 1 },
            Soldier {
                attack_range: 150.0,
                aggro_radius: 240.0,
                attack_damage: 15.0,
                attack_cooldown: 1.0,
                ..default()
            },
        )).id();

        // Spawn a hostile enemy at (80, 0) (within 150px attack range)
        let hostile = app.world_mut().spawn((
            NetEntity { net_id: 2, owner_peer_id: 2 },
            Faction::HostileAi,
            RoomId(1),
            Transform::from_xyz(80.0, 0.0, 0.0),
            Radius(16.0),
            Health::new(100.0),
            Unit { name: "Enemy".to_string(), supply_cost: 1 },
            Soldier::default(),
        )).id();

        app.add_systems(Update, server_combat_system);

        // Step simulation
        app.update();

        let friendly_entity = app.world().entity(friendly);
        let soldier = friendly_entity.get::<Soldier>().unwrap();
        assert_eq!(soldier.target, Some(hostile), "Idle soldier must auto-acquire hostile within attack range");
        assert_eq!(soldier.state, SoldierState::Attacking, "Soldier must be in Attacking state");
    }

    #[test]
    fn test_unit_to_unit_hard_collision() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        // Spawn two overlapping friendly units at (0,0) and (10,0) with radius 16.0 (min_dist = 32.0)
        let u1 = app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            Radius(16.0),
            Faction::Player1,
            RoomId(1),
            Unit { name: "Marine 1".to_string(), supply_cost: 1 },
        )).id();

        let u2 = app.world_mut().spawn((
            Transform::from_xyz(10.0, 0.0, 0.0),
            Radius(16.0),
            Faction::Player1,
            RoomId(1),
            Unit { name: "Marine 2".to_string(), supply_cost: 1 },
        )).id();

        app.add_systems(Update, server_unit_separation_and_collision_system);
        app.update();

        let p1 = app.world().entity(u1).get::<Transform>().unwrap().translation.truncate();
        let p2 = app.world().entity(u2).get::<Transform>().unwrap().translation.truncate();
        let dist = p1.distance(p2);

        assert!(dist >= 31.9, "Overlapping units must be pushed apart to at least radius+radius (was {:.2})", dist);
    }

    #[test]
    fn test_building_obstacle_collision_resolution() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        // Spawn a building in open terrain at (-800, -800) with radius 50.0
        let b_pos = Vec2::new(-800.0, -800.0);
        let _building = app.world_mut().spawn((
            Building::new("Barracks", Vec2::new(100.0, 100.0), 3.0, false),
            Transform::from_xyz(b_pos.x, b_pos.y, 0.0),
            Radius(50.0),
            Faction::Player1,
            RoomId(1),
        )).id();

        // Spawn a unit inside the building footprint at (-790, -800) with radius 16.0 (required min_dist = 68.0)
        let unit = app.world_mut().spawn((
            Transform::from_xyz(b_pos.x + 10.0, b_pos.y, 0.0),
            Radius(16.0),
            Faction::Player1,
            RoomId(1),
            Unit { name: "Marine".to_string(), supply_cost: 1 },
        )).id();

        app.add_systems(Update, server_unit_separation_and_collision_system);
        app.update();

        let u_pos = app.world().entity(unit).get::<Transform>().unwrap().translation.truncate();
        let dist_to_building = u_pos.distance(b_pos);

        assert!(dist_to_building >= 65.9, "Unit must be ejected outside building radius (dist: {:.2})", dist_to_building);
    }

    #[test]
    fn test_static_map_obstacle_collision_resolution() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        // Pick an obstacle from STATIC_MAP_OBSTACLES
        let obs = shared::map::STATIC_MAP_OBSTACLES[0];

        // Spawn a unit inside the obstacle center (pos = obs.position, radius = 16.0)
        let unit = app.world_mut().spawn((
            Transform::from_xyz(obs.position.x, obs.position.y, 0.0),
            Radius(16.0),
            Faction::Player1,
            RoomId(1),
            Unit { name: "Marine".to_string(), supply_cost: 1 },
        )).id();

        app.add_systems(Update, server_unit_separation_and_collision_system);
        app.update();

        let u_pos = app.world().entity(unit).get::<Transform>().unwrap().translation.truncate();
        let dist = u_pos.distance(obs.position);
        let min_required = 16.0 + obs.radius - 0.1;

        assert!(dist >= min_required, "Unit must be pushed outside static obstacle radius (dist: {:.2}, req: {:.2})", dist, min_required);
    }

    #[test]
    fn test_attack_move_acquires_and_engages_enemy_on_encounter() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let (_tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, _rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        let mut matchmaker = Matchmaker::new();
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(1),
                p2_peer: None,
                is_active: true,
                match_time: 1.0,
                countdown_timer: 0.0,
                current_wave: 1,
                time_until_next_wave: 40.0,
            },
        );
        app.insert_resource(matchmaker);
        app.insert_resource(PlayerEconomy::new());

        // Spawn Hostile AI marine attack-moving towards player base at (1000, 0)
        let hostile = app.world_mut().spawn((
            NetEntity { net_id: 1, owner_peer_id: 2 },
            Faction::HostileAi,
            RoomId(1),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Radius(16.0),
            MoveSpeed(180.0),
            Velocity::default(),
            Health::new(120.0),
            Unit { name: "Hostile Marine".to_string(), supply_cost: 2 },
            Soldier {
                state: SoldierState::AttackMoving,
                attack_range: 150.0,
                aggro_radius: 240.0,
                attack_damage: 15.0,
                attack_cooldown: 0.85,
                ..default()
            },
            MoveTarget::new(Vec2::new(1000.0, 0.0), true), // Attack-Move order!
        )).id();

        // Spawn Player marine standing at (180, 0) (within 240px aggro range)
        let friendly = app.world_mut().spawn((
            NetEntity { net_id: 2, owner_peer_id: 1 },
            Faction::Player1,
            RoomId(1),
            Transform::from_xyz(180.0, 0.0, 0.0),
            Radius(16.0),
            MoveSpeed(180.0),
            Velocity::default(),
            Health::new(120.0),
            Unit { name: "Marine".to_string(), supply_cost: 1 },
            Soldier::default(),
        )).id();

        app.add_systems(Update, (server_combat_system, server_movement_system).chain());
        app.update();

        // Assert hostile marine stopped ignoring player and engaged in combat!
        let hostile_soldier = app.world().entity(hostile).get::<Soldier>().unwrap();
        assert_eq!(hostile_soldier.target, Some(friendly), "Attack-moving enemy must acquire encountered player unit");
        assert_eq!(hostile_soldier.state, SoldierState::ChasingTarget, "Hostile soldier should be chasing/engaging the target");

        // Velocity must be zeroed for waypoint marching
        let hostile_vel = app.world().entity(hostile).get::<Velocity>().unwrap();
        assert_eq!(hostile_vel.0, Vec2::ZERO, "Waypoint velocity must pause while actively engaging in combat");
    }

    #[test]
    fn test_inactive_room_freezes_movement_and_combat() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let (_tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, _rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });

        // Set room.is_active = false (Match ended!)
        let mut matchmaker = Matchmaker::new();
        matchmaker.rooms.insert(
            1,
            Room {
                room_id: 1,
                room_code: None,
                mode: GameMode::SoloVsAi,
                p1_peer: Some(1),
                p2_peer: None,
                is_active: false, // Inactive / Ended
                match_time: 120.0,
                countdown_timer: 0.0,
                current_wave: 2,
                time_until_next_wave: 0.0,
            },
        );
        app.insert_resource(matchmaker);
        app.insert_resource(PlayerEconomy::new());

        let unit = app.world_mut().spawn((
            NetEntity { net_id: 1, owner_peer_id: 1 },
            Faction::Player1,
            RoomId(1),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Radius(16.0),
            MoveSpeed(180.0),
            Velocity::default(),
            Health::new(120.0),
            Unit { name: "Marine".to_string(), supply_cost: 1 },
            Soldier::default(),
            MoveTarget::new(Vec2::new(500.0, 0.0), false),
        )).id();

        app.add_systems(Update, (server_movement_system, server_combat_system));
        app.update();

        let vel = app.world().entity(unit).get::<Velocity>().unwrap();
        assert_eq!(vel.0, Vec2::ZERO, "Movement must freeze when match is inactive / ended");
        let soldier = app.world().entity(unit).get::<Soldier>().unwrap();
        assert_eq!(soldier.target, None, "No target acquisition when match is inactive / ended");
    }

    #[test]
    fn test_queue_cancellation_and_telemetry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let (tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, mut rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        app.insert_resource(Matchmaker::new());
        app.insert_resource(PlayerEconomy::new());
        app.insert_resource(NavGrid::default());
        app.add_systems(Update, handle_incoming_network_events);

        // 1. Peer 101 joins 1v1 queue
        tx_in.send(IncomingNetEvent::MessageReceived {
            peer_id: 101,
            msg: ClientMessage::JoinLobby {
                player_name: "Player 101".to_string(),
                mode: GameMode::Multiplayer1v1,
                room_code: None,
                faction_color: Some(FactionColor::Blue),
            },
        }).unwrap();
        app.update();

        assert_eq!(app.world().resource::<Matchmaker>().waiting_1v1_peer, Some(101));

        // 2. Peer 101 cancels queue
        tx_in.send(IncomingNetEvent::MessageReceived {
            peer_id: 101,
            msg: ClientMessage::CancelQueue,
        }).unwrap();
        app.update();

        assert_eq!(app.world().resource::<Matchmaker>().waiting_1v1_peer, None);
        let mut received_cancel = false;
        while let Ok(ev) = rx_out.try_recv() {
            if let OutgoingNetEvent::SendToPeer { peer_id, msg: ServerMessage::QueueCancelled } = ev {
                assert_eq!(peer_id, 101);
                received_cancel = true;
            }
        }
        assert!(received_cancel, "Must send QueueCancelled acknowledgement to client");
    }

    #[test]
    fn test_match_found_triggers_3s_countdown() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let (tx_in, rx_in) = crossbeam_channel::unbounded();
        let (tx_out, mut rx_out) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ServerNetworkChannels {
            rx_incoming: rx_in,
            tx_outgoing: tx_out,
        });
        app.insert_resource(Matchmaker::new());
        app.insert_resource(PlayerEconomy::new());
        app.insert_resource(NavGrid::default());
        app.add_systems(Update, handle_incoming_network_events);

        // P1 queues
        tx_in.send(IncomingNetEvent::MessageReceived {
            peer_id: 101,
            msg: ClientMessage::JoinLobby {
                player_name: "Alice".to_string(),
                mode: GameMode::Multiplayer1v1,
                room_code: None,
                faction_color: Some(FactionColor::Blue),
            },
        }).unwrap();
        app.update();

        // P2 queues -> Match made!
        tx_in.send(IncomingNetEvent::MessageReceived {
            peer_id: 102,
            msg: ClientMessage::JoinLobby {
                player_name: "Bob".to_string(),
                mode: GameMode::Multiplayer1v1,
                room_code: None,
                faction_color: Some(FactionColor::Red),
            },
        }).unwrap();
        app.update();

        let mm = app.world().resource::<Matchmaker>();
        let room = mm.rooms.get(&1).unwrap();
        assert_eq!(room.countdown_timer, 3.0, "Room must start with 3.0s countdown");

        let mut match_found_count = 0;
        while let Ok(ev) = rx_out.try_recv() {
            if let OutgoingNetEvent::SendToPeer { msg: ServerMessage::MatchFound { countdown_seconds, .. }, .. } = ev {
                assert_eq!(countdown_seconds, 3.0);
                match_found_count += 1;
            }
        }
        assert_eq!(match_found_count, 2, "Both players must receive MatchFound message");
    }
}
