use super::*;
use bevy::ecs::system::RunSystemOnce;
use crate::net_server::{IncomingNetEvent, OutgoingNetEvent, ServerNetworkChannels};
use crate::session::{Matchmaker, PlayerSession, Room};
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::NavGrid;
use shared::protocol::{ClientMessage, ClientPlatform, FactionColor, GameMode, PingType, ServerMessage};

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
    let mut r1 = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    r1.match_time = 25.0;
    matchmaker.rooms.insert(1, r1);

    let mut r2 = Room::new(2, None, GameMode::SoloVsAi, Some(201), None);
    r2.match_time = 50.0;
    r2.current_wave = 2;
    r2.time_until_next_wave = 20.0;
    matchmaker.rooms.insert(2, r2);
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
            platform: ClientPlatform::Desktop,
        },
    );
    let mut r1 = Room::new(1, None, GameMode::SoloVsAi, Some(101), None);
    r1.match_time = 10.0;
    r1.current_wave = 1;
    r1.time_until_next_wave = 30.0;
    matchmaker.rooms.insert(1, r1);
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
            platform: ClientPlatform::Desktop,
        },
    );
    let mut r1 = Room::new(1, None, GameMode::SoloVsAi, Some(101), None);
    r1.match_time = 5.0;
    r1.current_wave = 1;
    r1.time_until_next_wave = 30.0;
    matchmaker.rooms.insert(1, r1);
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
            platform: ClientPlatform::Desktop,
        },
    );
    let mut r1 = Room::new(1, None, GameMode::SoloVsAi, Some(101), None);
    r1.match_time = 5.0;
    r1.current_wave = 1;
    r1.time_until_next_wave = 30.0;
    matchmaker.rooms.insert(1, r1);

    matchmaker.players.insert(
        201,
        PlayerSession {
            peer_id: 201,
            name: "Player 2".to_string(),
            room_id: 2,
            faction: Faction::Player1,
            color: FactionColor::Teal,
            platform: ClientPlatform::Desktop,
        },
    );
    let mut r2 = Room::new(2, None, GameMode::SoloVsAi, Some(201), None);
    r2.match_time = 15.0;
    r2.current_wave = 3;
    r2.time_until_next_wave = 10.0;
    matchmaker.rooms.insert(2, r2);
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
            platform: Some(ClientPlatform::Desktop),
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
            platform: Some(ClientPlatform::Desktop),
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
            platform: ClientPlatform::Desktop,
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
            platform: ClientPlatform::Desktop,
        },
    );
    let mut r1 = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    r1.match_time = 10.0;
    r1.current_wave = 0;
    r1.time_until_next_wave = 40.0;
    matchmaker.rooms.insert(1, r1);
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
    let mut r1 = Room::new(1, None, GameMode::SoloVsAi, Some(1), None);
    r1.match_time = 1.0;
    matchmaker.rooms.insert(1, r1);
    app.insert_resource(matchmaker);

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
    let mut r1 = Room::new(1, None, GameMode::SoloVsAi, Some(1), None);
    r1.match_time = 1.0;
    matchmaker.rooms.insert(1, r1);
    app.insert_resource(matchmaker);

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
    let mut r1 = Room::new(1, None, GameMode::SoloVsAi, Some(1), None);
    r1.match_time = 1.0;
    r1.current_wave = 1;
    matchmaker.rooms.insert(1, r1);
    app.insert_resource(matchmaker);

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
    let mut r1 = Room::new(1, None, GameMode::SoloVsAi, Some(1), None);
    r1.is_active = false;
    r1.match_time = 120.0;
    r1.current_wave = 2;
    r1.time_until_next_wave = 0.0;
    matchmaker.rooms.insert(1, r1);
    app.insert_resource(matchmaker);

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
            platform: Some(ClientPlatform::Desktop),
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
            platform: Some(ClientPlatform::Desktop),
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
            platform: Some(ClientPlatform::Mobile),
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

#[test]
fn test_room_economy_isolation() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let (tx_in, rx_in) = crossbeam_channel::unbounded();
    let (tx_out, _rx_out) = tokio::sync::mpsc::unbounded_channel();
    app.insert_resource(ServerNetworkChannels {
        rx_incoming: rx_in,
        tx_outgoing: tx_out,
    });
    app.insert_resource(Matchmaker::new());
    app.insert_resource(NavGrid::default());
    app.add_systems(Update, handle_incoming_network_events);

    // 1. Peer 101 starts Solo game -> Room 1
    tx_in.send(IncomingNetEvent::MessageReceived {
        peer_id: 101,
        msg: ClientMessage::JoinLobby {
            player_name: "Commander 1".to_string(),
            mode: GameMode::SoloVsAi,
            room_code: None,
            faction_color: Some(FactionColor::Blue),
            platform: Some(ClientPlatform::Desktop),
        },
    }).unwrap();
    app.update();

    // 2. Peer 201 starts Solo game -> Room 2
    tx_in.send(IncomingNetEvent::MessageReceived {
        peer_id: 201,
        msg: ClientMessage::JoinLobby {
            player_name: "Commander 2".to_string(),
            mode: GameMode::SoloVsAi,
            room_code: None,
            faction_color: Some(FactionColor::Teal),
            platform: Some(ClientPlatform::Desktop),
        },
    }).unwrap();
    app.update();

    let mut mm = app.world_mut().resource_mut::<Matchmaker>();
    let r1 = mm.rooms.get_mut(&1).expect("Room 1 must exist");
    assert_eq!(r1.economy.get_minerals(Faction::Player1), 200);
    assert_eq!(r1.economy.get_supply(Faction::Player1), (2, 10));

    // Room 1 gathers minerals
    r1.economy.add_minerals(Faction::Player1, 350);
    assert_eq!(r1.economy.get_minerals(Faction::Player1), 550);

    // Room 2 economy must remain completely untouched!
    let r2 = mm.rooms.get(&2).expect("Room 2 must exist");
    assert_eq!(
        r2.economy.get_minerals(Faction::Player1),
        200,
        "Room 2 minerals must remain 200 despite Room 1 mining"
    );
    assert_eq!(
        r2.economy.get_supply(Faction::Player1),
        (2, 10),
        "Room 2 supply must remain untouched"
    );
}

#[test]
fn test_consecutive_matches_have_fresh_economy() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let (tx_in, rx_in) = crossbeam_channel::unbounded();
    let (tx_out, mut rx_out) = tokio::sync::mpsc::unbounded_channel();
    app.insert_resource(ServerNetworkChannels {
        rx_incoming: rx_in,
        tx_outgoing: tx_out,
    });
    app.insert_resource(Matchmaker::new());
    app.insert_resource(NavGrid::default());
    app.add_systems(Update, handle_incoming_network_events);

    // 1. Peer 101 starts Match 1 (SoloVsAi)
    tx_in.send(IncomingNetEvent::MessageReceived {
        peer_id: 101,
        msg: ClientMessage::JoinLobby {
            player_name: "Commander".to_string(),
            mode: GameMode::SoloVsAi,
            room_code: None,
            faction_color: Some(FactionColor::Blue),
            platform: Some(ClientPlatform::Desktop),
        },
    }).unwrap();
    app.update();

    // Drain initial outgoing messages
    while rx_out.try_recv().is_ok() {}

    // 2. Mutate Match 1 economy (player collected 1000 minerals in game 1)
    {
        let mut mm = app.world_mut().resource_mut::<Matchmaker>();
        let r1 = mm.rooms.get_mut(&1).expect("Room 1 must exist");
        r1.economy.add_minerals(Faction::Player1, 1000);
        assert_eq!(r1.economy.get_minerals(Faction::Player1), 1200);
    }

    // 3. Peer 101 forfeits/closes Match 1
    tx_in.send(IncomingNetEvent::MessageReceived {
        peer_id: 101,
        msg: ClientMessage::ForfeitMatch,
    }).unwrap();
    app.update();

    // Verify room 1 was cleaned up
    {
        let mm = app.world().resource::<Matchmaker>();
        assert!(!mm.rooms.contains_key(&1), "Room 1 must be removed after forfeit");
    }

    // Drain messages from forfeit
    while rx_out.try_recv().is_ok() {}

    // 4. Peer 101 starts a brand new match (Match 2)
    tx_in.send(IncomingNetEvent::MessageReceived {
        peer_id: 101,
        msg: ClientMessage::JoinLobby {
            player_name: "Commander".to_string(),
            mode: GameMode::SoloVsAi,
            room_code: None,
            faction_color: Some(FactionColor::Blue),
            platform: Some(ClientPlatform::Desktop),
        },
    }).unwrap();
    app.update();

    // Check newly created Room 2 economy
    let mm = app.world().resource::<Matchmaker>();
    let r2 = mm.rooms.get(&2).expect("Room 2 must exist");
    assert_eq!(
        r2.economy.get_minerals(Faction::Player1),
        200,
        "New match must start with exactly 200 minerals, not lingering minerals from previous game"
    );
    assert_eq!(
        r2.economy.get_supply(Faction::Player1),
        (2, 10),
        "New match must start with 2 supply for 2 workers"
    );

    // Verify InitialWorldState sent to client has fresh minerals
    let mut initial_state_minerals = None;
    while let Ok(ev) = rx_out.try_recv() {
        if let OutgoingNetEvent::SendToPeer {
            msg: ServerMessage::InitialWorldState { p1_minerals, .. },
            ..
        } = ev
        {
            initial_state_minerals = Some(p1_minerals);
        }
    }
    assert_eq!(
        initial_state_minerals,
        Some(200),
        "Client must receive InitialWorldState with 200 minerals on fresh match"
    );
}

#[test]
fn test_idle_worker_auto_mines_nearest_gold_rock_within_range() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    // Friendly base at (0, -1000)
    world.spawn((
        Transform::from_xyz(0.0, -1000.0, 1.0),
        Faction::Player1,
        RoomId(1),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
    ));

    // Friendly mineral node at (0, -1200)
    let home_rock = world.spawn((
        Transform::from_xyz(0.0, -1200.0, 0.5),
        ResourceNode::new(2000),
        NetEntity { net_id: 10, owner_peer_id: 0 },
        RoomId(1),
    )).id();

    // Enemy base at (0, 1000)
    world.spawn((
        Transform::from_xyz(0.0, 1000.0, 1.0),
        Faction::Player2,
        RoomId(1),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
    ));

    // Enemy mineral node at (0, 1200)
    world.spawn((
        Transform::from_xyz(0.0, 1200.0, 0.5),
        ResourceNode::new(2000),
        NetEntity { net_id: 20, owner_peer_id: 0 },
        RoomId(1),
    ));

    // Idle friendly worker at (0, -1050) (dist 150 to home rock, within 450 range)
    let worker_ent = world.spawn((
        Transform::from_xyz(0.0, -1050.0, 2.0),
        MoveSpeed(WORKER_MOVE_SPEED),
        Faction::Player1,
        RoomId(1),
        Worker::default(),
    )).id();

    app.update();

    let worker = app.world().get::<Worker>(worker_ent).unwrap();
    assert_eq!(worker.state, WorkerState::MovingToResource, "Idle worker within range should start moving to resource");
    assert_eq!(worker.target_node, Some(home_rock), "Worker should target home mineral node");
}

#[test]
fn test_idle_worker_does_not_target_enemy_base_or_out_of_range_rock() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    // Friendly base at (0, -1000)
    world.spawn((
        Transform::from_xyz(0.0, -1000.0, 1.0),
        Faction::Player1,
        RoomId(1),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
    ));

    // Enemy base at (0, 1000)
    world.spawn((
        Transform::from_xyz(0.0, 1000.0, 1.0),
        Faction::Player2,
        RoomId(1),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
    ));

    // Enemy mineral node at (0, 1200)
    world.spawn((
        Transform::from_xyz(0.0, 1200.0, 0.5),
        ResourceNode::new(2000),
        NetEntity { net_id: 20, owner_peer_id: 0 },
        RoomId(1),
    ));

    // Worker stationed at scouting position (0, 800) near enemy base
    // Distance to enemy rock is 400 (within WORKER_AUTO_MINE_RANGE), but it's in enemy territory
    let worker_near_enemy = world.spawn((
        Transform::from_xyz(0.0, 800.0, 2.0),
        MoveSpeed(WORKER_MOVE_SPEED),
        Faction::Player1,
        RoomId(1),
        Worker::default(),
    )).id();

    // Worker stationed at middle of map (0, 0)
    // Distance to any rock is > 1000 (well outside 450 range)
    let worker_in_middle = world.spawn((
        Transform::from_xyz(0.0, 0.0, 2.0),
        MoveSpeed(WORKER_MOVE_SPEED),
        Faction::Player1,
        RoomId(1),
        Worker::default(),
    )).id();

    app.update();

    let w_enemy = app.world().get::<Worker>(worker_near_enemy).unwrap();
    assert_eq!(w_enemy.state, WorkerState::Idle, "Worker near enemy base should not auto-mine enemy gold");
    assert_eq!(w_enemy.target_node, None);

    let w_mid = app.world().get::<Worker>(worker_in_middle).unwrap();
    assert_eq!(w_mid.state, WorkerState::Idle, "Worker in middle of map should remain idle");
    assert_eq!(w_mid.target_node, None);
}

#[test]
fn test_gold_rock_5_worker_saturation_limit() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    // Friendly base at (0, -1000)
    world.spawn((
        Transform::from_xyz(0.0, -1000.0, 1.0),
        Faction::Player1,
        RoomId(1),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
    ));

    // Friendly mineral node at (0, -1200) (distance 200 to base, within 380)
    let home_rock = world.spawn((
        Transform::from_xyz(0.0, -1200.0, 0.5),
        ResourceNode::new(2000),
        NetEntity { net_id: 10, owner_peer_id: 0 },
        RoomId(1),
    )).id();

    // Spawn 7 idle friendly workers near base (dist ~150 to home rock)
    let mut worker_ents = Vec::new();
    for i in 0..7 {
        let ent = world.spawn((
            Transform::from_xyz(i as f32 * 5.0, -1050.0, 2.0),
            MoveSpeed(WORKER_MOVE_SPEED),
            Faction::Player1,
            RoomId(1),
            Worker::default(),
        )).id();
        worker_ents.push(ent);
    }

    app.update();

    let mut mining_count = 0;
    let mut idle_count = 0;

    for &w_ent in &worker_ents {
        let w = app.world().get::<Worker>(w_ent).unwrap();
        if w.state == WorkerState::MovingToResource {
            assert_eq!(w.target_node, Some(home_rock));
            mining_count += 1;
        } else if w.state == WorkerState::Idle {
            assert_eq!(w.target_node, None);
            idle_count += 1;
        }
    }

    assert_eq!(mining_count, 5, "Exactly 5 workers should target the gold rock (MAX_WORKERS_PER_ROCK)");
    assert_eq!(idle_count, 2, "Remaining 2 workers should stay idle due to saturation limit");
}

#[test]
fn test_idle_worker_base_hq_proximity_constraint() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    // Friendly base at (0, -1000)
    world.spawn((
        Transform::from_xyz(0.0, -1000.0, 1.0),
        Faction::Player1,
        RoomId(1),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
    ));

    // Expansion mineral node at (800, -750) - distance to Base HQ is ~838px (> 380px radius)
    let expansion_rock = world.spawn((
        Transform::from_xyz(800.0, -750.0, 0.5),
        ResourceNode::new(2000),
        NetEntity { net_id: 20, owner_peer_id: 0 },
        RoomId(1),
    )).id();

    // Worker stationed at (750, -750) - only 50px from expansion rock, but expansion has no Base HQ
    let worker_ent = world.spawn((
        Transform::from_xyz(750.0, -750.0, 2.0),
        MoveSpeed(WORKER_MOVE_SPEED),
        Faction::Player1,
        RoomId(1),
        Worker::default(),
    )).id();

    app.update();

    let worker = app.world().get::<Worker>(worker_ent).unwrap();
    assert_eq!(worker.state, WorkerState::Idle, "Worker should not auto-mine rock without friendly Base HQ within 380px");
    assert_eq!(worker.target_node, None);

    // Now establish an expansion Base HQ at (850, -750) (distance 50px <= 380px)
    app.world_mut().spawn((
        Transform::from_xyz(850.0, -750.0, 1.0),
        Faction::Player1,
        RoomId(1),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
    ));

    app.update();

    let worker_after = app.world().get::<Worker>(worker_ent).unwrap();
    assert_eq!(worker_after.state, WorkerState::MovingToResource, "Worker should now auto-mine rock with Base HQ nearby");
    assert_eq!(worker_after.target_node, Some(expansion_rock));
}

#[test]
fn test_depleted_rock_workers_fallback_to_idle() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    // Friendly base at (0, -1000)
    world.spawn((
        Transform::from_xyz(0.0, -1000.0, 1.0),
        Faction::Player1,
        RoomId(1),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
    ));

    // Mineral node that is depleted (0 remaining minerals)
    let depleted_rock = world.spawn((
        Transform::from_xyz(0.0, -1200.0, 0.5),
        ResourceNode { remaining_minerals: 0, max_minerals: 2000 },
        NetEntity { net_id: 10, owner_peer_id: 0 },
        RoomId(1),
    )).id();

    // Worker currently in MovingToResource state targeting the depleted rock
    let worker_ent = world.spawn((
        Transform::from_xyz(0.0, -1050.0, 2.0),
        MoveSpeed(WORKER_MOVE_SPEED),
        Faction::Player1,
        RoomId(1),
        Worker {
            state: WorkerState::MovingToResource,
            target_node: Some(depleted_rock),
            ..default()
        },
    )).id();

    app.update();

    let worker = app.world().get::<Worker>(worker_ent).unwrap();
    assert_eq!(worker.state, WorkerState::Idle, "Worker targeting depleted rock must fallback to Idle");
    assert_eq!(worker.target_node, None);
}

#[test]
fn test_manual_harvest_respects_5_worker_saturation_limit() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let (_tx_in, rx_in) = crossbeam_channel::unbounded();
    let (tx_out, mut rx_out) = tokio::sync::mpsc::unbounded_channel();
    app.insert_resource(ServerNetworkChannels {
        rx_incoming: rx_in,
        tx_outgoing: tx_out,
    });

    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    mm.rooms.insert(1, room);
    mm.players.insert(101, PlayerSession {
        peer_id: 101,
        name: "Player 1".to_string(),
        room_id: 1,
        faction: Faction::Player1,
        color: FactionColor::Blue,
        platform: ClientPlatform::Desktop,
    });
    app.insert_resource(mm);

    let world = app.world_mut();

    let rock_ent = world.spawn((
        ResourceNode::new(2000),
        NetEntity { net_id: 50, owner_peer_id: 0 },
        Radius(36.0),
        RoomId(1),
        Transform::from_xyz(0.0, -1200.0, 1.0),
    )).id();

    // 4 workers already mining rock 50
    for i in 0..4 {
        world.spawn((
            Unit { name: "Worker".to_string(), supply_cost: 1 },
            Faction::Player1,
            RoomId(1),
            NetEntity { net_id: 10 + i, owner_peer_id: 101 },
            Worker {
                state: WorkerState::Mining,
                target_node: Some(rock_ent),
                ..default()
            },
            Transform::from_xyz(0.0, -1190.0, 2.0),
        ));
    }

    // 3 new workers to order
    for i in 0..3 {
        let net_id = 20 + i;
        world.spawn((
            Unit { name: "Worker".to_string(), supply_cost: 1 },
            Faction::Player1,
            RoomId(1),
            NetEntity { net_id, owner_peer_id: 101 },
            Worker::default(),
            Transform::from_xyz(0.0, -1000.0, 2.0),
        ));
    }

    // Run handle_harvest via an exclusive system
    let harvest_system = move |mut commands: Commands,
                              channels: Res<ServerNetworkChannels>,
                              matchmaker: Res<Matchmaker>,
                              mut unit_query: crate::sim::commands::UnitQuery,
                              node_query: crate::sim::commands::NodeQuery| {
        crate::sim::commands::economy::handle_harvest(
            &mut commands,
            &channels,
            &matchmaker,
            &mut unit_query,
            &node_query,
            101,
            &[20, 21, 22],
            50,
        );
    };

    app.add_systems(Update, harvest_system);
    app.update();

    // Verify broadcast event only contained 1 worker (20), because 4 + 1 = 5 (cap)
    if let Ok(event) = rx_out.try_recv() {
        match event {
            OutgoingNetEvent::BroadcastToPeers { msg, .. } => match msg {
                ServerMessage::WorkersOrderedHarvest { worker_net_ids, .. } => {
                    assert_eq!(worker_net_ids.len(), 1, "Only 1 additional worker can be assigned before hitting 5 worker cap");
                    assert_eq!(worker_net_ids[0], 20);
                }
                _ => panic!("Expected WorkersOrderedHarvest message"),
            },
            _ => panic!("Expected BroadcastToPeers event"),
        }
    } else {
        panic!("Expected outgoing network event");
    }
}

#[test]
fn test_unconstructed_building_no_workers_stays_at_zero_progress() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    let barracks_e = world.spawn((
        Building::new("Barracks", Vec2::new(60.0, 60.0), 10.0, false),
        Health::new(500.0),
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 30, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -900.0, 1.0),
    )).id();

    // Advance 5 seconds with no workers present
    for _ in 0..5 {
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_secs(1));
        app.update();
    }

    let b = app.world().get::<Building>(barracks_e).unwrap();
    assert_eq!(b.build_timer, 0.0, "Building should not passively construct without workers");
    assert!(!b.is_constructed, "Building should remain unconstructed");
}

#[test]
fn test_idle_worker_auto_moves_to_and_constructs_building() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    let barracks_e = world.spawn((
        Building::new("Barracks", Vec2::new(60.0, 60.0), 2.0, false),
        Health::new(500.0),
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 30, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -950.0, 1.0),
    )).id();

    // Idle worker nearby
    let worker_e = world.spawn((
        Transform::from_xyz(0.0, -960.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        Worker::default(),
    )).id();

    // Tick 1: Worker detects uncompleted building and starts moving/building
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(100));
    app.world_mut().run_system_once(server_mining_system).unwrap();

    let w = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w.target_building, Some(barracks_e), "Idle worker must target unconstructed building");
    assert!(w.state == WorkerState::MovingToBuilding || w.state == WorkerState::Building);

    // Advance 5 seconds with 0.5s steps (duration is 2.0s)
    for _ in 0..10 {
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(500));
        app.world_mut().run_system_once(server_mining_system).unwrap();
    }

    let b = app.world().get::<Building>(barracks_e).unwrap();
    assert!(b.is_constructed, "Building must be completed by worker");

    let w_after = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w_after.state, WorkerState::Idle, "Worker must return to Idle upon building completion");
    assert_eq!(w_after.target_building, None);
}

#[test]
fn test_zero_idle_workers_drafts_closest_miner() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    let rock_e = world.spawn((
        ResourceNode { remaining_minerals: 500, max_minerals: 500 },
        Radius(20.0),
        NetEntity { net_id: 10, owner_peer_id: 0 },
        RoomId(1),
        Transform::from_xyz(50.0, -1000.0, 1.0),
    )).id();

    // Miner actively mining the rock
    let mut mining_worker = Worker::default();
    mining_worker.state = WorkerState::Mining;
    mining_worker.target_node = Some(rock_e);

    let worker_e = world.spawn((
        Transform::from_xyz(50.0, -1000.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        mining_worker,
    )).id();

    // Now spawn unconstructed Barracks
    let barracks_e = world.spawn((
        Building::new("Barracks", Vec2::new(60.0, 60.0), 10.0, false),
        Health::new(500.0),
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 30, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -900.0, 1.0),
    )).id();

    // Run system tick: 0 idle workers exist, so closest miner must be drafted!
    {
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(100));
        app.update();
    }

    let w = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w.target_building, Some(barracks_e), "Miner should be drafted to construct building");
    assert_eq!(w.state, WorkerState::MovingToBuilding);
    assert_eq!(w.target_node, None, "Drafted worker must clear its mining target");
}

#[test]
fn test_collaborative_construction_two_workers_double_speed() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    let barracks_e = world.spawn((
        Building::new("Barracks", Vec2::new(60.0, 60.0), 10.0, false),
        Health::new(500.0),
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 30, owner_peer_id: 101 },
        Transform::from_xyz(0.0, 0.0, 1.0),
    )).id();

    // Worker 1 in Building state
    let mut w1 = Worker::default();
    w1.state = WorkerState::Building;
    w1.target_building = Some(barracks_e);
    world.spawn((
        Transform::from_xyz(0.0, 20.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        w1,
    ));

    // Worker 2 in Building state
    let mut w2 = Worker::default();
    w2.state = WorkerState::Building;
    w2.target_building = Some(barracks_e);
    world.spawn((
        Transform::from_xyz(0.0, -20.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        w2,
    ));

    // Advance by 1 second: with 2 workers, build_timer must advance by 2.0s!
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(1));
    app.world_mut().run_system_once(server_mining_system).unwrap();

    let b = app.world().get::<Building>(barracks_e).unwrap();
    assert!((b.build_timer - 2.0).abs() < 0.01, "2 workers should advance build_timer by 2.0s in 1.0s elapsed (got {})", b.build_timer);
}

#[test]
fn test_completed_building_transitions_workers_to_idle_and_mines() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    // Friendly base at (0, -1000)
    world.spawn((
        Transform::from_xyz(0.0, -1000.0, 1.0),
        Faction::Player1,
        RoomId(1),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
    ));

    // Gold rock at (50, -1000)
    let rock_e = world.spawn((
        ResourceNode { remaining_minerals: 500, max_minerals: 500 },
        Radius(20.0),
        NetEntity { net_id: 10, owner_peer_id: 0 },
        RoomId(1),
        Transform::from_xyz(50.0, -1000.0, 1.0),
    )).id();

    // Building almost done (build_timer = 9.9, duration = 10.0)
    let mut b = Building::new("Barracks", Vec2::new(60.0, 60.0), 10.0, false);
    b.build_timer = 9.9;
    let barracks_e = world.spawn((
        b,
        Health::new(500.0),
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 30, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -980.0, 1.0),
    )).id();

    // Worker in Building state
    let mut w = Worker::default();
    w.state = WorkerState::Building;
    w.target_building = Some(barracks_e);
    let worker_e = world.spawn((
        Transform::from_xyz(0.0, -990.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        w,
    )).id();

    // Advance 0.2s: building completes, worker becomes Idle
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(200));
    app.world_mut().run_system_once(server_mining_system).unwrap();

    let b_after = app.world().get::<Building>(barracks_e).unwrap();
    assert!(b_after.is_constructed, "Building must be finished");

    let w1 = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w1.state, WorkerState::Idle, "Worker must become idle upon building completion");

    // Next tick: Idle worker automatically targets nearest available rock
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(100));
    app.world_mut().run_system_once(server_mining_system).unwrap();

    let w2 = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w2.target_node, Some(rock_e), "Worker should auto-target nearby gold rock after building finishes");
    assert_eq!(w2.state, WorkerState::MovingToResource);
}

#[test]
fn test_damaged_building_triggers_idle_worker_auto_repair() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    // Friendly Base HQ constructed
    world.spawn((
        Building::new("Base HQ", Vec2::new(100.0, 100.0), 0.0, true),
        Health::new(1500.0),
        BaseHQ { supply_provided: 10, dropoff_radius: 70.0 },
        Radius(50.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 10, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -1000.0, 1.0),
    ));

    // Damaged friendly building (100 / 500 HP)
    let mut damaged_hp = Health::new(500.0);
    damaged_hp.current = 100.0;
    let bldg_e = world.spawn((
        Building::new("Barracks", Vec2::new(60.0, 60.0), 10.0, true),
        damaged_hp,
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 20, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -950.0, 1.0),
    )).id();

    // Idle worker nearby
    let worker_e = world.spawn((
        Transform::from_xyz(0.0, -960.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        Worker::default(),
    )).id();

    // Advance 0.1s: idle worker notices damaged building and starts moving to repair / repairing
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(100));
    app.world_mut().run_system_once(server_mining_system).unwrap();

    let w = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w.target_building, Some(bldg_e), "Worker must target damaged building for repair");
    assert!(
        w.state == WorkerState::MovingToRepair || w.state == WorkerState::Repairing,
        "Worker state should be MovingToRepair or Repairing, got {:?}",
        w.state
    );
}

#[test]
fn test_collaborative_repair_multiple_workers() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    let mut hp = Health::new(500.0);
    hp.current = 100.0;
    let bldg_e = world.spawn((
        Building::new("Barracks", Vec2::new(60.0, 60.0), 10.0, true),
        hp,
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 10, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -950.0, 1.0),
    )).id();

    // Two workers actively repairing
    let mut w1 = Worker::default();
    w1.state = WorkerState::Repairing;
    w1.target_building = Some(bldg_e);
    world.spawn((
        Transform::from_xyz(0.0, -955.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        w1,
    ));

    let mut w2 = Worker::default();
    w2.state = WorkerState::Repairing;
    w2.target_building = Some(bldg_e);
    world.spawn((
        Transform::from_xyz(0.0, -945.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        w2,
    ));

    // Advance 1.0s: 2 workers * 20 HP/s = 40 HP healed -> 100 + 40 = 140 HP
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(1));
    app.world_mut().run_system_once(server_mining_system).unwrap();

    let hp_after = app.world().get::<Health>(bldg_e).unwrap();
    assert!((hp_after.current - 140.0).abs() < 0.1, "Expected 140.0 HP, got {}", hp_after.current);
}

#[test]
fn test_repaired_building_full_hp_transitions_worker_to_idle() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    let mut hp = Health::new(500.0);
    hp.current = 495.0; // 5 HP away from max
    let bldg_e = world.spawn((
        Building::new("Barracks", Vec2::new(60.0, 60.0), 10.0, true),
        hp,
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 10, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -950.0, 1.0),
    )).id();

    let mut w = Worker::default();
    w.state = WorkerState::Repairing;
    w.target_building = Some(bldg_e);
    let worker_e = world.spawn((
        Transform::from_xyz(0.0, -955.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        w,
    )).id();

    // Advance 0.5s: 1 worker * 20 HP/s * 0.5s = 10 HP healed -> clamps to 500.0 max HP
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(500));
    app.world_mut().run_system_once(server_mining_system).unwrap();

    let hp_after = app.world().get::<Health>(bldg_e).unwrap();
    assert_eq!(hp_after.current, 500.0);

    let w_after = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w_after.state, WorkerState::Idle, "Worker must become idle once repair completes");
    assert_eq!(w_after.target_building, None);
}

#[test]
fn test_manual_override_prevents_worker_auto_tasking_and_drafting() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    let world = app.world_mut();

    // Unconstructed building nearby
    world.spawn((
        Building::new("Supply Depot", Vec2::new(40.0, 40.0), 10.0, false),
        Health::new(300.0),
        Radius(20.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 10, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -900.0, 1.0),
    ));

    // Damaged building nearby
    let mut hp = Health::new(500.0);
    hp.current = 100.0;
    world.spawn((
        Building::new("Barracks", Vec2::new(60.0, 60.0), 10.0, true),
        hp,
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 20, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -950.0, 1.0),
    ));

    // Worker with manual_override: true at a retreat position
    let mut worker = Worker::default();
    worker.state = WorkerState::Idle;
    worker.manual_override = true;
    let worker_e = world.spawn((
        Transform::from_xyz(0.0, -800.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        worker,
    )).id();

    // Advance time: worker should NOT auto-build, auto-mine, or auto-repair
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(1));
    app.world_mut().run_system_once(server_mining_system).unwrap();

    let w = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w.state, WorkerState::Idle, "Manual override worker must stay Idle");
    assert_eq!(w.target_building, None);
    assert_eq!(w.target_node, None);
    assert!(w.manual_override, "manual_override flag must remain true");
}

#[test]
fn test_handle_stop_and_repair_commands_clear_manual_override() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    mm.players.insert(
        101,
        PlayerSession {
            peer_id: 101,
            name: "Alice".to_string(),
            room_id: 1,
            faction: Faction::Player1,
            color: FactionColor::Blue,
            platform: ClientPlatform::Desktop,
        },
    );
    app.insert_resource(mm);

    let (_tx_in, rx_in) = crossbeam_channel::unbounded();
    let (tx_out, _rx_out) = tokio::sync::mpsc::unbounded_channel();
    app.insert_resource(ServerNetworkChannels {
        rx_incoming: rx_in,
        tx_outgoing: tx_out,
    });

    let world = app.world_mut();

    let bldg_e = world.spawn((
        Building::new("Barracks", Vec2::new(60.0, 60.0), 10.0, true),
        Health::new(500.0),
        Radius(30.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 10, owner_peer_id: 101 },
        Transform::from_xyz(0.0, -950.0, 1.0),
    )).id();

    let mut w = Worker::default();
    w.manual_override = true;
    let worker_e = world.spawn((
        Transform::from_xyz(0.0, -800.0, 2.0),
        MoveSpeed(100.0),
        Faction::Player1,
        RoomId(1),
        NetEntity { net_id: 50, owner_peer_id: 101 },
        w,
    )).id();

    // 1. Issue repair command via handle_repair inside run_system_once
    app.world_mut().run_system_once(
        |mut commands: Commands,
         channels: Res<ServerNetworkChannels>,
         matchmaker: Res<Matchmaker>,
         mut unit_query: crate::sim::commands::UnitQuery,
         bldg_query: crate::sim::commands::BuildingQuery| {
            crate::sim::commands::economy::handle_repair(
                &mut commands,
                &channels,
                &matchmaker,
                &mut unit_query,
                &bldg_query,
                101,
                &[50],
                10,
            );
        },
    ).unwrap();

    let w_after_repair = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w_after_repair.state, WorkerState::MovingToRepair);
    assert_eq!(w_after_repair.target_building, Some(bldg_e));
    assert!(!w_after_repair.manual_override, "Repair order must clear manual_override");

    // 2. Set manual_override back to true and issue stop
    app.world_mut().get_mut::<Worker>(worker_e).unwrap().manual_override = true;

    app.world_mut().run_system_once(
        |mut commands: Commands,
         channels: Res<ServerNetworkChannels>,
         matchmaker: Res<Matchmaker>,
         mut unit_query: crate::sim::commands::UnitQuery| {
            crate::sim::commands::movement::handle_stop(
                &mut commands,
                &channels,
                &matchmaker,
                &mut unit_query,
                101,
                &[50],
            );
        },
    ).unwrap();

    let w_after_stop = app.world().get::<Worker>(worker_e).unwrap();
    assert_eq!(w_after_stop.state, WorkerState::Idle);
    assert!(!w_after_stop.manual_override, "Stop order must clear manual_override");
}





