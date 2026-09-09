use bevy::prelude::*;
use shared::components::*;
use shared::grid::NavGrid;
use shared::protocol::{ServerMessage, UnitKind};

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;

/// Unit training queues and construction progression
pub fn server_production_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    mut matchmaker: ResMut<Matchmaker>,
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
                    if let Some(room) = matchmaker.rooms.get_mut(&room_id.0) {
                        room.economy.add_max_supply(*faction, depot.supply_provided);
                    }
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

                        let unit_kind = if finished_unit.name.contains("Worker") || finished_unit.name.contains("SCV") {
                            UnitKind::Worker
                        } else if finished_unit.name.contains("Melee") {
                            UnitKind::MeleeFighter
                        } else {
                            UnitKind::RangedFighter
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
                            UnitKind::RangedFighter => {
                                unit_cmds.insert((
                                    Soldier {
                                        state: SoldierState::MovingToGround,
                                        attack_range: 150.0,
                                        aggro_radius: 240.0,
                                        attack_damage: 15.0,
                                        attack_cooldown: 0.85,
                                        ..default()
                                    },
                                    TacticalStance::default(),
                                    Radius(16.0),
                                    MoveSpeed(180.0),
                                    Velocity::default(),
                                ));
                            }
                            UnitKind::MeleeFighter => {
                                unit_cmds.insert((
                                    MeleeFighter::default(),
                                    TacticalStance::default(),
                                    Radius(16.0),
                                    MoveSpeed(195.0),
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

