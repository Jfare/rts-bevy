use bevy::prelude::*;
use shared::components::{AppState, Faction};
use shared::protocol::ClientMessage;

use crate::net::{NetClient, NetStatus, ServerTelemetry};
use super::{LobbyButtonAction, LobbyModalContainer, LobbyStatusText};

pub fn close_menu_on_game_start(mut modal_query: Query<&mut Node, With<LobbyModalContainer>>) {
    for mut node in &mut modal_query {
        node.display = Display::None;
    }
}

pub fn handle_lobby_button_interactions(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, &LobbyButtonAction),
        (Changed<Interaction>, With<Button>),
    >,
    mut modal_query: Query<&mut Node, With<LobbyModalContainer>>,
    mut net_client: ResMut<NetClient>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::F1) || keyboard.just_pressed(KeyCode::Tab) || keyboard.just_pressed(KeyCode::Escape) {
        for mut node in &mut modal_query {
            node.display = if node.display == Display::None {
                Display::Flex
            } else {
                Display::None
            };
        }
    }

    for (interaction, mut bg_color, action) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                match action {
                    LobbyButtonAction::ToggleModal => {
                        for mut node in &mut modal_query {
                            node.display = if node.display == Display::None {
                                Display::Flex
                            } else {
                                Display::None
                            };
                        }
                    }
                    LobbyButtonAction::CloseModal => {
                        for mut node in &mut modal_query {
                            node.display = Display::None;
                        }
                    }
                    LobbyButtonAction::ForfeitMatch => {
                        info!("🏳️ [GameMenu] Player forfeited match, returning to landing page.");
                        for mut node in &mut modal_query {
                            node.display = Display::None;
                        }
                        net_client.send(&ClientMessage::ForfeitMatch);
                        net_client.status = NetStatus::Connected;
                        next_state.set(AppState::Lobby);
                        #[cfg(target_arch = "wasm32")]
                        {
                            let _ = js_sys::eval("if (window.__rts_return_to_lobby) { window.__rts_return_to_lobby(); }");
                        }
                    }
                }
            }
            Interaction::Hovered => {
                bg_color.0 = match action {
                    LobbyButtonAction::ForfeitMatch => Color::srgba(0.55, 0.18, 0.18, 0.95),
                    LobbyButtonAction::CloseModal => Color::srgba(0.20, 0.40, 0.65, 0.95),
                    LobbyButtonAction::ToggleModal => Color::srgba(0.20, 0.35, 0.50, 0.95),
                };
            }
            Interaction::None => {
                bg_color.0 = match action {
                    LobbyButtonAction::ForfeitMatch => Color::srgba(0.40, 0.12, 0.12, 0.95),
                    LobbyButtonAction::CloseModal => Color::srgba(0.12, 0.28, 0.45, 0.95),
                    LobbyButtonAction::ToggleModal => Color::srgba(0.12, 0.22, 0.32, 0.95),
                };
            }
        }
    }
}

pub fn update_lobby_modal_status_text(
    net_client: Res<NetClient>,
    telemetry: Option<Res<ServerTelemetry>>,
    mut text_query: Query<&mut Text, With<LobbyStatusText>>,
) {
    let telem_str = if let Some(t) = telemetry {
        format!(
            " | 🌐 Online: {} | 👥 Queue: {} | ⚔️ 1v1: {}/{} | 🤖 Solo: {}/{}",
            t.total_online, t.queue_1v1, t.active_1v1_matches, t.max_1v1_matches, t.active_solo_matches, t.max_solo_matches
        )
    } else {
        String::new()
    };

    let room_code_str = if let Some(ref code) = net_client.current_room_code {
        format!(" | 🔑 Private Room Code: [{}]", code)
    } else {
        String::new()
    };

    for mut text in &mut text_query {
        match net_client.status {
            NetStatus::InGame => {
                let role = if net_client.my_faction == Faction::Player1 {
                    format!("Player 1 ({:?} / West Base)", net_client.my_color)
                } else {
                    format!("Player 2 ({:?} / East Base)", net_client.my_color)
                };
                text.0 = format!("🟢 Match in Progress! Assigned: {}{}{}", role, room_code_str, telem_str);
            }
            NetStatus::InLobby => {
                if let Some(ref code) = net_client.current_room_code {
                    text.0 = format!("🟡 Private Lobby [{}] created! Waiting for opponent to join...{}", code, telem_str);
                } else {
                    text.0 = format!("🟡 In Matchmaking Queue... Waiting for opponent (1/2){}", telem_str);
                }
            }
            NetStatus::Connected => {
                text.0 = format!("🟢 Connected to Battle Server{}", telem_str);
            }
            NetStatus::Connecting => {
                text.0 = "🟡 Connecting to Server...".to_string();
            }
            NetStatus::Disconnected => {
                text.0 = "⚪ Offline (Solo Skirmish Active)".to_string();
            }
        }
    }
}

