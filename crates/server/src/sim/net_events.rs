use std::time::{SystemTime, UNIX_EPOCH};
use bevy::prelude::*;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::{BuildingKind, NavGrid};
use shared::protocol::ServerMessage;

use crate::net_server::{IncomingNetEvent, OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;
use super::lobby::handle_join_lobby;

/// Reads and executes client network commands
pub fn handle_incoming_network_events(
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
        Option<&mut MeleeFighter>,
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

                            for (e, tf, net_entity, faction, unit_room, move_target_opt, soldier_opt, worker_opt, melee_opt, _, stance_opt) in
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

                            for (e, tf, net_entity, faction, unit_room, move_target_opt, soldier_opt, _, _, _, stance_opt) in
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
                            .find(|(_, _, net_entity, _, unit_room, ..)| {
                                net_entity.net_id == target_net_id && unit_room.0 == player_room
                            })
                            .map(|(e, ..)| e);

                        if let Some(target) = target_entity {
                            let mut valid_net_ids = Vec::new();
                            for (e, _, net_entity, faction, unit_room, _, soldier_opt, _, melee_opt, _, _) in
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
                            for (e, _, net_entity, faction, unit_room, _, _, worker_opt, ..) in
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
                        for (e, _, net_entity, faction, unit_room, _, soldier_opt, worker_opt, melee_opt, _, stance_opt) in
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
                        for (e, _, net_entity, faction, unit_room, _, soldier_opt, _, mut melee_opt, _, stance_opt) in
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
