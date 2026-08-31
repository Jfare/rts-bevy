use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use ewebsock::{Options, WsEvent, WsMessage, WsReceiver, WsSender};
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::BuildingKind;
use shared::protocol::{
    decode_server_msg, encode_client_msg, ClientMessage, EntityKind, FactionColor, GameMode,
    ServerMessage, UnitKind,
};
use std::collections::HashMap;
use crate::chat::{ChatEntry, ChatLog};
use crate::pings::TacticalPingVisual;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    InLobby,
    InGame,
}

#[allow(dead_code)]
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ServerTelemetry {
    pub queue_1v1: u32,
    pub active_1v1_matches: u32,
    pub max_1v1_matches: u32,
    pub active_solo_matches: u32,
    pub max_solo_matches: u32,
    pub total_online: u32,
    pub last_updated_ms: u64,
}

/// Thread-safe client network state resource accessible across all systems
#[derive(Resource)]
pub struct NetClient {
    pub tx_outgoing_cmds: Sender<ClientMessage>,
    pub status: NetStatus,
    pub my_peer_id: u64,
    pub my_faction: Faction,
    pub my_color: FactionColor,
    pub player_name: String,
    pub current_room_code: Option<String>,
    pub current_mode: GameMode,
    pub server_url: String,
    pub ping_timer: Timer,
    pub last_ping_sent: u64,
    pub rtt_ms: u32,
    pub last_error_message: Option<String>,
}

impl Default for NetClient {
    fn default() -> Self {
        let (tx, _) = crossbeam_channel::unbounded();
        let server_url = get_default_ws_url();
        Self {
            tx_outgoing_cmds: tx,
            status: NetStatus::Disconnected,
            my_peer_id: 1,
            my_faction: Faction::Player1,
            my_color: FactionColor::Blue,
            player_name: "Commander".to_string(),
            current_room_code: None,
            current_mode: GameMode::SoloVsAi,
            server_url,
            ping_timer: Timer::from_seconds(2.0, TimerMode::Repeating),
            last_ping_sent: 0,
            rtt_ms: 0,
            last_error_message: None,
        }
    }
}

impl NetClient {
    pub fn send(&self, msg: &ClientMessage) {
        let _ = self.tx_outgoing_cmds.send(msg.clone());
    }
}

/// Non-Send resource holding platform-specific WebSocket handles (isolated to the main thread)
pub struct WsConnection {
    pub sender: Option<WsSender>,
    pub receiver: Option<WsReceiver>,
    pub rx_outgoing_cmds: Receiver<ClientMessage>,
}

fn get_default_ws_url() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            if let Ok(host) = win.location().host() {
                let proto = if win.location().protocol().unwrap_or_default() == "https:" {
                    "wss:"
                } else {
                    "ws:"
                };
                // If running on local Trunk port 8000, target server port 8080
                let ws_host = if host.contains(":8000") {
                    host.replace(":8000", ":8080")
                } else {
                    format!("{}/ws", host)
                };
                return format!("{}//{}", proto, ws_host);
            }
        }
        "ws://127.0.0.1:8080".to_string()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("RTS_SERVER_URL").unwrap_or_else(|_| "ws://127.0.0.1:8080".to_string())
    }
}

pub struct NetClientPlugin;

impl Plugin for NetClientPlugin {
    fn build(&self, app: &mut App) {
        let (tx_cmds, rx_cmds) = crossbeam_channel::unbounded();

        let mut net_client = NetClient::default();
        net_client.tx_outgoing_cmds = tx_cmds;
        app.insert_resource(net_client)
            .init_resource::<ServerTelemetry>();

        app.insert_non_send_resource(WsConnection {
            sender: None,
            receiver: None,
            rx_outgoing_cmds: rx_cmds,
        });

        app.add_systems(Startup, connect_to_server_startup)
            .add_systems(
                Update,
                (
                    poll_network_events,
                    net_heartbeat_system,
                    poll_web_portal_launch_requests,
                ),
            );
    }
}

#[allow(unused_mut, unused_variables)]
fn poll_web_portal_launch_requests(
    mut net_client: ResMut<NetClient>,
    mut wave_ai_opt: Option<ResMut<bot_ai::WaveAiState>>,
) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Ok(val) = js_sys::eval("window.__rts_requested_mode || ''") {
            if let Some(mode_str) = val.as_string() {
                if !mode_str.is_empty() {
                    let _ = js_sys::eval("window.__rts_requested_mode = null;");
                    if mode_str == "1v1" {
                        info!("⚔️ [Portal] Launching 1v1 Multiplayer matchmaking");
                        net_client.current_mode = GameMode::Multiplayer1v1;
                        if let Some(ref mut wave_ai) = wave_ai_opt {
                            wave_ai.is_active = false;
                        }
                        net_client.send(&ClientMessage::JoinLobby {
                            player_name: net_client.player_name.clone(),
                            mode: GameMode::Multiplayer1v1,
                            room_code: None,
                            faction_color: Some(net_client.my_color),
                        });
                    } else if mode_str == "solo" {
                        info!("🤖 [Portal] Launching Solo vs AI match");
                        net_client.current_mode = GameMode::SoloVsAi;
                        if let Some(ref mut wave_ai) = wave_ai_opt {
                            wave_ai.is_active = true;
                            wave_ai.time_until_next_wave = 40.0;
                            wave_ai.current_wave = 0;
                        }
                        net_client.send(&ClientMessage::JoinLobby {
                            player_name: net_client.player_name.clone(),
                            mode: GameMode::SoloVsAi,
                            room_code: None,
                            faction_color: Some(net_client.my_color),
                        });
                    }
                }
            }
        }
    }
}

fn connect_to_server_startup(
    mut net_client: ResMut<NetClient>,
    mut ws_conn: NonSendMut<WsConnection>,
) {
    let url = net_client.server_url.clone();
    info!("🌐 [NetClient] Connecting to RTS game server at {}...", url);
    net_client.status = NetStatus::Connecting;

    let options = Options::default();
    match ewebsock::connect_with_wakeup(&url, options, move || {}) {
        Ok((sender, receiver)) => {
            ws_conn.sender = Some(sender);
            ws_conn.receiver = Some(receiver);
            info!("✅ [NetClient] WebSocket stream initiated to {}", url);
        }
        Err(err) => {
            warn!("⚠️ [NetClient] Offline / Standalone mode active: {}", err);
            net_client.status = NetStatus::Disconnected;
        }
    }
}

fn poll_network_events(
    mut commands: Commands,
    time: Res<Time>,
    mut ws_conn: NonSendMut<WsConnection>,
    mut net_client: ResMut<NetClient>,
    mut economy: ResMut<PlayerEconomy>,
    mut outcome_opt: Option<ResMut<MatchOutcome>>,
    mut chat_log_opt: Option<ResMut<ChatLog>>,
    cleanup_query: Query<Entity, Or<(With<NetEntity>, With<Unit>, With<Building>, With<ResourceNode>)>>,
    mut camera_query: Query<&mut Transform, (With<Camera2d>, Without<NetEntity>, Without<Unit>, Without<Building>)>,
    mut entity_query: Query<(
        Entity,
        &NetEntity,
        &mut Transform,
        &mut Health,
        Option<&mut Worker>,
    )>,
) {
    let now_ms = time.elapsed().as_millis() as u64;

    // 1. Dispatch outgoing client commands over WebSocket
    let mut outgoing = Vec::new();
    while let Ok(msg) = ws_conn.rx_outgoing_cmds.try_recv() {
        outgoing.push(msg);
    }

    if let Some(ref mut sender) = ws_conn.sender {
        for msg in outgoing {
            if let Ok(bytes) = encode_client_msg(&msg) {
                sender.send(WsMessage::Binary(bytes));
            }
        }
    }

    // 2. Poll incoming network events from server
    let mut received_events = Vec::new();
    if let Some(ref receiver) = ws_conn.receiver {
        while let Some(event) = receiver.try_recv() {
            received_events.push(event);
        }
    }

    for event in received_events {
        match event {
            WsEvent::Opened => {
                info!("🟢 [NetClient] Connected to server (Standby / Lobby Ready)");
                net_client.status = NetStatus::Connected;
                net_client.send(&ClientMessage::RequestLobbyStats);
            }
            WsEvent::Message(WsMessage::Binary(bytes)) => {
                if let Ok(server_msg) = decode_server_msg(&bytes) {
                    handle_server_message(
                        &mut commands,
                        &mut net_client,
                        &mut economy,
                        &mut outcome_opt,
                        &mut chat_log_opt,
                        &cleanup_query,
                        &mut camera_query,
                        &mut entity_query,
                        now_ms,
                        server_msg,
                    );
                }
            }
            WsEvent::Closed => {
                warn!("🔴 [NetClient] WebSocket connection closed by server.");
                net_client.status = NetStatus::Disconnected;
            }
            WsEvent::Error(err) => {
                warn!("⚠️ [NetClient] WebSocket error: {}", err);
                net_client.status = NetStatus::Disconnected;
            }
            _ => {}
        }
    }
}

fn handle_server_message(
    commands: &mut Commands,
    net_client: &mut ResMut<NetClient>,
    economy: &mut ResMut<PlayerEconomy>,
    outcome_opt: &mut Option<ResMut<MatchOutcome>>,
    chat_log_opt: &mut Option<ResMut<ChatLog>>,
    cleanup_query: &Query<Entity, Or<(With<NetEntity>, With<Unit>, With<Building>, With<ResourceNode>)>>,
    camera_query: &mut Query<&mut Transform, (With<Camera2d>, Without<NetEntity>, Without<Unit>, Without<Building>)>,
    entity_query: &mut Query<(
        Entity,
        &NetEntity,
        &mut Transform,
        &mut Health,
        Option<&mut Worker>,
    )>,
    now_ms: u64,
    msg: ServerMessage,
) {
    match msg {
        ServerMessage::LobbyJoined {
            player_id,
            assigned_faction,
            room_id,
            room_code,
            is_game_ready,
        } => {
            net_client.my_peer_id = player_id;
            net_client.my_faction = assigned_faction;
            net_client.current_room_code = room_code.clone();
            net_client.status = if is_game_ready {
                NetStatus::InGame
            } else {
                NetStatus::InLobby
            };
            info!(
                "🎯 [NetClient] Joined Room #{} (Code: {:?}) as {:?} (Ready: {})",
                room_id, room_code, assigned_faction, is_game_ready
            );
        }
        ServerMessage::GameStarted { .. } => {
            net_client.status = NetStatus::InGame;
            info!("⚔️ [NetClient] Match started!");
        }
        ServerMessage::InitialWorldState {
            entities,
            p1_minerals,
            p1_supply: _,
            p1_max_supply: _,
            p2_minerals,
            p2_supply: _,
            p2_max_supply: _,
        } => {
            info!(
                "🌐 [NetClient] Received InitialWorldState with {} entities. Resetting match battlefield!",
                entities.len()
            );

            // 1. Despawn any existing battlefield entities (previous game or demo scene)
            for e in cleanup_query.iter() {
                commands.entity(e).despawn_recursive();
            }

            // 2. Reset match outcome to InProgress
            if let Some(ref mut outcome) = outcome_opt {
                **outcome = MatchOutcome::InProgress;
            }

            // 3. Center camera on local player's base
            let my_base_pos = if net_client.my_faction == Faction::Player1 {
                Vec2::new(-600.0, 250.0)
            } else {
                Vec2::new(600.0, -250.0)
            };
            for mut cam_tf in camera_query.iter_mut() {
                cam_tf.translation.x = my_base_pos.x;
                cam_tf.translation.y = my_base_pos.y;
            }

            // 4. Reset local economy
            let starting_min = if net_client.my_faction == Faction::Player1 {
                p1_minerals
            } else {
                p2_minerals
            };
            let cur_min = economy.get_minerals(net_client.my_faction);
            if cur_min != starting_min {
                if starting_min > cur_min {
                    economy.add_minerals(net_client.my_faction, starting_min - cur_min);
                } else {
                    economy.spend_minerals(net_client.my_faction, cur_min - starting_min);
                }
            }

            // 5. Authoritatively spawn all entities
            for ent in entities {
                match ent.kind {
                    EntityKind::ResourceNode => {
                        commands.spawn((
                            ResourceNode::new(1500),
                            Faction::Neutral,
                            Selectable::default(),
                            Radius(24.0),
                            NetEntity {
                                net_id: ent.net_id,
                                owner_peer_id: 0,
                            },
                            Transform::from_xyz(ent.position.x, ent.position.y, 0.5),
                        ));
                    }
                    EntityKind::Building(b_kind) => {
                        let mut entity_cmds = commands.spawn((
                            Building::new(
                                b_kind.name(),
                                b_kind.size(),
                                b_kind.build_duration(),
                                true,
                            ),
                            Health::new(ent.max_hp),
                            ent.faction,
                            Selectable::default(),
                            Radius(b_kind.size().x.max(b_kind.size().y) * 0.5),
                            NetEntity {
                                net_id: ent.net_id,
                                owner_peer_id: 0,
                            },
                            Transform::from_xyz(ent.position.x, ent.position.y, 1.0),
                        ));

                        match b_kind {
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
                                        rally_point: ent.position + Vec2::new(0.0, -100.0),
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
                                        rally_point: ent.position + Vec2::new(0.0, -100.0),
                                    },
                                ));
                            }
                            BuildingKind::SupplyDepot => {
                                entity_cmds.insert(SupplyDepot {
                                    supply_provided: 8,
                                });
                            }
                            BuildingKind::Turret => {
                                entity_cmds.insert(GunTurret::default());
                            }
                        }
                    }
                    EntityKind::Unit(u_kind) => {
                        let mut unit_cmds = commands.spawn((
                            Unit {
                                name: u_kind.name().to_string(),
                                supply_cost: u_kind.supply_cost(),
                            },
                            Health::new(ent.max_hp),
                            ent.faction,
                            Selectable::default(),
                            NetEntity {
                                net_id: ent.net_id,
                                owner_peer_id: 0,
                            },
                            Transform::from_xyz(ent.position.x, ent.position.y, 2.0),
                        ));

                        match u_kind {
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
                                        state: SoldierState::Idle,
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
                            UnitKind::Tank => {
                                unit_cmds.insert((
                                    SiegeTank::default(),
                                    Radius(22.0),
                                    MoveSpeed(140.0),
                                    Velocity::default(),
                                ));
                            }
                        }
                    }
                }
            }
        }
        ServerMessage::TickSnapshotBatch {
            snapshots,
            p1_minerals,
            p1_supply: _,
            p1_max_supply: _,
            p2_minerals: _,
            p2_supply: _,
            p2_max_supply: _,
            ..
        } => {
            // Index existing entities by Net ID
            let mut entity_map: HashMap<u32, (Entity, Mut<Transform>, Mut<Health>, Option<Mut<Worker>>)> =
                HashMap::new();

            for (entity, net_entity, transform, health, worker_opt) in entity_query.iter_mut() {
                entity_map.insert(net_entity.net_id, (entity, transform, health, worker_opt));
            }

            // Sync positions, health, and state from server snapshot only during active online matches
            if net_client.status == NetStatus::InGame {
                for snap in snapshots {
                    if let Some((_e, mut tf, mut hp, mut worker_opt)) = entity_map.remove(&snap.net_id) {
                        // Smooth lerp towards authoritative position
                        let target_pos = Vec3::new(snap.position.x, snap.position.y, tf.translation.z);
                        tf.translation = tf.translation.lerp(target_pos, 0.45);
                        tf.rotation = Quat::from_rotation_z(snap.rotation);

                        hp.current = snap.current_hp;
                        hp.max = snap.max_hp;

                        if let Some(ref mut worker) = worker_opt {
                            if snap.is_mining {
                                worker.state = WorkerState::Mining;
                            }
                        }
                    }
                }

                // Sync local player economy minerals
                if net_client.my_faction == Faction::Player1 {
                    let current = economy.get_minerals(Faction::Player1);
                    if current != p1_minerals {
                        let diff = p1_minerals as i64 - current as i64;
                        if diff > 0 {
                            economy.add_minerals(Faction::Player1, diff as u32);
                        } else if diff < 0 {
                            economy.spend_minerals(Faction::Player1, (-diff) as u32);
                        }
                    }
                }
            }
        }

        ServerMessage::BuildingSpawned {
            net_id,
            faction,
            building_kind,
            position,
            max_hp,
        } => {
            info!("🏗️ [NetClient] Server spawned building #{}: {:?}", net_id, building_kind);
            let mut entity_cmds = commands.spawn((
                Building::new(
                    building_kind.name(),
                    building_kind.size(),
                    building_kind.build_duration(),
                    false,
                ),
                Health::new(max_hp),
                faction,
                Selectable::default(),
                Radius(building_kind.size().x.max(building_kind.size().y) * 0.5),
                NetEntity {
                    net_id,
                    owner_peer_id: 0,
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
        }
        ServerMessage::UnitSpawned {
            net_id,
            faction,
            unit_kind,
            position,
            max_hp,
        } => {
            info!("🎖️ [NetClient] Server spawned unit #{}: {:?}", net_id, unit_kind);
            let mut unit_cmds = commands.spawn((
                Unit {
                    name: unit_kind.name().to_string(),
                    supply_cost: unit_kind.supply_cost(),
                },
                Health::new(max_hp),
                faction,
                Selectable::default(),
                NetEntity {
                    net_id,
                    owner_peer_id: 0,
                },
                Transform::from_xyz(position.x, position.y, 2.0),
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
                            state: SoldierState::Idle,
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
                UnitKind::Tank => {
                    unit_cmds.insert((
                        SiegeTank::default(),
                        Radius(22.0),
                        MoveSpeed(140.0),
                        Velocity::default(),
                    ));
                }
            }
        }

        ServerMessage::ProjectileFired {
            attacker_net_id: _,
            target_net_id: _,
            origin,
            target_pos,
            damage,
        } => {
            let diff = target_pos - origin;
            let dist = diff.length();
            let speed = 780.0;
            let lifetime = if dist > 0.0 { dist / speed } else { 0.1 };

            commands.spawn((
                Projectile {
                    origin,
                    target_entity: None,
                    target_pos,
                    speed,
                    damage,
                    splash_radius: 0.0,
                    faction: Faction::Neutral,
                    lifetime: 0.0,
                    max_lifetime: lifetime,
                },
                Transform::from_xyz(origin.x, origin.y, 3.0),
            ));

            commands.spawn((
                MuzzleFlash {
                    lifetime: 0.0,
                    max_lifetime: 0.07,
                    color: Color::srgb(1.0, 0.85, 0.35),
                },
                Transform::from_xyz(origin.x, origin.y, 3.1),
            ));
        }
        ServerMessage::EntityDamaged {
            target_net_id,
            current_hp,
            max_hp,
        } => {
            for (_, net_entity, _, mut hp, _) in entity_query.iter_mut() {
                if net_entity.net_id == target_net_id {
                    hp.current = current_hp;
                    hp.max = max_hp;
                }
            }
        }
        ServerMessage::EntityDied { net_id, .. } => {
            for (entity, net_entity, _, _, _) in entity_query.iter_mut() {
                if net_entity.net_id == net_id {
                    commands.entity(entity).despawn_recursive();
                }
            }
        }
        ServerMessage::MatchEnded { winning_faction, .. } => {
            if let Some(ref mut outcome) = outcome_opt {
                if winning_faction == net_client.my_faction {
                    **outcome = MatchOutcome::Victory;
                } else {
                    **outcome = MatchOutcome::Defeat;
                }
            }
        }
        ServerMessage::Pong {
            client_timestamp, ..
        } => {
            if now_ms >= client_timestamp {
                net_client.rtt_ms = (now_ms - client_timestamp) as u32;
            }
        }
        ServerMessage::LobbyStats {
            queue_1v1,
            active_1v1_matches,
            max_1v1_matches,
            active_solo_matches,
            max_solo_matches,
            total_online,
        } => {
            commands.insert_resource(ServerTelemetry {
                queue_1v1,
                active_1v1_matches,
                max_1v1_matches,
                active_solo_matches,
                max_solo_matches,
                total_online,
                last_updated_ms: now_ms,
            });
        }
        ServerMessage::ChatMessageReceived {
            sender_name,
            faction,
            color,
            text,
            is_system,
        } => {
            info!("💬 [Chat] {}: {}", sender_name, text);
            if let Some(ref mut chat_log) = chat_log_opt {
                chat_log.entries.push(ChatEntry {
                    sender_name,
                    faction,
                    color,
                    text,
                    is_system,
                    timestamp_ms: now_ms,
                });
                if chat_log.entries.len() > 100 {
                    chat_log.entries.remove(0);
                }
            }
        }
        ServerMessage::TacticalPingReceived {
            sender_name,
            faction: _,
            color,
            position,
            ping_type,
        } => {
            info!("📍 [Ping] {} pinged {:?} at {:?}", sender_name, ping_type, position);
            commands.spawn((
                TacticalPingVisual {
                    position,
                    ping_type,
                    color,
                    lifetime: 0.0,
                    max_lifetime: 3.5,
                },
                Transform::from_xyz(position.x, position.y, 4.0),
            ));
        }
        ServerMessage::ErrorMessage { reason } => {
            warn!("🛑 [NetClient] Server message: {}", reason);
            net_client.last_error_message = Some(reason);
        }
        _ => {}
    }
}

fn net_heartbeat_system(
    time: Res<Time>,
    mut net_client: ResMut<NetClient>,
) {
    net_client.ping_timer.tick(time.delta());
    if net_client.ping_timer.just_finished() && net_client.status != NetStatus::Disconnected {
        let now = time.elapsed().as_millis() as u64;
        net_client.last_ping_sent = now;
        net_client.send(&ClientMessage::Ping { timestamp: now });
    }
}
