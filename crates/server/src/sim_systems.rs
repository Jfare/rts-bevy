use bevy::prelude::*;
use crate::game_session::{spawn_match_entities, Matchmaker, PlayerSession, Room};
use crate::net_server::{IncomingNetEvent, OutgoingNetEvent, ServerNetworkChannels};
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::BuildingKind;
use shared::protocol::{
    EntitySnapshot, GameMode, ServerMessage, UnitKind,
};
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
            .insert_resource(ServerTickTimer(Timer::from_seconds(
                1.0 / 30.0,
                TimerMode::Repeating,
            )))
            .add_systems(
                Update,
                (
                    handle_incoming_network_events,
                    server_movement_system,
                    server_boids_separation_system,
                    server_mining_system,
                    server_production_system,
                    server_combat_system,
                    server_match_outcome_system,
                    server_tick_snapshot_system,
                ),
            );
    }
}

#[derive(Resource)]
pub struct ServerTickTimer(pub Timer);

/// Reads and executes client network commands
fn handle_incoming_network_events(
    mut commands: Commands,
    net_channels: Res<ServerNetworkChannels>,
    mut matchmaker: ResMut<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    mut unit_query: Query<(
        Entity,
        &NetEntity,
        &Faction,
        Option<&mut MoveTarget>,
        Option<&mut Soldier>,
        Option<&mut Worker>,
    ), Without<ResourceNode>>,
    node_query: Query<(Entity, &NetEntity, &Transform), With<ResourceNode>>,
    mut prod_query: Query<(Entity, &NetEntity, &Faction, &mut ProductionBuilding)>,
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
                matchmaker.players.remove(&peer_id);
            }
            IncomingNetEvent::MessageReceived { peer_id, msg } => {
                match msg {
                    shared::protocol::ClientMessage::JoinLobby { player_name, mode } => {
                        handle_join_lobby(
                            &mut commands,
                            &net_channels,
                            &mut matchmaker,
                            &mut economy,
                            peer_id,
                            player_name,
                            mode,
                        );
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

                        for (_, net_entity, faction, move_target_opt, soldier_opt, worker_opt) in
                            &mut unit_query
                        {
                            if unit_net_ids.contains(&net_entity.net_id) && *faction == player_faction
                            {
                                if let Some(mut soldier) = soldier_opt {
                                    soldier.target = None;
                                    soldier.state = if is_attack_move {
                                        SoldierState::AttackMoving
                                    } else {
                                        SoldierState::MovingToGround
                                    };
                                }
                                if let Some(mut worker) = worker_opt {
                                    worker.state = WorkerState::Idle;
                                    worker.target_node = None;
                                }

                                if let Some(mut mt) = move_target_opt {
                                    mt.destination = target_position;
                                    mt.is_attack_move = is_attack_move;
                                }
                            }
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

                        let target_entity = unit_query
                            .iter()
                            .find(|(_, net_entity, _, _, _, _)| net_entity.net_id == target_net_id)
                            .map(|(e, _, _, _, _, _)| e);

                        if let Some(target) = target_entity {
                            for (_, net_entity, faction, _, soldier_opt, _) in &mut unit_query {
                                if unit_net_ids.contains(&net_entity.net_id)
                                    && *faction == player_faction
                                {
                                    if let Some(mut soldier) = soldier_opt {
                                        soldier.target = Some(target);
                                        soldier.state = SoldierState::ChasingTarget;
                                    }
                                }
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

                        let target_node = node_query
                            .iter()
                            .find(|(_, net_entity, _)| net_entity.net_id == resource_net_id)
                            .map(|(e, _, _)| e);

                        if let Some(node_e) = target_node {
                            for (_, net_entity, faction, _, _, worker_opt) in &mut unit_query {
                                if worker_net_ids.contains(&net_entity.net_id)
                                    && *faction == player_faction
                                {
                                    if let Some(mut worker) = worker_opt {
                                        worker.target_node = Some(node_e);
                                        worker.state = WorkerState::MovingToResource;
                                    }
                                }
                            }
                        }
                    }
                    shared::protocol::ClientMessage::RequestStop { unit_net_ids } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);

                        for (e, net_entity, faction, _, soldier_opt, worker_opt) in
                            &mut unit_query
                        {
                            if unit_net_ids.contains(&net_entity.net_id) && *faction == player_faction
                            {
                                commands.entity(e).remove::<MoveTarget>();
                                if let Some(mut soldier) = soldier_opt {
                                    soldier.target = None;
                                    soldier.state = SoldierState::Idle;
                                }
                                if let Some(mut worker) = worker_opt {
                                    worker.state = WorkerState::Idle;
                                }
                            }
                        }
                    }
                    shared::protocol::ClientMessage::RequestHoldPosition { unit_net_ids } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);

                        for (e, net_entity, faction, _, soldier_opt, _) in &mut unit_query {
                            if unit_net_ids.contains(&net_entity.net_id) && *faction == player_faction
                            {
                                commands.entity(e).remove::<MoveTarget>();
                                if let Some(mut soldier) = soldier_opt {
                                    soldier.target = None;
                                    soldier.state = SoldierState::HoldingPosition;
                                }
                            }
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
                            }

                            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
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
                    shared::protocol::ClientMessage::RequestTrainUnit {
                        building_net_id,
                        unit_kind,
                    } => {
                        let player_faction = matchmaker
                            .players
                            .get(&peer_id)
                            .map(|p| p.faction)
                            .unwrap_or(Faction::Player1);

                        for (_, net_entity, faction, mut prod) in &mut prod_query {
                            if net_entity.net_id == building_net_id && *faction == player_faction {
                                if economy.has_minerals(player_faction, unit_kind.mineral_cost())
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

                                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
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
                        for (_, net_entity, _, mut prod) in &mut prod_query {
                            if net_entity.net_id == building_net_id {
                                prod.rally_point = rally_position;
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
) {
    match mode {
        GameMode::SoloVsAi => {
            let room_id = matchmaker.next_room_id;
            matchmaker.next_room_id += 1;

            matchmaker.players.insert(
                peer_id,
                PlayerSession {
                    peer_id,
                    name: player_name,
                    room_id,
                    faction: Faction::Player1,
                },
            );

            matchmaker.rooms.insert(
                room_id,
                Room {
                    room_id,
                    mode,
                    p1_peer: Some(peer_id),
                    p2_peer: None,
                    is_active: true,
                    match_time: 0.0,
                },
            );

            let initial_entities = spawn_match_entities(
                commands,
                matchmaker,
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
                    is_game_ready: true,
                },
            });

            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                peer_id,
                msg: ServerMessage::GameStarted {
                    p1_pos: Vec2::new(-700.0, 250.0),
                    p2_pos: Vec2::new(700.0, -250.0),
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
        }
        GameMode::Multiplayer1v1 => {
            if let Some(waiting_p1) = matchmaker.waiting_1v1_peer.take() {
                // Pair both players!
                let room_id = matchmaker.next_room_id;
                matchmaker.next_room_id += 1;

                matchmaker.players.insert(
                    waiting_p1,
                    PlayerSession {
                        peer_id: waiting_p1,
                        name: "Player 1".to_string(),
                        room_id,
                        faction: Faction::Player1,
                    },
                );

                matchmaker.players.insert(
                    peer_id,
                    PlayerSession {
                        peer_id,
                        name: player_name,
                        room_id,
                        faction: Faction::Player2,
                    },
                );

                matchmaker.rooms.insert(
                    room_id,
                    Room {
                        room_id,
                        mode,
                        p1_peer: Some(waiting_p1),
                        p2_peer: Some(peer_id),
                        is_active: true,
                        match_time: 0.0,
                    },
                );

                let initial_entities = spawn_match_entities(
                    commands,
                    matchmaker,
                    GameMode::Multiplayer1v1,
                    waiting_p1,
                    Some(peer_id),
                );

                let (p1_cur_sup, p1_max_sup) = economy.get_supply(Faction::Player1);
                let (p2_cur_sup, p2_max_sup) = economy.get_supply(Faction::Player2);

                for (p_id, faction) in [(waiting_p1, Faction::Player1), (peer_id, Faction::Player2)] {
                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                        peer_id: p_id,
                        msg: ServerMessage::LobbyJoined {
                            player_id: p_id,
                            assigned_faction: faction,
                            room_id,
                            is_game_ready: true,
                        },
                    });

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                        peer_id: p_id,
                        msg: ServerMessage::GameStarted {
                            p1_pos: Vec2::new(-700.0, 250.0),
                            p2_pos: Vec2::new(700.0, -250.0),
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
            } else {
                // First player in 1v1 queue -> wait for opponent
                matchmaker.waiting_1v1_peer = Some(peer_id);
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::SendToPeer {
                    peer_id,
                    msg: ServerMessage::LobbyJoined {
                        player_id: peer_id,
                        assigned_faction: Faction::Player1,
                        room_id: 0,
                        is_game_ready: false,
                    },
                });
            }
        }
    }
}

/// Authoritative unit steering and movement
fn server_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Transform, &MoveSpeed, &mut Velocity, &MoveTarget)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, speed, mut velocity, move_target) in &mut query {
        let current_pos = transform.translation.truncate();
        let diff = move_target.destination - current_pos;
        let dist = diff.length();

        if dist <= 12.0 {
            velocity.0 = Vec2::ZERO;
            commands.entity(entity).remove::<MoveTarget>();
        } else {
            let dir = diff / dist;
            velocity.0 = dir * speed.0;
            transform.translation.x += velocity.0.x * dt;
            transform.translation.y += velocity.0.y * dt;

            let angle = dir.y.atan2(dir.x);
            transform.rotation = Quat::from_rotation_z(angle);
        }
    }
}

/// Unit-to-unit soft separation
fn server_boids_separation_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &Radius), With<Unit>>,
) {
    let dt = time.delta_secs();
    let mut combinations = query.iter_combinations_mut();
    while let Some([(mut t1, r1), (mut t2, r2)]) = combinations.fetch_next() {
        let p1 = t1.translation.truncate();
        let p2 = t2.translation.truncate();
        let diff = p1 - p2;
        let dist = diff.length();
        let min_dist = r1.0 + r2.0;

        if dist < min_dist && dist > 0.001 {
            let overlap = min_dist - dist;
            let push = (diff / dist) * overlap * 6.0 * dt;
            t1.translation.x += push.x * 0.5;
            t1.translation.y += push.y * 0.5;
            t2.translation.x -= push.x * 0.5;
            t2.translation.y -= push.y * 0.5;
        }
    }
}

/// SCV mining and resource dropoff loop
fn server_mining_system(
    time: Res<Time>,
    mut economy: ResMut<PlayerEconomy>,
    mut workers: Query<(&mut Transform, &MoveSpeed, &Faction, &mut Worker)>,
    mut nodes: Query<(&Transform, &mut ResourceNode, &NetEntity), Without<Worker>>,
    bases: Query<(&Transform, &Faction), (With<BaseHQ>, Without<Worker>, Without<ResourceNode>)>,
) {
    let dt = time.delta_secs();
    for (mut transform, speed, faction, mut worker) in &mut workers {
        match worker.state {
            WorkerState::Idle => {}
            WorkerState::MovingToResource => {
                if let Some(target_node_e) = worker.target_node {
                    if let Ok((node_tf, _, _)) = nodes.get(target_node_e) {
                        let current_pos = transform.translation.truncate();
                        let target_pos = node_tf.translation.truncate();
                        let diff = target_pos - current_pos;
                        let dist = diff.length();

                        if dist <= worker.interact_distance {
                            worker.state = WorkerState::Mining;
                            worker.harvest_timer = 0.0;
                        } else {
                            let dir = diff / dist;
                            transform.translation.x += dir.x * speed.0 * dt;
                            transform.translation.y += dir.y * speed.0 * dt;
                            transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                        }
                    } else {
                        worker.state = WorkerState::Idle;
                    }
                }
            }
            WorkerState::Mining => {
                worker.harvest_timer += dt;
                if worker.harvest_timer >= worker.harvest_duration {
                    worker.harvest_timer = 0.0;
                    if let Some(target_node_e) = worker.target_node {
                        if let Ok((_, mut node, _)) = nodes.get_mut(target_node_e) {
                            let harvested = node.harvest(worker.harvest_capacity);
                            worker.carried_minerals = harvested;
                            worker.state = WorkerState::MovingToBase;
                        } else {
                            worker.state = WorkerState::Idle;
                        }
                    }
                }
            }
            WorkerState::MovingToBase => {
                // Find closest friendly Base HQ
                let current_pos = transform.translation.truncate();
                let mut closest_base_pos = None;
                let mut min_dist = f32::MAX;

                for (base_tf, base_faction) in &bases {
                    if base_faction == faction {
                        let pos = base_tf.translation.truncate();
                        let d = (pos - current_pos).length();
                        if d < min_dist {
                            min_dist = d;
                            closest_base_pos = Some(pos);
                        }
                    }
                }

                if let Some(base_pos) = closest_base_pos {
                    let diff = base_pos - current_pos;
                    let dist = diff.length();

                    if dist <= worker.base_interact_distance {
                        economy.add_minerals(*faction, worker.carried_minerals);
                        worker.carried_minerals = 0;
                        worker.state = WorkerState::MovingToResource;
                    } else {
                        let dir = diff / dist;
                        transform.translation.x += dir.x * speed.0 * dt;
                        transform.translation.y += dir.y * speed.0 * dt;
                        transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
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
    mut buildings: Query<(
        Entity,
        &NetEntity,
        &Faction,
        &Transform,
        &mut Building,
        Option<&mut ProductionBuilding>,
        Option<&SupplyDepot>,
    )>,
) {
    let dt = time.delta_secs();
    for (_entity, net_entity, faction, transform, mut building, prod_opt, supply_depot_opt) in
        &mut buildings
    {
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
                        } else {
                            UnitKind::Soldier
                        };

                        let net_id = matchmaker.alloc_net_id();
                        let spawn_pos = transform.translation.truncate() + Vec2::new(0.0, -60.0);
                        let rally = prod.rally_point;

                        let mut unit_cmds = commands.spawn((
                            Unit {
                                name: finished_unit.name.clone(),
                                supply_cost: finished_unit.supply_cost,
                            },
                            Health::new(unit_kind.max_health()),
                            *faction,
                            NetEntity {
                                net_id,
                                owner_peer_id: net_entity.owner_peer_id,
                            },
                            MoveTarget {
                                destination: rally,
                                is_attack_move: false,
                            },
                            Transform::from_xyz(spawn_pos.x, spawn_pos.y, 2.0),
                        ));

                        match unit_kind {
                            UnitKind::Worker => {
                                unit_cmds.insert((
                                    Worker::default(),
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
                                    Radius(16.0),
                                    MoveSpeed(180.0),
                                    Velocity::default(),
                                ));
                            }
                        }

                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
                            msg: ServerMessage::UnitSpawned {
                                net_id,
                                faction: *faction,
                                unit_kind,
                                position: spawn_pos,
                                max_hp: unit_kind.max_health(),
                            },
                        });

                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
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

/// Military combat, aggro, weapon cooldowns, and damage deduction
fn server_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    mut economy: ResMut<PlayerEconomy>,
    mut soldiers: Query<(
        Entity,
        &NetEntity,
        &Faction,
        &Transform,
        &mut Soldier,
    )>,
    mut targets: Query<(
        Entity,
        &NetEntity,
        &Faction,
        &Transform,
        &mut Health,
        Option<&Unit>,
    )>,
) {
    let dt = time.delta_secs();

    for (_s_entity, attacker_net, attacker_faction, attacker_tf, mut soldier) in &mut soldiers {
        soldier.attack_timer += dt;
        let attacker_pos = attacker_tf.translation.truncate();

        // 1. Scan for nearest hostile target if idle or attack-moving without target
        if soldier.target.is_none()
            && (soldier.state == SoldierState::Idle
                || soldier.state == SoldierState::AttackMoving
                || soldier.state == SoldierState::HoldingPosition)
        {
            let mut closest_target = None;
            let mut min_dist = soldier.aggro_radius;

            for (t_entity, _, target_faction, target_tf, target_hp, _) in &targets {
                if attacker_faction.is_hostile_to(target_faction) && !target_hp.is_dead() {
                    let d = (target_tf.translation.truncate() - attacker_pos).length();
                    if d < min_dist {
                        min_dist = d;
                        closest_target = Some(t_entity);
                    }
                }
            }

            if let Some(target) = closest_target {
                soldier.target = Some(target);
            }
        }

        // 2. Fire weapon if target is in range and cooldown is ready
        if let Some(target_entity) = soldier.target {
            if let Ok((_, target_net, _, target_tf, mut target_hp, unit_opt)) =
                targets.get_mut(target_entity)
            {
                let target_pos = target_tf.translation.truncate();
                let dist = (target_pos - attacker_pos).length();

                if dist <= soldier.attack_range {
                    if soldier.attack_timer >= soldier.attack_cooldown {
                        soldier.attack_timer = 0.0;
                        target_hp.take_damage(soldier.attack_damage);

                        // Broadcast tracer projectile event
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
                            msg: ServerMessage::ProjectileFired {
                                attacker_net_id: attacker_net.net_id,
                                target_net_id: target_net.net_id,
                                origin: attacker_pos,
                                target_pos,
                                damage: soldier.attack_damage,
                            },
                        });

                        // Broadcast damage update
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
                            msg: ServerMessage::EntityDamaged {
                                target_net_id: target_net.net_id,
                                current_hp: target_hp.current,
                                max_hp: target_hp.max,
                            },
                        });

                        // If dead, cleanup & broadcast death
                        if target_hp.is_dead() {
                            if let Some(unit) = unit_opt {
                                economy.unregister_supply(*attacker_faction, unit.supply_cost);
                            }

                            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
                                msg: ServerMessage::EntityDied {
                                    net_id: target_net.net_id,
                                    faction: *attacker_faction,
                                },
                            });

                            commands.entity(target_entity).despawn_recursive();
                            soldier.target = None;
                        }
                    }
                } else if soldier.state != SoldierState::HoldingPosition {
                    // Step closer to target
                    let dir = (target_pos - attacker_pos).normalize_or_zero();
                    let step = dir * 180.0 * dt;
                    // (Transform translation handled in main movement)
                    let _ = step;
                }
            } else {
                soldier.target = None;
            }
        }
    }
}

/// Checks for Victory or Defeat when a Base HQ is destroyed
fn server_match_outcome_system(
    net_channels: Res<ServerNetworkChannels>,
    hq_query: Query<&Faction, With<BaseHQ>>,
    mut match_ended: Local<bool>,
) {
    if *match_ended {
        return;
    }

    let mut has_p1_hq = false;
    let mut has_enemy_hq = false;

    for faction in &hq_query {
        if *faction == Faction::Player1 {
            has_p1_hq = true;
        } else {
            has_enemy_hq = true;
        }
    }

    if !has_p1_hq && has_enemy_hq {
        *match_ended = true;
        info!("💥 [GameServer] Player 1 Base HQ Destroyed! Defeat.");
        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
            msg: ServerMessage::MatchEnded {
                winning_faction: Faction::Player2,
                duration_seconds: 0.0,
            },
        });
    } else if has_p1_hq && !has_enemy_hq {
        *match_ended = true;
        info!("🏆 [GameServer] Enemy Base HQ Destroyed! Victory.");
        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
            msg: ServerMessage::MatchEnded {
                winning_faction: Faction::Player1,
                duration_seconds: 0.0,
            },
        });
    }
}

/// 30 Hz position, rotation, and health snapshot broadcast
fn server_tick_snapshot_system(
    time: Res<Time>,
    mut tick_timer: ResMut<ServerTickTimer>,
    net_channels: Res<ServerNetworkChannels>,
    economy: Res<PlayerEconomy>,
    ai_state: Option<Res<bot_ai::WaveAiState>>,
    entities_query: Query<(
        &NetEntity,
        &Transform,
        &Health,
        Option<&Worker>,
    )>,
    node_query: Query<(&NetEntity, &Transform), With<ResourceNode>>,
    mut tick_counter: Local<u32>,
) {
    tick_timer.0.tick(time.delta());
    if tick_timer.0.just_finished() {
        *tick_counter += 1;
        let mut snapshots = Vec::new();

        for (net_entity, transform, health, worker_opt) in &entities_query {
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

            snapshots.push(EntitySnapshot {
                net_id: net_entity.net_id,
                position: transform.translation.truncate(),
                rotation,
                current_hp: health.current,
                max_hp: health.max,
                is_mining,
                laser_target,
            });
        }

        let (next_wave, wave_num) = if let Some(ai) = ai_state {
            (ai.time_until_next_wave, ai.current_wave)
        } else {
            (0.0, 0)
        };

        let (p1_cur_sup, p1_max_sup) = economy.get_supply(Faction::Player1);
        let (p2_cur_sup, p2_max_sup) = economy.get_supply(Faction::Player2);

        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
            msg: ServerMessage::TickSnapshotBatch {
                tick: *tick_counter,
                snapshots,
                p1_minerals: economy.get_minerals(Faction::Player1),
                p1_supply: p1_cur_sup,
                p1_max_supply: p1_max_sup,
                p2_minerals: economy.get_minerals(Faction::Player2),
                p2_supply: p2_cur_sup,
                p2_max_supply: p2_max_sup,
                next_wave_seconds: next_wave,
                current_wave: wave_num,
            },
        });
    }
}
