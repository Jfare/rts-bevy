use bevy::prelude::*;
use shared::components::*;
use shared::grid::BuildingKind;
use shared::protocol::{ServerMessage, UnitKind};

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;
use super::{get_player_and_room, NodeQuery, ProdQuery, UnitQuery};

pub fn handle_harvest(
    commands: &mut Commands,
    net_channels: &ServerNetworkChannels,
    matchmaker: &Matchmaker,
    unit_query: &mut UnitQuery,
    node_query: &NodeQuery,
    peer_id: u64,
    worker_net_ids: &[u32],
    resource_net_id: u32,
) {
    let (player_faction, player_room) = get_player_and_room(matchmaker, peer_id);
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
            unit_query.iter_mut()
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

pub fn handle_build(
    commands: &mut Commands,
    net_channels: &ServerNetworkChannels,
    matchmaker: &mut Matchmaker,
    peer_id: u64,
    building_kind: BuildingKind,
    position: Vec2,
) {
    let (player_faction, player_room) = get_player_and_room(matchmaker, peer_id);

    let b_radius = building_kind.size().x.max(building_kind.size().y) * 0.5;
    if shared::map::is_obstacle_blocked(position, b_radius, 4.0) {
        return;
    }

    let has_funds = matchmaker
        .rooms
        .get_mut(&player_room)
        .map(|r| {
            if r.economy.has_minerals(player_faction, building_kind.mineral_cost()) {
                r.economy.spend_minerals(player_faction, building_kind.mineral_cost());
                true
            } else {
                false
            }
        })
        .unwrap_or(false);

    if has_funds {
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

pub fn handle_train_unit(
    net_channels: &ServerNetworkChannels,
    matchmaker: &mut Matchmaker,
    prod_query: &mut ProdQuery,
    peer_id: u64,
    building_net_id: u32,
    unit_kind: UnitKind,
) {
    let (player_faction, player_room) = get_player_and_room(matchmaker, peer_id);

    for (_, net_entity, faction, b_room, mut prod) in prod_query.iter_mut() {
        if net_entity.net_id == building_net_id
            && *faction == player_faction
            && b_room.0 == player_room
            && prod.queue.len() < prod.max_queue_size
        {
            let can_train = matchmaker
                .rooms
                .get_mut(&player_room)
                .map(|r| {
                    if r.economy.has_minerals(player_faction, unit_kind.mineral_cost())
                        && r.economy.has_supply(player_faction, unit_kind.supply_cost())
                    {
                        r.economy.spend_minerals(player_faction, unit_kind.mineral_cost());
                        r.economy.register_supply(player_faction, unit_kind.supply_cost());
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);

            if can_train {
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
}

pub fn handle_set_rally_point(
    prod_query: &mut ProdQuery,
    building_net_id: u32,
    rally_position: Vec2,
) {
    for (_, net_entity, _, _, mut prod) in prod_query.iter_mut() {
        if net_entity.net_id == building_net_id {
            prod.rally_point = rally_position;
        }
    }
}

