use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use shared::components::{AppState, Faction};
use shared::protocol::ClientMessage;

use crate::net::{NetClient, NetStatus, ServerTelemetry};
use super::{LobbyButtonAction, LobbyModalContainer, LobbyStatusText};

/// Marker component for the dialog card within the menu modal backdrop
#[derive(Component)]
pub struct LobbyModalDialog;

/// Marker component for the title text in the menu modal
#[derive(Component)]
pub struct LobbyModalTitleText;

/// Marker component for the subtitle text in the menu modal (hidden on mobile landscape)
#[derive(Component)]
pub struct LobbyModalSubtitleText;

/// Marker component for the controls summary box
#[derive(Component)]
pub struct LobbyModalControlsBox;

/// Marker component for the header inside the controls box
#[derive(Component)]
pub struct LobbyModalControlsHeader;

/// Marker component for the descriptive text inside the controls box
#[derive(Component)]
pub struct LobbyModalControlsText;

/// Marker component for the row of action buttons
#[derive(Component)]
pub struct LobbyModalActionsBox;

/// Marker component for the text on the fullscreen action button
#[derive(Component)]
pub struct FullscreenButtonText;

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
    #[allow(unused_mut)]
    mut _window_query: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
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
                            let _ = js_sys::eval(r#"
                                if (window.__rts_is_fullscreen && window.__rts_is_fullscreen()) {
                                    if (window.__rts_exit_fullscreen) {
                                        window.__rts_exit_fullscreen();
                                    } else if (window.__rts_toggle_fullscreen) {
                                        window.__rts_toggle_fullscreen();
                                    }
                                } else if (window.__rts_toggle_fullscreen) {
                                    window.__rts_toggle_fullscreen();
                                }
                            "#);
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            for mut window in &mut _window_query {
                                window.mode = match window.mode {
                                    bevy::window::WindowMode::Windowed => bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current),
                                    _ => bevy::window::WindowMode::Windowed,
                                };
                            }
                        }
                    }
                    LobbyButtonAction::ExitFullscreen => {
                        #[cfg(target_arch = "wasm32")]
                        {
                            let _ = js_sys::eval(r#"
                                if (window.__rts_exit_fullscreen) {
                                    window.__rts_exit_fullscreen();
                                } else if (window.__rts_toggle_fullscreen) {
                                    window.__rts_toggle_fullscreen();
                                }
                            "#);
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            for mut window in &mut _window_query {
                                window.mode = bevy::window::WindowMode::Windowed;
                            }
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
                    LobbyButtonAction::ToggleFullscreen | LobbyButtonAction::ExitFullscreen => {
                        Color::srgba(0.18, 0.42, 0.55, 0.95)
                    }
                };
            }
            Interaction::None => {
                bg_color.0 = match action {
                    LobbyButtonAction::ForfeitMatch => Color::srgba(0.40, 0.12, 0.12, 0.95),
                    LobbyButtonAction::CloseModal => Color::srgba(0.12, 0.28, 0.45, 0.95),
                    LobbyButtonAction::ToggleModal => Color::srgba(0.12, 0.22, 0.32, 0.95),
                    LobbyButtonAction::ToggleFullscreen | LobbyButtonAction::ExitFullscreen => {
                        Color::srgba(0.10, 0.25, 0.38, 0.95)
                    }
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
        format!(" | 🔑 Room: [{}]", code)
    } else {
        String::new()
    };

    for mut text in &mut text_query {
        match net_client.status {
            NetStatus::InGame => {
                let role = if net_client.my_faction == Faction::Player1 {
                    format!("Player 1 ({:?})", net_client.my_color)
                } else {
                    format!("Player 2 ({:?})", net_client.my_color)
                };
                text.0 = format!("🟢 Match Active | {}{}{}", role, room_code_str, telem_str);
            }
            NetStatus::InLobby => {
                if let Some(ref code) = net_client.current_room_code {
                    text.0 = format!("🟡 Private Lobby [{}] waiting...{}", code, telem_str);
                } else {
                    text.0 = format!("🟡 In Queue... Waiting (1/2){}", telem_str);
                }
            }
            NetStatus::Connected => {
                text.0 = format!("🟢 Connected to Server{}", telem_str);
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

/// Dynamically updates the fullscreen action button label to clearly display "⛶ EXIT FULLSCREEN"
/// when the game is currently fullscreen, or "⛶ FULLSCREEN" when windowed.
pub fn update_fullscreen_button_text_system(
    mut fs_text_query: Query<&mut Text, With<FullscreenButtonText>>,
    #[cfg(not(target_arch = "wasm32"))]
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    #[cfg(target_arch = "wasm32")]
    let is_fullscreen = {
        js_sys::eval("Boolean(window.__rts_is_fullscreen && window.__rts_is_fullscreen())")
            .map(|v| v.as_bool().unwrap_or(false))
            .unwrap_or(false)
    };

    #[cfg(not(target_arch = "wasm32"))]
    let is_fullscreen = {
        window_query.get_single().map_or(false, |w| w.mode != bevy::window::WindowMode::Windowed)
    };

    for mut text in &mut fs_text_query {
        let desired = if is_fullscreen {
            "⛶ EXIT FULLSCREEN"
        } else {
            "⛶ FULLSCREEN"
        };
        if text.0 != desired {
            text.0 = desired.to_string();
        }
    }
}

/// Dynamically updates the Game Menu modal layout to fit mobile landscape mode screens
/// (height 320-400px), ensuring the entire menu and all buttons are 100% visible and accessible.
pub fn update_responsive_menu_modal_system(
    control_scheme: Option<Res<crate::controls::ControlScheme>>,
    net_client: Res<NetClient>,
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut dialog_query: Query<&mut Node, (With<LobbyModalDialog>, Without<LobbyModalSubtitleText>, Without<LobbyModalControlsBox>, Without<LobbyModalActionsBox>)>,
    mut subtitle_query: Query<(&mut Node, &mut TextFont), (With<LobbyModalSubtitleText>, Without<LobbyModalDialog>, Without<LobbyModalControlsBox>, Without<LobbyModalActionsBox>)>,
    mut status_font_query: Query<&mut TextFont, (With<LobbyStatusText>, Without<LobbyModalSubtitleText>, Without<LobbyModalControlsHeader>, Without<LobbyModalControlsText>, Without<FullscreenButtonText>)>,
    mut controls_box_query: Query<&mut Node, (With<LobbyModalControlsBox>, Without<LobbyModalDialog>, Without<LobbyModalSubtitleText>, Without<LobbyModalActionsBox>)>,
    mut controls_header_query: Query<(&mut Text, &mut TextFont), (With<LobbyModalControlsHeader>, Without<LobbyStatusText>, Without<LobbyModalSubtitleText>, Without<LobbyModalControlsText>, Without<FullscreenButtonText>)>,
    mut controls_text_query: Query<(&mut Text, &mut TextFont), (With<LobbyModalControlsText>, Without<LobbyStatusText>, Without<LobbyModalSubtitleText>, Without<LobbyModalControlsHeader>, Without<FullscreenButtonText>)>,
    mut actions_box_query: Query<&mut Node, (With<LobbyModalActionsBox>, Without<LobbyModalDialog>, Without<LobbyModalSubtitleText>, Without<LobbyModalControlsBox>)>,
) {
    let win_mobile = window_query.get_single().map_or(false, |w| w.width() < 960.0 || w.height() < 550.0);
    let is_mobile = control_scheme.map_or(false, |s| *s == crate::controls::ControlScheme::MobileTouch)
        || net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_mobile;

    if is_mobile {
        for mut node in &mut dialog_query {
            node.width = Val::Px(560.0);
            node.max_width = Val::Percent(96.0);
            node.max_height = Val::Percent(94.0);
            node.padding = UiRect::axes(Val::Px(16.0), Val::Px(10.0));
            node.row_gap = Val::Px(6.0);
        }
        for (mut node, _font) in &mut subtitle_query {
            node.display = Display::None;
        }
        for mut font in &mut status_font_query {
            font.font_size = 11.5;
        }
        for mut node in &mut controls_box_query {
            node.padding = UiRect::axes(Val::Px(10.0), Val::Px(5.0));
            node.row_gap = Val::Px(2.0);
        }
        for (mut text, mut font) in &mut controls_header_query {
            text.0 = "📱 MOBILE TOUCH CONTROLS".to_string();
            font.font_size = 11.0;
        }
        for (mut text, mut font) in &mut controls_text_query {
            let mob_text = "• Tap: Select / Move / Attack / Mine | Drag: Pan Camera | Pinch: Zoom\n• [⛏️ Worker] Quick Train | [🏗️ BUILD] Menu | [✕] Deselect Army";
            if text.0 != mob_text {
                text.0 = mob_text.to_string();
            }
            font.font_size = 10.5;
        }
        for mut node in &mut actions_box_query {
            node.flex_direction = FlexDirection::Row;
            node.column_gap = Val::Px(8.0);
            node.margin = UiRect::top(Val::Px(4.0));
        }
    } else {
        for mut node in &mut dialog_query {
            node.width = Val::Px(520.0);
            node.max_width = Val::Percent(96.0);
            node.max_height = Val::Percent(94.0);
            node.padding = UiRect::axes(Val::Px(20.0), Val::Px(16.0));
            node.row_gap = Val::Px(10.0);
        }
        for (mut node, mut font) in &mut subtitle_query {
            node.display = Display::Flex;
            font.font_size = 12.0;
        }
        for mut font in &mut status_font_query {
            font.font_size = 13.0;
        }
        for mut node in &mut controls_box_query {
            node.padding = UiRect::all(Val::Px(10.0));
            node.row_gap = Val::Px(4.0);
        }
        for (mut text, mut font) in &mut controls_header_query {
            text.0 = "🎮 CONTROLS & COMMANDS".to_string();
            font.font_size = 12.0;
        }
        for (mut text, mut font) in &mut controls_text_query {
            let desk_text = "• Select Units: Left-Click / Drag Selection Box | Orders: Right-Click (Move / Attack / Mine)\n• Unit Tactics: [S] Stop | [H] Hold | Production: [V] Worker | [R] Ranged | [F] Melee\n• Structures: [B] Build Menu (HQ, Barracks, Depot, Turret) | Game Menu: [Tab] / [F1] / [Esc]";
            if text.0 != desk_text {
                text.0 = desk_text.to_string();
            }
            font.font_size = 11.0;
        }
        for mut node in &mut actions_box_query {
            node.flex_direction = FlexDirection::Row;
            node.column_gap = Val::Px(10.0);
            node.margin = UiRect::top(Val::Px(6.0));
        }
    }
}

/// Spawns the in-game modal menu (controls summary, match status, resume, fullscreen toggle, and forfeit)
pub fn spawn_game_menu_modal(parent: &mut ChildBuilder) {
    parent
        .spawn((
            LobbyModalContainer,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.75)),
            ZIndex(100),
            FocusPolicy::Block,
        ))
        .with_children(|backdrop| {
            backdrop
                .spawn((
                    LobbyModalDialog,
                    Node {
                        width: Val::Px(560.0),
                        max_width: Val::Percent(96.0),
                        max_height: Val::Percent(94.0),
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        ..default()
                    },
                    BorderRadius::all(Val::Px(8.0)),
                    BackgroundColor(Color::srgba(0.06, 0.09, 0.14, 0.98)),
                    BorderColor(Color::srgb(0.30, 0.75, 1.0)),
                    FocusPolicy::Block,
                ))
                .with_children(|modal| {
                    // Header Row: Title on left, [✕] Close button on right
                    modal
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            FocusPolicy::Pass,
                        ))
                        .with_children(|header| {
                            header.spawn((
                                LobbyModalTitleText,
                                Text::new("⚙️ GAME MENU"),
                                TextFont {
                                    font_size: 17.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.35, 0.85, 1.0)),
                                FocusPolicy::Pass,
                            ));

                            // Top-Right [✕] Close Button
                            header
                                .spawn((
                                    Button,
                                    Node {
                                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        border: UiRect::all(Val::Px(1.0)),
                                        ..default()
                                    },
                                    BorderRadius::all(Val::Px(4.0)),
                                    BackgroundColor(Color::srgba(0.12, 0.28, 0.45, 0.90)),
                                    BorderColor(Color::srgb(0.35, 0.75, 1.0)),
                                    LobbyButtonAction::CloseModal,
                                ))
                                .with_children(|close_btn| {
                                    close_btn.spawn((
                                        Text::new("✕"),
                                        TextFont {
                                            font_size: 13.0,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                        FocusPolicy::Pass,
                                    ));
                                });
                        });

                    // Subtitle (hidden on mobile to save vertical space)
                    modal.spawn((
                        LobbyModalSubtitleText,
                        Text::new("Match is in progress. Review game status and controls, resume, or forfeit."),
                        TextFont {
                            font_size: 11.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.70, 0.78, 0.85)),
                        FocusPolicy::Pass,
                    ));

                    // Match status card
                    modal.spawn((
                        LobbyStatusText,
                        Text::new("Status: Match Active"),
                        TextFont {
                            font_size: 11.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.80, 0.92, 1.0)),
                        FocusPolicy::Pass,
                    ));

                    // Controls summary container
                    modal
                        .spawn((
                            LobbyModalControlsBox,
                            Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                                row_gap: Val::Px(3.0),
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
                                LobbyModalControlsHeader,
                                Text::new("🎮 CONTROLS & COMMANDS"),
                                TextFont {
                                    font_size: 11.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.35, 0.85, 1.0)),
                                FocusPolicy::Pass,
                            ));
                            guide.spawn((
                                LobbyModalControlsText,
                                Text::new("• Tap: Select / Move / Attack / Mine | Drag: Pan Camera | Pinch: Zoom\n• [⛏️ Worker] Quick Train | [🏗️ BUILD] Menu | [✕] Deselect Army"),
                                TextFont {
                                    font_size: 10.5,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.75, 0.85, 0.95)),
                                FocusPolicy::Pass,
                            ));
                        });

                    // Action Buttons Row (Resume, Fullscreen, Forfeit)
                    modal
                        .spawn((
                            LobbyModalActionsBox,
                            Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(8.0),
                                justify_content: JustifyContent::SpaceBetween,
                                margin: UiRect::top(Val::Px(4.0)),
                                ..default()
                            },
                            FocusPolicy::Pass,
                        ))
                        .with_children(|actions| {
                            // 1. Resume Button
                            actions
                                .spawn((
                                    Button,
                                    Node {
                                        flex_grow: 1.0,
                                        flex_shrink: 1.0,
                                        flex_basis: Val::Percent(30.0),
                                        padding: UiRect::axes(Val::Px(6.0), Val::Px(9.0)),
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
                                        Text::new("▶ RESUME"),
                                        TextFont {
                                            font_size: 12.5,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                        FocusPolicy::Pass,
                                    ));
                                });

                            // 2. Fullscreen Button (Exit / Enter Fullscreen)
                            actions
                                .spawn((
                                    Button,
                                    Node {
                                        flex_grow: 1.0,
                                        flex_shrink: 1.0,
                                        flex_basis: Val::Percent(36.0),
                                        padding: UiRect::axes(Val::Px(6.0), Val::Px(9.0)),
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
                                        FullscreenButtonText,
                                        Text::new("⛶ EXIT FULLSCREEN"),
                                        TextFont {
                                            font_size: 12.5,
                                            ..default()
                                        },
                                        TextColor(Color::srgb(0.85, 0.95, 1.0)),
                                        FocusPolicy::Pass,
                                    ));
                                });

                            // 3. Forfeit Button
                            actions
                                .spawn((
                                    Button,
                                    Node {
                                        flex_grow: 1.0,
                                        flex_shrink: 1.0,
                                        flex_basis: Val::Percent(30.0),
                                        padding: UiRect::axes(Val::Px(6.0), Val::Px(9.0)),
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
                                        Text::new("🏳️ FORFEIT"),
                                        TextFont {
                                            font_size: 12.5,
                                            ..default()
                                        },
                                        TextColor(Color::srgb(1.0, 0.90, 0.90)),
                                        FocusPolicy::Pass,
                                    ));
                                });
                        });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::ControlScheme;
    use shared::protocol::ClientPlatform;

    #[test]
    fn test_menu_modal_landscape_mobile_layout() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NetClient>();

        let mut window = Window::default();
        window.resolution.set(800.0, 360.0); // Typical landscape mobile phone resolution
        app.world_mut().spawn(window);

        app.insert_resource(ControlScheme::MobileTouch);
        let mut net_client = app.world_mut().resource_mut::<NetClient>();
        net_client.my_platform = ClientPlatform::Mobile;

        let dialog = app.world_mut().spawn((LobbyModalDialog, Node::default())).id();
        let subtitle = app.world_mut().spawn((LobbyModalSubtitleText, Node::default(), TextFont::default())).id();
        let status = app.world_mut().spawn((LobbyStatusText, TextFont::default())).id();
        let _controls_box = app.world_mut().spawn((LobbyModalControlsBox, Node::default())).id();
        let controls_header = app.world_mut().spawn((LobbyModalControlsHeader, Text::new(""), TextFont::default())).id();
        let controls_text = app.world_mut().spawn((LobbyModalControlsText, Text::new(""), TextFont::default())).id();
        let actions_box = app.world_mut().spawn((LobbyModalActionsBox, Node::default())).id();

        app.add_systems(Update, update_responsive_menu_modal_system);
        app.update();

        let d_node = app.world().get::<Node>(dialog).unwrap();
        assert_eq!(d_node.width, Val::Px(560.0));
        assert_eq!(d_node.max_width, Val::Percent(96.0));
        assert_eq!(d_node.max_height, Val::Percent(94.0));

        let sub_node = app.world().get::<Node>(subtitle).unwrap();
        assert_eq!(sub_node.display, Display::None);

        let stat_font = app.world().get::<TextFont>(status).unwrap();
        assert_eq!(stat_font.font_size, 11.5);

        let header_t = app.world().get::<Text>(controls_header).unwrap();
        assert_eq!(header_t.0, "📱 MOBILE TOUCH CONTROLS");

        let ctrl_t = app.world().get::<Text>(controls_text).unwrap();
        assert!(ctrl_t.0.contains("Tap: Select / Move / Attack"));

        let act_node = app.world().get::<Node>(actions_box).unwrap();
        assert_eq!(act_node.flex_direction, FlexDirection::Row);
    }

    #[test]
    fn test_menu_modal_desktop_layout() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NetClient>();

        let mut window = Window::default();
        window.resolution.set(1920.0, 1080.0);
        app.world_mut().spawn(window);

        app.insert_resource(ControlScheme::DesktopMouseKeyboard);

        let dialog = app.world_mut().spawn((LobbyModalDialog, Node::default())).id();
        let subtitle = app.world_mut().spawn((LobbyModalSubtitleText, Node::default(), TextFont::default())).id();
        let _status = app.world_mut().spawn((LobbyStatusText, TextFont::default())).id();
        let _controls_box = app.world_mut().spawn((LobbyModalControlsBox, Node::default())).id();
        let controls_header = app.world_mut().spawn((LobbyModalControlsHeader, Text::new(""), TextFont::default())).id();
        let controls_text = app.world_mut().spawn((LobbyModalControlsText, Text::new(""), TextFont::default())).id();
        let _actions_box = app.world_mut().spawn((LobbyModalActionsBox, Node::default())).id();

        app.add_systems(Update, update_responsive_menu_modal_system);
        app.update();

        let d_node = app.world().get::<Node>(dialog).unwrap();
        assert_eq!(d_node.width, Val::Px(520.0));

        let sub_node = app.world().get::<Node>(subtitle).unwrap();
        assert_eq!(sub_node.display, Display::Flex);

        let header_t = app.world().get::<Text>(controls_header).unwrap();
        assert_eq!(header_t.0, "🎮 CONTROLS & COMMANDS");

        let ctrl_t = app.world().get::<Text>(controls_text).unwrap();
        assert!(ctrl_t.0.contains("Left-Click"));
    }

    #[test]
    fn test_update_fullscreen_button_text() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let mut window = Window::default();
        window.mode = bevy::window::WindowMode::Windowed;
        let win_id = app.world_mut().spawn((window, bevy::window::PrimaryWindow)).id();

        let fs_text_ent = app.world_mut().spawn((FullscreenButtonText, Text::new(""))).id();

        app.add_systems(Update, update_fullscreen_button_text_system);
        app.update();

        let text = app.world().get::<Text>(fs_text_ent).unwrap();
        assert_eq!(text.0, "⛶ FULLSCREEN");

        // When window is in Fullscreen / BorderlessFullscreen
        app.world_mut().get_mut::<Window>(win_id).unwrap().mode = bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current);
        app.update();

        let text_after = app.world().get::<Text>(fs_text_ent).unwrap();
        assert_eq!(text_after.0, "⛶ EXIT FULLSCREEN");
    }

    #[test]
    fn test_modal_toggle_and_close_interactions() {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_resource::<NetClient>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_state::<AppState>();

        let window = Window::default();
        app.world_mut().spawn((window, bevy::window::PrimaryWindow));

        let modal_container = app.world_mut().spawn((
            LobbyModalContainer,
            Node { display: Display::None, ..default() },
        )).id();

        // 1. Toggle via Escape
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        app.world_mut().run_system_once(handle_lobby_button_interactions).unwrap();

        assert_eq!(app.world().get::<Node>(modal_container).unwrap().display, Display::Flex);

        // 2. Toggle via Escape again
        *app.world_mut().resource_mut::<ButtonInput<KeyCode>>() = ButtonInput::default();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        app.world_mut().run_system_once(handle_lobby_button_interactions).unwrap();

        assert_eq!(app.world().get::<Node>(modal_container).unwrap().display, Display::None);

        // 3. CloseModal button interaction
        *app.world_mut().resource_mut::<ButtonInput<KeyCode>>() = ButtonInput::default();
        app.world_mut().get_mut::<Node>(modal_container).unwrap().display = Display::Flex;

        let _btn = app.world_mut().spawn((
            Button,
            Interaction::Pressed,
            BackgroundColor::default(),
            LobbyButtonAction::CloseModal,
        )).id();

        app.world_mut().run_system_once(handle_lobby_button_interactions).unwrap();
        assert_eq!(app.world().get::<Node>(modal_container).unwrap().display, Display::None);
    }
}
