pub mod message_handler;
pub mod offline_spawner;
pub mod portal_bridge;

#[allow(unused_imports)]
pub use offline_spawner::spawn_standalone_offline_match;
pub use portal_bridge::poll_web_portal_launch_requests;

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use ewebsock::{Options, WsEvent, WsMessage, WsReceiver, WsSender};
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::NavGrid;
use shared::protocol::{
    decode_server_msg, encode_client_msg, ClientMessage, FactionColor, GameMode,
};

use crate::audio_sfx::SoundEffect;
use crate::chat::ChatLog;
use crate::particles::ParticleEvent;
use crate::AppState;
use message_handler::{
    handle_server_message, CameraQuery, CleanupQuery, EntityNetQuery, NodeQuery,
};

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
            .add_systems(OnEnter(AppState::Lobby), cleanup_on_lobby_enter)
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

pub fn cleanup_on_lobby_enter(
    mut commands: Commands,
    cleanup_query: Query<Entity, Or<(With<NetEntity>, With<Unit>, With<Building>, With<ResourceNode>)>>,
    mut camera_query: Query<&mut Transform, (With<Camera2d>, Without<NetEntity>, Without<Unit>, Without<Building>, Without<ResourceNode>)>,
    mut outcome_opt: Option<ResMut<MatchOutcome>>,
    mut countdown_opt: Option<ResMut<crate::ui::MatchCountdown>>,
    mut wave_ai_opt: Option<ResMut<bot_ai::WaveAiState>>,
    mut economy_opt: Option<ResMut<PlayerEconomy>>,
    mut stats_opt: Option<ResMut<crate::stats::MatchStats>>,
    mut placement_opt: Option<ResMut<crate::placement::PlacementState>>,
    mut attack_move_opt: Option<ResMut<crate::ui::AttackMovePending>>,
) {
    info!("🧹 [AppState::Lobby] Cleaning up match entities and resetting match state.");
    for ent in cleanup_query.iter() {
        commands.entity(ent).despawn_recursive();
    }
    for mut cam_tf in camera_query.iter_mut() {
        cam_tf.translation = Vec3::new(0.0, 0.0, 100.0);
    }
    if let Some(ref mut outcome) = outcome_opt {
        **outcome = MatchOutcome::InProgress;
    }
    if let Some(ref mut cd) = countdown_opt {
        cd.is_active = false;
    }
    if let Some(ref mut wave) = wave_ai_opt {
        wave.is_active = false;
    }
    if let Some(ref mut economy) = economy_opt {
        **economy = PlayerEconomy::new();
    }
    if let Some(ref mut stats) = stats_opt {
        **stats = crate::stats::MatchStats::default();
    }
    if let Some(ref mut placement) = placement_opt {
        **placement = crate::placement::PlacementState::default();
    }
    if let Some(ref mut attack_move) = attack_move_opt {
        attack_move.0 = false;
    }
}

pub fn connect_to_server_startup(
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

pub fn poll_network_events(
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
    cleanup_query: CleanupQuery,
    mut camera_query: CameraQuery,
    node_query: NodeQuery,
    mut entity_query: EntityNetQuery,
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

pub fn net_heartbeat_system(
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

pub fn net_reconnect_system(
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
