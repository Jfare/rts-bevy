use super::*;
use crate::net_server::{IncomingNetEvent, OutgoingNetEvent, ServerNetworkChannels};
use crate::session::{Matchmaker, PlayerSession, Room};
use shared::economy::PlayerEconomy;
use shared::grid::NavGrid;
use shared::protocol::{ClientMessage, FactionColor, GameMode, PingType, ServerMessage};

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

