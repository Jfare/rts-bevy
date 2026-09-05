use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use ewebsock::{Options, WsEvent, WsMessage, WsReceiver, WsSender};
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::{BuildingKind, NavGrid};
use shared::protocol::{
    decode_server_msg, encode_client_msg, ClientMessage, EntityKind, EntityState, FactionColor, GameMode,
    ServerMessage, UnitKind,
};
use std::collections::HashMap;
use crate::audio_sfx::SoundEffect;
use crate::chat::{ChatEntry, ChatLog};
use crate::particles::ParticleEvent;
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
    pub pending_mode_request: Option<GameMode>,
    pub server_url: String,
    pub ping_timer: Timer,
    pub reconnect_timer: Timer,
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
            pending_mode_request: None,
            server_url,
            ping_timer: Timer::from_seconds(2.0, TimerMode::Repeating),
            reconnect_timer: Timer::from_seconds(2.5, TimerMode::Repeating),
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
    pub buffered_messages: Vec<ClientMessage>,
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

        let net_client = NetClient {
            tx_outgoing_cmds: tx_cmds,
            ..Default::default()
        };
        app.insert_resource(net_client)
            .init_resource::<ServerTelemetry>();

        app.insert_non_send_resource(WsConnection {
            sender: None,
            receiver: None,
            rx_outgoing_cmds: rx_cmds,
            buffered_messages: Vec::new(),
        });

        app.add_systems(Startup, connect_to_server_startup)
            .add_systems(
                Update,
                (
                    poll_network_events,
                    net_heartbeat_system,
                    net_reconnect_system,
                    poll_web_portal_launch_requests,
                ),
            );
    }
}

#[allow(unused_mut, unused_variables)]
fn poll_web_portal_launch_requests(
    mut commands: Commands,
    mut net_client: ResMut<NetClient>,
    mut economy: ResMut<PlayerEconomy>,
    mut wave_ai_opt: Option<ResMut<bot_ai::WaveAiState>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut camera_query: Query<&mut Transform, (With<Camera2d>, Without<NetEntity>, Without<Unit>, Without<Building>, Without<ResourceNode>)>,
    cleanup_query: Query<Entity, Or<(With<NetEntity>, With<Unit>, With<Building>, With<ResourceNode>)>>,
) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Ok(val) = js_sys::eval("window.__rts_cancel_queue || false") {
            if val.as_bool().unwrap_or(false) {
                let _ = js_sys::eval("window.__rts_cancel_queue = false;");
                info!("🚪 [Portal] Sending CancelQueue order to server");
                net_client.pending_mode_request = None;
                net_client.send(&ClientMessage::CancelQueue);
            }
        }

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
                        if net_client.status == NetStatus::Connecting {
                            info!("⏳ [Portal] WebSocket still connecting; buffering 1v1 matchmaking request.");
                            net_client.pending_mode_request = Some(GameMode::Multiplayer1v1);
                        } else {
                            net_client.send(&ClientMessage::JoinLobby {
                                player_name: net_client.player_name.clone(),
                                mode: GameMode::Multiplayer1v1,
                                room_code: None,
                                faction_color: Some(net_client.my_color),
                            });
                        }
                    } else if mode_str == "solo" {
                        info!("🤖 [Portal] Launching Solo vs AI match");
                        net_client.current_mode = GameMode::SoloVsAi;
                        if net_client.status == NetStatus::Connecting {
                            info!("⏳ [Portal] WebSocket connecting; buffering Solo vs AI match request.");
                            net_client.pending_mode_request = Some(GameMode::SoloVsAi);
                        } else if net_client.status == NetStatus::Disconnected {
                            info!("🤖 [Portal] Server offline; launching local standalone match.");
                            if let Some(ref mut wave_ai) = wave_ai_opt {
                                wave_ai.is_active = true;
                                wave_ai.time_until_next_wave = 40.0;
                                wave_ai.current_wave = 0;
                            }
                            spawn_standalone_offline_match(
                                &mut commands,
                                &mut economy,
                                wave_ai_opt.as_deref_mut(),
                                &mut camera_query,
                                &cleanup_query,
                            );
                            next_state.set(AppState::InGame);
                        } else {
                            if let Some(ref mut wave_ai) = wave_ai_opt {
                                wave_ai.is_active = false;
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
    nav_grid: Res<NavGrid>,
    mut ws_conn: NonSendMut<WsConnection>,
    mut net_client: ResMut<NetClient>,
    mut economy: ResMut<PlayerEconomy>,
    mut outcome_opt: Option<ResMut<MatchOutcome>>,
    mut chat_log_opt: Option<ResMut<ChatLog>>,
    mut countdown_opt: Option<ResMut<crate::ui::MatchCountdown>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut sound_events: EventWriter<SoundEffect>,
    mut particle_events: EventWriter<ParticleEvent>,
    cleanup_query: Query<Entity, Or<(With<NetEntity>, With<Unit>, With<Building>, With<ResourceNode>)>>,
    mut camera_query: Query<&mut Transform, (With<Camera2d>, Without<NetEntity>, Without<Unit>, Without<Building>, Without<ResourceNode>)>,
    node_query: Query<(Entity, &NetEntity, &Transform), (With<ResourceNode>, Without<Camera2d>, Without<Unit>, Without<Building>)>,
    mut entity_query: Query<(
        Entity,
        &NetEntity,
        &Faction,
        &mut Transform,
        &mut Health,
        Option<&mut Worker>,
        Option<&mut Soldier>,
        Option<&mut SiegeTank>,
        Option<&mut MoveTarget>,
        Option<&mut TacticalStance>,
        Option<&mut Stimpack>,
        Option<&Radius>,
        Option<&mut GunTurret>,
        Option<&mut ProductionBuilding>,
    ), (Without<Camera2d>, Without<ResourceNode>)>,
) {
    let now_ms = time.elapsed().as_millis() as u64;

    // 1. Buffer and dispatch outgoing client commands over WebSocket
    while let Ok(msg) = ws_conn.rx_outgoing_cmds.try_recv() {
        ws_conn.buffered_messages.push(msg);
    }

    if net_client.status != NetStatus::Connecting && net_client.status != NetStatus::Disconnected
        && ws_conn.sender.is_some() && !ws_conn.buffered_messages.is_empty() {
            let messages = std::mem::take(&mut ws_conn.buffered_messages);
            if let Some(ref mut sender) = ws_conn.sender {
                for msg in messages {
                    if let Ok(bytes) = encode_client_msg(&msg) {
                        sender.send(WsMessage::Binary(bytes));
                    }
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

                if let Some(mode) = net_client.pending_mode_request.take() {
                    info!("🚀 [NetClient] Dispatching pending match request: {:?}", mode);
                    net_client.current_mode = mode;
                    net_client.send(&ClientMessage::JoinLobby {
                        player_name: net_client.player_name.clone(),
                        mode,
                        room_code: None,
                        faction_color: Some(net_client.my_color),
                    });
                }
            }
            WsEvent::Message(WsMessage::Binary(bytes)) => {
                if let Ok(server_msg) = decode_server_msg(&bytes) {
                    handle_server_message(
                        &mut commands,
                        &nav_grid,
                        &mut net_client,
                        &mut economy,
                        &mut outcome_opt,
                        &mut chat_log_opt,
                        &mut countdown_opt,
                        &mut next_state,
                        &mut sound_events,
                        &mut particle_events,
                        &cleanup_query,
                        &mut camera_query,
                        &node_query,
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
    nav_grid: &NavGrid,
    net_client: &mut ResMut<NetClient>,
    economy: &mut ResMut<PlayerEconomy>,
    outcome_opt: &mut Option<ResMut<MatchOutcome>>,
    chat_log_opt: &mut Option<ResMut<ChatLog>>,
    countdown_opt: &mut Option<ResMut<crate::ui::MatchCountdown>>,
    next_state: &mut ResMut<NextState<AppState>>,
    sound_events: &mut EventWriter<SoundEffect>,
    particle_events: &mut EventWriter<ParticleEvent>,
    cleanup_query: &Query<Entity, Or<(With<NetEntity>, With<Unit>, With<Building>, With<ResourceNode>)>>,
    camera_query: &mut Query<&mut Transform, (With<Camera2d>, Without<NetEntity>, Without<Unit>, Without<Building>, Without<ResourceNode>)>,
    node_query: &Query<(Entity, &NetEntity, &Transform), (With<ResourceNode>, Without<Camera2d>, Without<Unit>, Without<Building>)>,
    entity_query: &mut Query<(
        Entity,
        &NetEntity,
        &Faction,
        &mut Transform,
        &mut Health,
        Option<&mut Worker>,
        Option<&mut Soldier>,
        Option<&mut SiegeTank>,
        Option<&mut MoveTarget>,
        Option<&mut TacticalStance>,
        Option<&mut Stimpack>,
        Option<&Radius>,
        Option<&mut GunTurret>,
        Option<&mut ProductionBuilding>,
    ), (Without<Camera2d>, Without<ResourceNode>)>,
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
                "🚪 [NetClient] Assigned Faction: {:?}, Room #{}, Code: {:?}, Status: {:?}",
                assigned_faction, room_id, room_code, net_client.status
            );
        }
        ServerMessage::GameStarted {
            p1_pos,
            p2_pos,
            wave_initial_delay: _,
        } => {
            info!("⚔️ [NetClient] Match started! Initializing battlefield cameras.");
            net_client.status = NetStatus::InGame;
            next_state.set(AppState::InGame);

            // Center camera on player spawn base
            let spawn_pos = if net_client.my_faction == Faction::Player1 {
                p1_pos
            } else {
                p2_pos
            };

            for mut cam_tf in camera_query.iter_mut() {
                cam_tf.translation.x = spawn_pos.x;
                cam_tf.translation.y = spawn_pos.y;
            }
        }
        ServerMessage::InitialWorldState {
            entities,
            p1_minerals,
            p1_supply,
            p1_max_supply,
            p2_minerals,
            p2_supply,
            p2_max_supply,
        } => {
            info!("🌍 [NetClient] Initializing authoritative world state from server ({} entities).", entities.len());

            // Clear previous match entities
            for ent in cleanup_query.iter() {
                commands.entity(ent).despawn_recursive();
            }

            // Sync starting minerals and supply
            let my_minerals = if net_client.my_faction == Faction::Player1 {
                p1_minerals
            } else {
                p2_minerals
            };
            let (my_supply, my_max_supply) = if net_client.my_faction == Faction::Player1 {
                (p1_supply, p1_max_supply)
            } else {
                (p2_supply, p2_max_supply)
            };
            economy.set_supply(net_client.my_faction, my_supply, my_max_supply);

            let cur_min = economy.get_minerals(net_client.my_faction);
            if cur_min != my_minerals {
                if my_minerals > cur_min {
                    economy.add_minerals(net_client.my_faction, my_minerals - cur_min);
                } else {
                    economy.spend_minerals(net_client.my_faction, cur_min - my_minerals);
                }
            }

            // Spawn authoritative entities
            let mut mineral_nodes: Vec<(Entity, Vec2)> = Vec::new();
            let mut pending_units: Vec<(EntityState, UnitKind)> = Vec::new();
            let mut pending_buildings: Vec<(EntityState, BuildingKind)> = Vec::new();

            for ent_state in entities {
                match ent_state.kind {
                    EntityKind::ResourceNode => {
                        let pos = ent_state.position;
                        let node_e = commands.spawn((
                            ResourceNode::new(1500),
                            Faction::Neutral,
                            Selectable::default(),
                            Radius(24.0),
                            NetEntity {
                                net_id: ent_state.net_id,
                                owner_peer_id: 0,
                            },
                            Transform::from_xyz(pos.x, pos.y, 0.5),
                        )).id();
                        mineral_nodes.push((node_e, pos));
                    }
                    EntityKind::Building(kind) => {
                        pending_buildings.push((ent_state, kind));
                    }
                    EntityKind::Unit(kind) => {
                        pending_units.push((ent_state, kind));
                    }
                }
            }

            for (ent_state, kind) in pending_buildings {
                let pos = ent_state.position;
                let mut b_cmds = commands.spawn((
                    Building::new(
                        kind.name(),
                        kind.size(),
                        kind.build_duration(),
                        true,
                    ),
                    Health::new(ent_state.max_hp),
                    ent_state.faction,
                    Selectable::default(),
                    Radius(kind.size().x.max(kind.size().y) * 0.5),
                    NetEntity {
                        net_id: ent_state.net_id,
                        owner_peer_id: 0,
                    },
                    Transform::from_xyz(pos.x, pos.y, 1.0),
                ));

                match kind {
                    BuildingKind::BaseHQ => {
                        b_cmds.insert((
                            BaseHQ {
                                supply_provided: 10,
                                dropoff_radius: 70.0,
                            },
                            ProductionBuilding {
                                queue: Vec::new(),
                                current_timer: 0.0,
                                max_queue_size: 5,
                                rally_point: pos + Vec2::new(0.0, -100.0),
                            },
                        ));
                    }
                    BuildingKind::Barracks => {
                        b_cmds.insert((
                            Barracks,
                            ProductionBuilding {
                                queue: Vec::new(),
                                current_timer: 0.0,
                                max_queue_size: 5,
                                rally_point: pos + Vec2::new(0.0, -100.0),
                            },
                        ));
                    }
                    BuildingKind::SupplyDepot => {
                        b_cmds.insert(SupplyDepot { supply_provided: 8 });
                    }
                    BuildingKind::Turret => {
                        b_cmds.insert(GunTurret::default());
                    }
                }
            }

            for (ent_state, kind) in pending_units {
                let pos = ent_state.position;
                let mut u_cmds = commands.spawn((
                    Unit {
                        name: kind.name().to_string(),
                        supply_cost: kind.supply_cost(),
                    },
                    Health::new(ent_state.max_hp),
                    ent_state.faction,
                    Selectable::default(),
                    NetEntity {
                        net_id: ent_state.net_id,
                        owner_peer_id: 0,
                    },
                    Transform::from_xyz(pos.x, pos.y, 2.0),
                ));

                match kind {
                    UnitKind::Worker => {
                        let closest_node = mineral_nodes.iter().min_by(|(_, a), (_, b)| {
                            let da = pos.distance(*a);
                            let db = pos.distance(*b);
                            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                        }).map(|(e, _)| *e);

                        u_cmds.insert((
                            Worker {
                                state: WorkerState::MovingToResource,
                                target_node: closest_node,
                                ..default()
                            },
                            Radius(14.0),
                            MoveSpeed(190.0),
                            Velocity::default(),
                        ));
                    }
                    UnitKind::Soldier => {
                        u_cmds.insert((
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
                        u_cmds.insert((
                            SiegeTank::default(),
                            Radius(22.0),
                            MoveSpeed(140.0),
                            Velocity::default(),
                        ));
                    }
                }
            }
        }
        ServerMessage::TickSnapshotBatch {
            snapshots,
            p1_minerals,
            p1_supply,
            p1_max_supply,
            p2_minerals,
            p2_supply,
            p2_max_supply,
            ..
        } => {
            // Authoritatively synchronize economy bank & supply from server tick snapshot
            let (my_minerals, my_cur_sup, my_max_sup) = if net_client.my_faction == Faction::Player1 {
                (p1_minerals, p1_supply, p1_max_supply)
            } else {
                (p2_minerals, p2_supply, p2_max_supply)
            };

            let cur_min = economy.get_minerals(net_client.my_faction);
            if cur_min != my_minerals {
                if my_minerals > cur_min {
                    economy.add_minerals(net_client.my_faction, my_minerals - cur_min);
                } else {
                    economy.spend_minerals(net_client.my_faction, cur_min - my_minerals);
                }
            }
            economy.set_supply(net_client.my_faction, my_cur_sup, my_max_sup);

            // Index existing entities by Net ID for Health, Mining, and deadband position reconciliation
            let mut entity_map: HashMap<u32, (Entity, Mut<Transform>, Mut<Health>, Option<Mut<Worker>>, bool)> = HashMap::new();

            for (entity, net_entity, _faction, transform, health, worker_opt, _, _, move_target_opt, ..) in entity_query.iter_mut() {
                let has_move = move_target_opt.is_some();
                entity_map.insert(net_entity.net_id, (entity, transform, health, worker_opt, has_move));
            }

            // Sync health, worker mining state, and deadband positions from server snapshot
            if net_client.status == NetStatus::InGame {
                for snap in snapshots {
                    if let Some((entity, mut tf, mut hp, mut worker_opt, has_move)) = entity_map.remove(&snap.net_id) {
                        if snap.current_hp <= 0.0 {
                            commands.entity(entity).despawn_recursive();
                            continue;
                        }

                        hp.current = snap.current_hp;
                        hp.max = snap.max_hp;

                        if let Some(ref mut worker) = worker_opt {
                            if snap.is_mining {
                                worker.state = WorkerState::Mining;
                            }
                        }

                        let cur_pos = tf.translation.truncate();
                        let dist = cur_pos.distance(snap.position);

                        if dist > 2.0 {
                            let target_3d = Vec3::new(snap.position.x, snap.position.y, tf.translation.z);
                            if dist > 25.0 {
                                tf.translation = target_3d;
                            } else if !has_move {
                                tf.translation = tf.translation.lerp(target_3d, 0.40);
                            } else {
                                tf.translation = tf.translation.lerp(target_3d, 0.20);
                            }
                        }
                    }
                }
            }
        }

        ServerMessage::UnitsOrderedMove {
            unit_net_ids,
            destinations,
            is_attack_move,
        } => {
            for (net_id, dest) in unit_net_ids.into_iter().zip(destinations) {
                for (entity, net_entity, _fac, tf, _hp, _worker, soldier_opt, tank_opt, move_target_opt, stance_opt, ..) in entity_query.iter_mut() {
                    if net_entity.net_id == net_id {
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
                            commands.entity(entity).insert(MoveTarget::with_waypoints(dest, is_attack_move, waypoints));
                        }
                        break;
                    }
                }
            }
        }

        ServerMessage::UnitsOrderedAttackTarget {
            unit_net_ids,
            target_net_id,
        } => {
            let target_entity = entity_query
                .iter()
                .find(|(_, net_entity, ..)| net_entity.net_id == target_net_id)
                .map(|(e, ..)| e);

            if let Some(target_e) = target_entity {
                for (entity, net_entity, _fac, _tf, _hp, _worker, soldier_opt, tank_opt, ..) in entity_query.iter_mut() {
                    if unit_net_ids.contains(&net_entity.net_id) {
                        commands.entity(entity).remove::<MoveTarget>();
                        if let Some(mut soldier) = soldier_opt {
                            soldier.target = Some(target_e);
                            soldier.state = SoldierState::ChasingTarget;
                        }
                        if let Some(mut tank) = tank_opt {
                            tank.target = Some(target_e);
                        }
                    }
                }
            }
        }

        ServerMessage::WorkersOrderedHarvest {
            worker_net_ids,
            resource_net_id,
        } => {
            let target_node = node_query
                .iter()
                .find(|(_, net_entity, _)| net_entity.net_id == resource_net_id)
                .map(|(e, _, _)| e);

            if let Some(node_e) = target_node {
                for (entity, net_entity, _fac, _tf, _hp, worker_opt, ..) in entity_query.iter_mut() {
                    if worker_net_ids.contains(&net_entity.net_id) {
                        commands.entity(entity).remove::<MoveTarget>();
                        if let Some(mut worker) = worker_opt {
                            worker.target_node = Some(node_e);
                            worker.state = WorkerState::MovingToResource;
                            worker.harvest_timer = 0.0;
                        }
                    }
                }
            }
        }

        ServerMessage::UnitsOrderedStop { unit_net_ids } => {
            for (entity, net_entity, _fac, _tf, _hp, worker_opt, soldier_opt, mut tank_opt, _, stance_opt, ..) in entity_query.iter_mut() {
                if unit_net_ids.contains(&net_entity.net_id) {
                    commands.entity(entity).remove::<MoveTarget>();
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
                }
            }
        }

        ServerMessage::UnitsOrderedHoldPosition { unit_net_ids } => {
            for (entity, net_entity, _fac, _tf, _hp, _worker, soldier_opt, mut tank_opt, _, stance_opt, ..) in entity_query.iter_mut() {
                if unit_net_ids.contains(&net_entity.net_id) {
                    commands.entity(entity).remove::<MoveTarget>();
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
                        commands.entity(entity).insert(TacticalStance::HoldPosition);
                    }
                }
            }
        }

        ServerMessage::UnitsOrderedPatrol {
            unit_net_ids,
            destinations,
        } => {
            for (net_id, dest) in unit_net_ids.into_iter().zip(destinations) {
                for (entity, net_entity, _fac, tf, _hp, _worker, soldier_opt, _tank, move_target_opt, stance_opt, ..) in entity_query.iter_mut() {
                    if net_entity.net_id == net_id {
                        let unit_pos = tf.translation.truncate();
                        let waypoints = nav_grid.find_path(unit_pos, dest);

                        if let Some(mut stance) = stance_opt {
                            *stance = TacticalStance::Patrol {
                                origin: unit_pos,
                                target: dest,
                                heading_to_target: true,
                            };
                        } else {
                            commands.entity(entity).insert(TacticalStance::Patrol {
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
                            commands.entity(entity).insert(MoveTarget::with_waypoints(dest, true, waypoints));
                        }
                        break;
                    }
                }
            }
        }

        ServerMessage::UnitsActivatedStimpack { unit_net_ids } => {
            for (entity, net_entity, _fac, tf, mut hp, _worker, _soldier, _tank, _mt, _stance, stim_opt, ..) in entity_query.iter_mut() {
                if unit_net_ids.contains(&net_entity.net_id)
                    && hp.current > 20.0 {
                        hp.take_damage(15.0);
                        let pos = tf.translation.truncate();
                        particle_events.send(ParticleEvent::StimpackVapor { pos });
                        if let Some(mut stim) = stim_opt {
                            stim.is_active = true;
                            stim.timer = 6.0;
                        } else {
                            commands.entity(entity).insert(Stimpack {
                                is_active: true,
                                timer: 6.0,
                                duration: 6.0,
                            });
                        }
                    }
            }
        }

        ServerMessage::UnitsToggledSiegeMode { unit_net_ids } => {
            for (entity, net_entity, _fac, tf, _hp, _worker, _soldier, tank_opt, ..) in entity_query.iter_mut() {
                if unit_net_ids.contains(&net_entity.net_id) {
                    if let Some(mut tank) = tank_opt {
                        let pos = tf.translation.truncate();
                        match tank.mode {
                            TankMode::Tank => {
                                tank.mode = TankMode::TransformingToSiege;
                                tank.transform_timer = 1.0;
                                commands.entity(entity).remove::<MoveTarget>();
                                particle_events.send(ParticleEvent::Shockwave {
                                    pos,
                                    radius: 45.0,
                                    color: Color::srgba(1.0, 0.6, 0.2, 0.8),
                                });
                            }
                            TankMode::Siege => {
                                tank.mode = TankMode::TransformingToTank;
                                tank.transform_timer = 1.0;
                                particle_events.send(ParticleEvent::Shockwave {
                                    pos,
                                    radius: 30.0,
                                    color: Color::srgba(0.4, 0.8, 1.0, 0.8),
                                });
                            }
                            _ => {}
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
                        TacticalStance::default(),
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
        }

        ServerMessage::QueueUpdated {
            building_net_id,
            queue_count,
            current_progress,
        } => {
            for (_e, net_entity, _fac, _tf, _hp, _worker, _soldier, _tank, _move, _stance, _stim, _rad, _turret, mut prod_opt) in entity_query.iter_mut() {
                if net_entity.net_id == building_net_id {
                    if let Some(ref mut prod) = prod_opt {
                        while prod.queue.len() > queue_count {
                            prod.queue.remove(0);
                        }
                        if !prod.queue.is_empty() {
                            let total_dur = prod.queue[0].build_duration;
                            prod.current_timer = current_progress * total_dur;
                        } else {
                            prod.current_timer = 0.0;
                        }
                    }
                }
            }
        }

        ServerMessage::ProjectileFired {
            attacker_net_id,
            target_net_id,
            origin,
            target_pos,
            damage,
        } => {
            // Find target's client position if present
            let target_client_pos = entity_query
                .iter()
                .find(|(_, net, ..)| net.net_id == target_net_id)
                .map(|(_, _, _, tf, ..)| tf.translation.truncate())
                .unwrap_or(target_pos);

            // Find attacker on client to ensure muzzle origin and barrel orientation are visually accurate
            let mut attacker_found = false;
            for (_e, net_entity, fac, mut tf, _hp, _worker, soldier_opt, mut tank_opt, _, _, _, rad_opt, mut turret_opt, ..) in entity_query.iter_mut() {
                if net_entity.net_id == attacker_net_id {
                    attacker_found = true;
                    let attacker_pos = tf.translation.truncate();
                    let diff = target_client_pos - attacker_pos;
                    let dir = diff.normalize_or_zero();
                    let angle = dir.y.atan2(dir.x);
                    let attacker_fac = *fac;
                    let rad = rad_opt.map(|r| r.0).unwrap_or(16.0);

                    let is_siege = tank_opt.as_ref().map(|t| t.mode == TankMode::Siege).unwrap_or(false);
                    let is_tank = tank_opt.is_some();
                    let is_turret = turret_opt.is_some();

                    // Orient attacker / turret towards target
                    if dir.length_squared() > 0.001 {
                        if soldier_opt.is_some() {
                            tf.rotation = Quat::from_rotation_z(angle);
                        }
                        if let Some(ref mut tank) = tank_opt {
                            tank.turret_angle = angle;
                        }
                        if let Some(ref mut turret) = turret_opt {
                            turret.barrel_angle = angle;
                        }
                    }

                    // Calculate muzzle start point aligned with visual barrel
                    let muzzle_start = if is_tank {
                        let muzzle_dist = if is_siege { rad * 2.2 } else { rad * 1.6 };
                        attacker_pos + dir * muzzle_dist
                    } else if is_turret {
                        attacker_pos + dir * 28.0
                    } else {
                        attacker_pos + dir * (rad + 8.0)
                    };

                    let to_target = target_client_pos - muzzle_start;
                    let dist = to_target.length();
                    let speed = if is_siege { 600.0 } else if is_turret { 850.0 } else { 780.0 };
                    let lifetime = if dist > 0.0 { dist / speed } else { 0.1 };

                    commands.spawn((
                        Projectile {
                            origin: muzzle_start,
                            target_entity: None,
                            target_pos: target_client_pos,
                            speed,
                            damage,
                            splash_radius: if is_siege { 45.0 } else { 0.0 },
                            faction: attacker_fac,
                            lifetime: 0.0,
                            max_lifetime: lifetime,
                        },
                        Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.0),
                    ));

                    commands.spawn((
                        MuzzleFlash {
                            lifetime: 0.0,
                            max_lifetime: if is_siege { 0.16 } else { 0.08 },
                            color: if is_siege { Color::srgb(1.0, 0.4, 0.1) } else { Color::srgb(1.0, 0.9, 0.3) },
                        },
                        Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.5),
                    ));

                    particle_events.send(ParticleEvent::MuzzleSmoke {
                        pos: muzzle_start,
                        dir,
                    });

                    if is_siege {
                        particle_events.send(ParticleEvent::Shockwave {
                            pos: muzzle_start,
                            radius: 25.0,
                            color: Color::srgba(1.0, 0.6, 0.2, 0.8),
                        });
                        sound_events.send(SoundEffect::SiegeTankShot);
                    } else {
                        sound_events.send(SoundEffect::Gunshot);
                    }
                    break;
                }
            }

            if !attacker_found {
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

                if damage >= 30.0 {
                    sound_events.send(SoundEffect::SiegeTankShot);
                } else {
                    sound_events.send(SoundEffect::Gunshot);
                }
            }
        }
        ServerMessage::EntityDamaged {
            target_net_id,
            current_hp,
            max_hp,
        } => {
            for (_, net_entity, _, _, mut hp, ..) in entity_query.iter_mut() {
                if net_entity.net_id == target_net_id {
                    hp.current = current_hp;
                    hp.max = max_hp;
                }
            }
        }
        ServerMessage::EntityDied { net_id, .. } => {
            for (entity, net_entity, ..) in entity_query.iter_mut() {
                if net_entity.net_id == net_id {
                    commands.entity(entity).despawn_recursive();
                    sound_events.send(SoundEffect::Explosion);
                    break;
                }
            }
        }
        ServerMessage::MatchEnded { winning_faction, .. } => {
            if let Some(ref mut outcome) = outcome_opt {
                if winning_faction == net_client.my_faction {
                    **outcome = MatchOutcome::Victory;
                    sound_events.send(SoundEffect::Victory);
                } else {
                    **outcome = MatchOutcome::Defeat;
                    sound_events.send(SoundEffect::Defeat);
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
        ServerMessage::MatchFound {
            opponent_name,
            opponent_color,
            countdown_seconds,
        } => {
            info!(
                "⚔️ [NetClient] Match Found vs [{}] ({:?})! Countdown: {:.1}s",
                opponent_name, opponent_color, countdown_seconds
            );
            sound_events.send(SoundEffect::CountdownBeep);

            #[cfg(target_arch = "wasm32")]
            {
                let js_call = format!(
                    "if (window.__rts_on_match_found) {{ window.__rts_on_match_found('{}', '{}', {:.1}); }}",
                    opponent_name.replace('\'', "\\'"),
                    opponent_color.name(),
                    countdown_seconds
                );
                let _ = js_sys::eval(&js_call);
            }

            if let Some(ref mut countdown) = countdown_opt {
                countdown.is_active = true;
                countdown.remaining_seconds = countdown_seconds;
                countdown.opponent_name = opponent_name.clone();
                countdown.opponent_color = opponent_color;
                countdown.last_announced_second = (countdown_seconds.ceil() as i32) + 1;
                countdown.has_played_go_sound = false;
            }
        }
        ServerMessage::QueueCancelled => {
            info!("🚪 [NetClient] Queue Cancelled acknowledged by server.");
            net_client.status = NetStatus::Connected;
            next_state.set(AppState::Lobby);
            #[cfg(target_arch = "wasm32")]
            {
                let _ = js_sys::eval("if (window.__rts_on_queue_cancelled) { window.__rts_on_queue_cancelled(); }");
            }
        }
        ServerMessage::ErrorMessage { reason } => {
            warn!("🛑 [NetClient] Server message: {}", reason);
            net_client.last_error_message = Some(reason);
        }
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

fn net_reconnect_system(
    time: Res<Time>,
    mut net_client: ResMut<NetClient>,
    mut ws_conn: NonSendMut<WsConnection>,
) {
    if net_client.status == NetStatus::Disconnected {
        net_client.reconnect_timer.tick(time.delta());
        if net_client.reconnect_timer.just_finished() {
            let url = net_client.server_url.clone();
            info!("🔄 [NetClient] Attempting automatic reconnection to {}...", url);
            net_client.status = NetStatus::Connecting;
            let options = Options::default();
            match ewebsock::connect_with_wakeup(&url, options, move || {}) {
                Ok((sender, receiver)) => {
                    ws_conn.sender = Some(sender);
                    ws_conn.receiver = Some(receiver);
                }
                Err(err) => {
                    warn!("⚠️ [NetClient] Reconnect failed: {}", err);
                    net_client.status = NetStatus::Disconnected;
                }
            }
        }
    }
}

/// Spawns a local offline game scene for Solo vs AI skirmish when running disconnected from server
#[allow(dead_code)]
pub fn spawn_standalone_offline_match(
    commands: &mut Commands,
    economy: &mut ResMut<PlayerEconomy>,
    mut wave_ai_opt: Option<&mut bot_ai::WaveAiState>,
    camera_query: &mut Query<&mut Transform, (With<Camera2d>, Without<NetEntity>, Without<Unit>, Without<Building>, Without<ResourceNode>)>,
    cleanup_query: &Query<Entity, Or<(With<NetEntity>, With<Unit>, With<Building>, With<ResourceNode>)>>,
) {
    info!("🤖 [Offline] Initializing standalone local Solo vs AI match");
    for ent in cleanup_query.iter() {
        commands.entity(ent).despawn_recursive();
    }

    // Reset economy
    let cur_min = economy.get_minerals(Faction::Player1);
    if cur_min != 200 {
        if cur_min < 200 {
            economy.add_minerals(Faction::Player1, 200 - cur_min);
        } else {
            economy.spend_minerals(Faction::Player1, cur_min - 200);
        }
    }
    economy.set_supply(Faction::Player1, 2, 10);

    // Setup wave AI
    if let Some(ref mut wave_ai) = wave_ai_opt {
        wave_ai.is_active = true;
        wave_ai.current_wave = 0;
        wave_ai.time_until_next_wave = 40.0;
        wave_ai.ai_spawn_pos = shared::map::P2_BASE_POS;
        wave_ai.target_player_pos = shared::map::P1_BASE_POS;
    }

    // Center camera
    for mut cam_tf in camera_query.iter_mut() {
        cam_tf.translation.x = shared::map::P1_BASE_POS.x;
        cam_tf.translation.y = shared::map::P1_BASE_POS.y;
    }

    let p1_pos = shared::map::P1_BASE_POS;

    // Spawn P1 Base HQ
    commands.spawn((
        Building::new("Base HQ", Vec2::new(110.0, 110.0), 5.0, true),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
        ProductionBuilding {
            queue: Vec::new(),
            current_timer: 0.0,
            max_queue_size: 5,
            rally_point: p1_pos + Vec2::new(0.0, 100.0),
        },
        Health::new(1500.0),
        Faction::Player1,
        Selectable::default(),
        Radius(55.0),
        NetEntity { net_id: 1, owner_peer_id: 1 },
        Transform::from_xyz(p1_pos.x, p1_pos.y, 1.0),
    ));

    // Spawn P1 Minerals
    let mut p1_primary_mineral_e = None;
    for (i, &min_pos) in shared::map::P1_MAIN_MINERALS.iter().enumerate() {
        let e = commands.spawn((
            ResourceNode::new(1500),
            Faction::Neutral,
            Selectable::default(),
            Radius(24.0),
            NetEntity { net_id: 2 + i as u32, owner_peer_id: 0 },
            Transform::from_xyz(min_pos.x, min_pos.y, 0.5),
        )).id();
        if p1_primary_mineral_e.is_none() {
            p1_primary_mineral_e = Some(e);
        }
    }

    // Spawn P1 SCVs (2 workers auto-harvesting at start)
    for (i, &pos) in shared::map::P1_STARTER_WORKERS.iter().enumerate() {
        commands.spawn((
            Unit { name: "SCV Worker".to_string(), supply_cost: 1 },
            Worker {
                state: WorkerState::MovingToResource,
                target_node: p1_primary_mineral_e,
                ..default()
            },
            Health::new(80.0),
            Faction::Player1,
            Selectable::default(),
            Radius(14.0),
            MoveSpeed(190.0),
            Velocity::default(),
            NetEntity { net_id: 10 + i as u32, owner_peer_id: 1 },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));
    }

    // Spawn Hostile AI Base (North)
    let ai_pos = shared::map::P2_BASE_POS;
    commands.spawn((
        Building::new("Hostile Base HQ", Vec2::new(110.0, 110.0), 5.0, true),
        BaseHQ {
            supply_provided: 10,
            dropoff_radius: 70.0,
        },
        Health::new(1500.0),
        Faction::HostileAi,
        Selectable::default(),
        Radius(55.0),
        NetEntity { net_id: 100, owner_peer_id: 2 },
        Transform::from_xyz(ai_pos.x, ai_pos.y, 1.0),
    ));

    // Spawn Hostile AI Minerals
    let mut ai_primary_mineral_e = None;
    for (i, &min_pos) in shared::map::P2_MAIN_MINERALS.iter().enumerate() {
        let e = commands.spawn((
            ResourceNode::new(1500),
            Faction::Neutral,
            Selectable::default(),
            Radius(24.0),
            NetEntity { net_id: 101 + i as u32, owner_peer_id: 0 },
            Transform::from_xyz(min_pos.x, min_pos.y, 0.5),
        )).id();
        if ai_primary_mineral_e.is_none() {
            ai_primary_mineral_e = Some(e);
        }
    }

    // Spawn Hostile AI SCVs (2 workers auto-harvesting at start)
    for (i, &pos) in shared::map::P2_STARTER_WORKERS.iter().enumerate() {
        commands.spawn((
            Unit { name: "SCV Worker".to_string(), supply_cost: 1 },
            Worker {
                state: WorkerState::MovingToResource,
                target_node: ai_primary_mineral_e,
                ..default()
            },
            Health::new(80.0),
            Faction::HostileAi,
            Selectable::default(),
            Radius(14.0),
            MoveSpeed(190.0),
            Velocity::default(),
            NetEntity { net_id: 110 + i as u32, owner_peer_id: 2 },
            Transform::from_xyz(pos.x, pos.y, 2.0),
        ));
    }

    // Spawn Expansion Minerals
    let all_expansions = [
        &shared::map::P1_NATURAL_EXPANSION_MINERALS[..],
        &shared::map::P2_NATURAL_EXPANSION_MINERALS[..],
        &shared::map::CONTESTED_WEST_MINERALS[..],
        &shared::map::CONTESTED_EAST_MINERALS[..],
    ];

    let mut net_id_counter = 200;
    for exp_cluster in all_expansions {
        for &exp_pos in exp_cluster {
            commands.spawn((
                ResourceNode::new(1500),
                Faction::Neutral,
                Selectable::default(),
                Radius(24.0),
                NetEntity { net_id: net_id_counter, owner_peer_id: 0 },
                Transform::from_xyz(exp_pos.x, exp_pos.y, 0.5),
            ));
            net_id_counter += 1;
        }
    }
}
