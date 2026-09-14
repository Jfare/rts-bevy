use bevy::prelude::*;
use bevy::ui::FocusPolicy;
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
                    LobbyButtonAction::ToggleFullscreen => {
                        #[cfg(target_arch = "wasm32")]
                        {
                            let _ = js_sys::eval("if (window.__rts_toggle_fullscreen) { window.__rts_toggle_fullscreen(); }");
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
                    LobbyButtonAction::ToggleFullscreen => Color::srgba(0.18, 0.42, 0.55, 0.95),
                };
            }
            Interaction::None => {
                bg_color.0 = match action {
                    LobbyButtonAction::ForfeitMatch => Color::srgba(0.40, 0.12, 0.12, 0.95),
                    LobbyButtonAction::CloseModal => Color::srgba(0.12, 0.28, 0.45, 0.95),
                    LobbyButtonAction::ToggleModal => Color::srgba(0.12, 0.22, 0.32, 0.95),
                    LobbyButtonAction::ToggleFullscreen => Color::srgba(0.10, 0.25, 0.38, 0.95),
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

/// Spawns the in-game modal menu (controls summary, match status, resume, and forfeit)
pub fn spawn_game_menu_modal(parent: &mut ChildBuilder) {
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                margin: UiRect {
                    left: Val::Px(-240.0),
                    top: Val::Px(-200.0),
                    right: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                },
                width: Val::Px(480.0),
                padding: UiRect::all(Val::Px(24.0)),
                border: UiRect::all(Val::Px(2.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(14.0),
                display: Display::None,
                ..default()
            },
            BorderRadius::all(Val::Px(8.0)),
            BackgroundColor(Color::srgba(0.06, 0.09, 0.14, 0.98)),
            BorderColor(Color::srgb(0.30, 0.75, 1.0)),
            LobbyModalContainer,
        ))
        .with_children(|modal| {
            modal.spawn((
                Text::new("⚙️ GAME MENU"),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(0.35, 0.85, 1.0)),
                FocusPolicy::Pass,
            ));

            modal.spawn((
                Text::new("Match is in progress. Review game status and controls, resume, or forfeit."),
                TextFont {
                    font_size: 12.5,
                    ..default()
                },
                TextColor(Color::srgb(0.70, 0.78, 0.85)),
                FocusPolicy::Pass,
            ));

            // Match status card
            modal.spawn((
                Text::new("Status: Match Active"),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.80, 0.92, 1.0)),
                LobbyStatusText,
                FocusPolicy::Pass,
            ));

            // Quick Controls summary container
            modal
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(12.0)),
                        row_gap: Val::Px(6.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BorderRadius::all(Val::Px(6.0)),
                    BackgroundColor(Color::srgba(0.08, 0.12, 0.18, 0.95)),
                    BorderColor(Color::srgb(0.20, 0.35, 0.50)),
                    FocusPolicy::Pass,
                ))
                .with_children(|guide| {
                    guide.spawn((
                        Text::new("🎮 CONTROLS & COMMANDS"),
                        TextFont {
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.35, 0.85, 1.0)),
                        FocusPolicy::Pass,
                    ));
                    guide.spawn((
                        Text::new("• Select Units: Left-Click / Drag Selection Box\n• Issue Orders: Right-Click (Move / Attack / Harvest)\n• Unit Tactics: [S] Stop | [H] Hold Position\n• Production: [V] Worker | [R] Ranged | [F] Melee\n• Structures: [B] Build Menu (HQ, Barracks, Supply Depot, Turret)\n• Game Menu: [Tab] / [F1] / [Esc]"),
                        TextFont {
                            font_size: 11.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.75, 0.85, 0.95)),
                        FocusPolicy::Pass,
                    ));
                });

            // Action Buttons (Resume & Forfeit)
            modal
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(10.0),
                        margin: UiRect::top(Val::Px(8.0)),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ))
                .with_children(|actions| {
                    // Resume Button
                    actions
                        .spawn((
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(1.5)),
                                ..default()
                            },
                            BorderRadius::all(Val::Px(6.0)),
                            BackgroundColor(Color::srgba(0.12, 0.28, 0.45, 0.95)),
                            BorderColor(Color::srgb(0.35, 0.85, 1.0)),
                            LobbyButtonAction::CloseModal,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("▶ RESUME MATCH"),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                                FocusPolicy::Pass,
                            ));
                        });

                    // Toggle Fullscreen Button
                    actions
                        .spawn((
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(1.5)),
                                ..default()
                            },
                            BorderRadius::all(Val::Px(6.0)),
                            BackgroundColor(Color::srgba(0.10, 0.25, 0.38, 0.95)),
                            BorderColor(Color::srgb(0.25, 0.65, 0.85)),
                            LobbyButtonAction::ToggleFullscreen,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("⛶ TOGGLE FULLSCREEN"),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.85, 0.95, 1.0)),
                                FocusPolicy::Pass,
                            ));
                        });

                    // Forfeit & Return to Landing Page Button
                    actions
                        .spawn((
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(1.5)),
                                ..default()
                            },
                            BorderRadius::all(Val::Px(6.0)),
                            BackgroundColor(Color::srgba(0.40, 0.12, 0.12, 0.95)),
                            BorderColor(Color::srgb(0.95, 0.30, 0.30)),
                            LobbyButtonAction::ForfeitMatch,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("🏳️ FORFEIT & QUIT TO LANDING PAGE"),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 0.90, 0.90)),
                                FocusPolicy::Pass,
                            ));
                        });
                });
        });
}


