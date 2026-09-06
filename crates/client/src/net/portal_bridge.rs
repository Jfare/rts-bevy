use bevy::prelude::*;
use shared::components::*;
use shared::economy::PlayerEconomy;
#[cfg(target_arch = "wasm32")]
use shared::protocol::{ClientMessage, GameMode};

use crate::AppState;
#[cfg(target_arch = "wasm32")]
use super::offline_spawner::spawn_standalone_offline_match;
use super::NetClient;
#[cfg(target_arch = "wasm32")]
use super::NetStatus;

#[allow(unused_mut, unused_variables)]
pub fn poll_web_portal_launch_requests(
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
