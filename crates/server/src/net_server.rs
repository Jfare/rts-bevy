use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use futures_util::{SinkExt, StreamExt};
use shared::protocol::{decode_client_msg, encode_server_msg, ClientMessage, ServerMessage};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

static NEXT_PEER_ID: AtomicU64 = AtomicU64::new(1);

pub static TELEMETRY_QUEUE: AtomicU32 = AtomicU32::new(0);
pub static TELEMETRY_ACTIVE_1V1: AtomicU32 = AtomicU32::new(0);
pub static TELEMETRY_ACTIVE_SOLO: AtomicU32 = AtomicU32::new(0);
pub static TELEMETRY_TOTAL_ONLINE: AtomicU32 = AtomicU32::new(0);

pub fn update_global_telemetry(q: u32, a1: u32, aso: u32, tot: u32) {
    TELEMETRY_QUEUE.store(q, Ordering::Relaxed);
    TELEMETRY_ACTIVE_1V1.store(a1, Ordering::Relaxed);
    TELEMETRY_ACTIVE_SOLO.store(aso, Ordering::Relaxed);
    TELEMETRY_TOTAL_ONLINE.store(tot, Ordering::Relaxed);
}

#[derive(Debug)]
pub enum IncomingNetEvent {
    PeerConnected { peer_id: u64, addr: SocketAddr },
    PeerDisconnected { peer_id: u64 },
    MessageReceived { peer_id: u64, msg: ClientMessage },
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum OutgoingNetEvent {
    SendToPeer { peer_id: u64, msg: ServerMessage },
    Broadcast { msg: ServerMessage },
    BroadcastToPeers { peer_ids: Vec<u64>, msg: ServerMessage },
}


#[derive(Resource)]
pub struct ServerNetworkChannels {
    pub rx_incoming: Receiver<IncomingNetEvent>,
    pub tx_outgoing: mpsc::UnboundedSender<OutgoingNetEvent>,
}

pub struct ServerNetworkPlugin {
    pub port: u16,
}

impl Default for ServerNetworkPlugin {
    fn default() -> Self {
        let port = std::env::var("PORT")
            .or_else(|_| std::env::var("RTS_PORT"))
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);
        Self { port }
    }
}

impl Plugin for ServerNetworkPlugin {
    fn build(&self, app: &mut App) {
        let (tx_incoming, rx_incoming) = crossbeam_channel::unbounded();
        let (tx_outgoing, rx_outgoing) = mpsc::unbounded_channel();

        app.insert_resource(ServerNetworkChannels {
            rx_incoming,
            tx_outgoing,
        });

        let port = self.port;
        // Start background Tokio thread
        std::thread::Builder::new()
            .name("rts-server-net".to_string())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("Failed to initialize Tokio runtime for RTS server");

                rt.block_on(async move {
                    run_network_server(port, tx_incoming, rx_outgoing).await;
                });
            })
            .expect("Failed to spawn RTS network server thread");
    }
}

async fn run_network_server(
    port: u16,
    tx_incoming: Sender<IncomingNetEvent>,
    mut rx_outgoing: mpsc::UnboundedReceiver<OutgoingNetEvent>,
) {
    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => {
            println!("🌐 [WebSocket Server] Listening on ws://{}", addr);
            l
        }
        Err(err) => {
            eprintln!("❌ [WebSocket Server] Failed to bind to {}: {}", addr, err);
            return;
        }
    };

    let peers: Arc<tokio::sync::Mutex<HashMap<u64, mpsc::UnboundedSender<ServerMessage>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    // Task to dispatch outgoing messages from ECS to WebSocket clients
    let peers_outgoing = peers.clone();
    tokio::spawn(async move {
        while let Some(event) = rx_outgoing.recv().await {
            let peers_guard = peers_outgoing.lock().await;
            match event {
                OutgoingNetEvent::SendToPeer { peer_id, msg } => {
                    if let Some(sender) = peers_guard.get(&peer_id) {
                        let _ = sender.send(msg);
                    }
                }
                OutgoingNetEvent::Broadcast { msg } => {
                    for sender in peers_guard.values() {
                        let _ = sender.send(msg.clone());
                    }
                }
                OutgoingNetEvent::BroadcastToPeers { peer_ids, msg } => {
                    for id in peer_ids {
                        if let Some(sender) = peers_guard.get(&id) {
                            let _ = sender.send(msg.clone());
                        }
                    }
                }
            }
        }
    });

    // Accept loop
    loop {
        match listener.accept().await {
            Ok((stream, client_addr)) => {
                let peer_id = NEXT_PEER_ID.fetch_add(1, Ordering::Relaxed);
                println!(
                    "🔌 [WebSocket Server] Incoming connection #{} from {}",
                    peer_id, client_addr
                );

                let tx_incoming = tx_incoming.clone();
                let peers = peers.clone();

                tokio::spawn(async move {
                    handle_connection(peer_id, client_addr, stream, tx_incoming, peers).await;
                });
            }
            Err(err) => {
                eprintln!("⚠️ [WebSocket Server] Error accepting connection: {}", err);
            }
        }
    }
}

async fn handle_connection(
    peer_id: u64,
    addr: SocketAddr,
    mut stream: TcpStream,
    tx_incoming: Sender<IncomingNetEvent>,
    peers: Arc<tokio::sync::Mutex<HashMap<u64, mpsc::UnboundedSender<ServerMessage>>>>,
) {
    let mut peek_buf = [0u8; 512];
    let n = stream.peek(&mut peek_buf).await.unwrap_or(0);
    let peek_str = String::from_utf8_lossy(&peek_buf[..n]);

    if peek_str.starts_with("GET /health") {
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\nContent-Length: 15\r\n\r\n{\"status\":\"ok\"}";
        let _ = stream.write_all(resp.as_bytes()).await;
        let _ = stream.flush().await;
        return;
    }

    if peek_str.starts_with("GET /api/stats") || peek_str.starts_with("GET /stats") {
        let q = TELEMETRY_QUEUE.load(Ordering::Relaxed);
        let a1 = TELEMETRY_ACTIVE_1V1.load(Ordering::Relaxed);
        let aso = TELEMETRY_ACTIVE_SOLO.load(Ordering::Relaxed);
        let tot = TELEMETRY_TOTAL_ONLINE.load(Ordering::Relaxed);
        let body = format!(
            "{{\"queue_1v1\":{},\"active_1v1_matches\":{},\"max_1v1_matches\":10,\"active_solo_matches\":{},\"max_solo_matches\":10,\"total_online\":{},\"status\":\"online\"}}",
            q, a1, aso, tot
        );
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(resp.as_bytes()).await;
        let _ = stream.flush().await;
        return;
    }

    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(err) => {
            eprintln!(
                "❌ [WebSocket Server] Handshake failed for peer #{}: {}",
                peer_id, err
            );
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (tx_peer_out, mut rx_peer_out) = mpsc::unbounded_channel::<ServerMessage>();

    {
        let mut peers_guard = peers.lock().await;
        peers_guard.insert(peer_id, tx_peer_out);
    }

    let _ = tx_incoming.send(IncomingNetEvent::PeerConnected { peer_id, addr });

    // Outgoing write loop for this connection
    let write_handle = tokio::spawn(async move {
        while let Some(msg) = rx_peer_out.recv().await {
            if let Ok(bytes) = encode_server_msg(&msg) {
                if let Err(err) = ws_sender.send(Message::Binary(bytes.into())).await {
                    eprintln!("⚠️ [WebSocket Server] Write error peer #{}: {}", peer_id, err);
                    break;
                }
            }
        }
    });

    // Incoming read loop for this connection
    while let Some(msg_result) = ws_receiver.next().await {
        match msg_result {
            Ok(Message::Binary(bytes)) => match decode_client_msg(&bytes) {
                Ok(client_msg) => {
                    let _ = tx_incoming.send(IncomingNetEvent::MessageReceived {
                        peer_id,
                        msg: client_msg,
                    });
                }
                Err(err) => {
                    eprintln!(
                        "⚠️ [WebSocket Server] Decode error from peer #{}: {}",
                        peer_id, err
                    );
                }
            },
            Ok(Message::Ping(data)) => {
                // Tungstenite handles pong automatically
                let _ = data;
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Err(err) => {
                eprintln!("⚠️ [WebSocket Server] Read error peer #{}: {}", peer_id, err);
                break;
            }
            _ => {}
        }
    }

    write_handle.abort();
    {
        let mut peers_guard = peers.lock().await;
        peers_guard.remove(&peer_id);
    }

    println!("🔌 [WebSocket Server] Peer #{} disconnected", peer_id);
    let _ = tx_incoming.send(IncomingNetEvent::PeerDisconnected { peer_id });
}
