use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use shared::components::{AppState, Faction, MatchOutcome};
use shared::protocol::{ClientMessage, FactionColor};
use crate::controls::ControlScheme;
use crate::net::NetClient;
use crate::stats::MatchStats;

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatLog>()
            .add_systems(Startup, setup_chat_ui)
            .add_systems(OnEnter(AppState::Lobby), reset_chat_log_system)
            .add_systems(
                Update,
                (
                    handle_chat_keyboard_input,
                    update_chat_display_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

pub fn reset_chat_log_system(mut chat_log: ResMut<ChatLog>) {
    chat_log.clear();
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ChatEntry {
    pub sender_name: String,
    pub faction: Faction,
    pub color: FactionColor,
    pub text: String,
    pub is_system: bool,
    pub timestamp_ms: u64,
}

#[derive(Resource, Debug, Clone)]
pub struct ChatLog {
    pub entries: Vec<ChatEntry>,
    pub is_input_active: bool,
    pub current_input: String,
    pub cursor_timer: Timer,
    pub show_cursor: bool,
}

impl Default for ChatLog {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            is_input_active: false,
            current_input: String::new(),
            cursor_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            show_cursor: true,
        }
    }
}

impl ChatLog {
    /// Resets all chat entries and active input fields (e.g. when match ends or new match starts)
    pub fn clear(&mut self) {
        self.entries.clear();
        self.is_input_active = false;
        self.current_input.clear();
    }
}

#[derive(Component)]
pub struct ChatBoxContainer;

#[derive(Component)]
pub struct ChatMessageLogText;

#[derive(Component)]
pub struct ChatInputContainer;

#[derive(Component)]
pub struct ChatInputPromptText;

fn setup_chat_ui(mut commands: Commands) {
    // Chat box overlay container positioned at bottom-left above the radar
    commands
        .spawn((
            ChatBoxContainer,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(210.0),
                width: Val::Px(340.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(4.0)),
            BackgroundColor(Color::srgba(0.04, 0.06, 0.09, 0.75)),
            BorderColor(Color::srgba(0.20, 0.40, 0.60, 0.50)),
            FocusPolicy::Pass,
        ))
        .with_children(|chat_box| {
            // Chat message history text
            chat_box.spawn((
                Text::new("> Press [ENTER] to chat"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.70, 0.80, 0.90)),
                ChatMessageLogText,
                FocusPolicy::Pass,
            ));

            // Chat input bar (active when typing)
            chat_box
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        display: Display::None,
                        ..default()
                    },
                    BorderRadius::all(Val::Px(3.0)),
                    BackgroundColor(Color::srgba(0.08, 0.14, 0.20, 0.95)),
                    BorderColor(Color::srgb(0.35, 0.80, 1.0)),
                    ChatInputContainer,
                    FocusPolicy::Pass,
                ))
                .with_children(|input_bar| {
                    input_bar.spawn((
                        Text::new("Say: "),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.95, 0.98, 1.0)),
                        ChatInputPromptText,
                        FocusPolicy::Pass,
                    ));
                });
        });
}

fn handle_chat_keyboard_input(
    time: Res<Time>,
    mut chat_log: ResMut<ChatLog>,
    keyboard: Res<ButtonInput<KeyCode>>,
    net_client: Res<NetClient>,
) {
    chat_log.cursor_timer.tick(time.delta());
    if chat_log.cursor_timer.just_finished() {
        chat_log.show_cursor = !chat_log.show_cursor;
    }

    // Toggle chat input with Enter
    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        if chat_log.is_input_active {
            let msg = chat_log.current_input.trim().to_string();
            if !msg.is_empty() {
                net_client.send(&ClientMessage::SendChatMessage { text: msg });
            }
            chat_log.current_input.clear();
            chat_log.is_input_active = false;
        } else {
            chat_log.is_input_active = true;
            chat_log.current_input.clear();
        }
        return;
    }

    // Cancel input with Escape
    if chat_log.is_input_active && keyboard.just_pressed(KeyCode::Escape) {
        chat_log.is_input_active = false;
        chat_log.current_input.clear();
        return;
    }

    if !chat_log.is_input_active {
        return;
    }

    // Backspace
    if keyboard.just_pressed(KeyCode::Backspace) {
        chat_log.current_input.pop();
    }

    // Space
    if keyboard.just_pressed(KeyCode::Space)
        && chat_log.current_input.len() < 120 {
            chat_log.current_input.push(' ');
        }

    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    // Alphanumeric keys
    let keys = [
        (KeyCode::KeyA, 'a', 'A'),
        (KeyCode::KeyB, 'b', 'B'),
        (KeyCode::KeyC, 'c', 'C'),
        (KeyCode::KeyD, 'd', 'D'),
        (KeyCode::KeyE, 'e', 'E'),
        (KeyCode::KeyF, 'f', 'F'),
        (KeyCode::KeyG, 'g', 'G'),
        (KeyCode::KeyH, 'h', 'H'),
        (KeyCode::KeyI, 'i', 'I'),
        (KeyCode::KeyJ, 'j', 'J'),
        (KeyCode::KeyK, 'k', 'K'),
        (KeyCode::KeyL, 'l', 'L'),
        (KeyCode::KeyM, 'm', 'M'),
        (KeyCode::KeyN, 'n', 'N'),
        (KeyCode::KeyO, 'o', 'O'),
        (KeyCode::KeyP, 'p', 'P'),
        (KeyCode::KeyQ, 'q', 'Q'),
        (KeyCode::KeyR, 'r', 'R'),
        (KeyCode::KeyS, 's', 'S'),
        (KeyCode::KeyT, 't', 'T'),
        (KeyCode::KeyU, 'u', 'U'),
        (KeyCode::KeyV, 'v', 'V'),
        (KeyCode::KeyW, 'w', 'W'),
        (KeyCode::KeyX, 'x', 'X'),
        (KeyCode::KeyY, 'y', 'Y'),
        (KeyCode::KeyZ, 'z', 'Z'),
        (KeyCode::Digit0, '0', ')'),
        (KeyCode::Digit1, '1', '!'),
        (KeyCode::Digit2, '2', '@'),
        (KeyCode::Digit3, '3', '#'),
        (KeyCode::Digit4, '4', '$'),
        (KeyCode::Digit5, '5', '%'),
        (KeyCode::Digit6, '6', '^'),
        (KeyCode::Digit7, '7', '&'),
        (KeyCode::Digit8, '8', '*'),
        (KeyCode::Digit9, '9', '('),
        (KeyCode::Period, '.', '>'),
        (KeyCode::Comma, ',', '<'),
        (KeyCode::Slash, '/', '?'),
        (KeyCode::Minus, '-', '_'),
        (KeyCode::Equal, '=', '+'),
    ];

    for (code, lower, upper) in keys {
        if keyboard.just_pressed(code)
            && chat_log.current_input.len() < 120 {
                let ch = if shift { upper } else { lower };
                chat_log.current_input.push(ch);
            }
    }
}

pub fn update_chat_display_system(
    time: Res<Time>,
    chat_log: Res<ChatLog>,
    stats_opt: Option<Res<MatchStats>>,
    outcome_opt: Option<Res<MatchOutcome>>,
    control_scheme: Option<Res<ControlScheme>>,
    net_client: Res<NetClient>,
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut chat_box_query: Query<&mut Node, With<ChatBoxContainer>>,
    mut log_query: Query<(&mut Text, &mut TextFont), (With<ChatMessageLogText>, Without<ChatInputPromptText>)>,
    mut input_text_query: Query<&mut Text, (With<ChatInputPromptText>, Without<ChatMessageLogText>)>,
    mut input_container_query: Query<&mut Node, (With<ChatInputContainer>, Without<ChatBoxContainer>)>,
) {
    let win_mobile = window_query.get_single().map_or(false, |w| w.width() < 960.0 || w.height() < 550.0);
    let is_mobile = control_scheme.map_or(false, |s| *s == ControlScheme::MobileTouch)
        || net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_mobile;

    let match_over = outcome_opt.map_or(false, |o| *o == MatchOutcome::Victory || *o == MatchOutcome::Defeat);

    let now_ms = time.elapsed().as_millis() as u64;
    let match_elapsed = stats_opt.as_ref().map_or(0.0, |s| s.elapsed_seconds);

    // Filter unexpired entries: system announcements expire after 10 seconds into the game or 10s after receipt.
    // If match is over, no messages are visible.
    let visible_entries: Vec<&ChatEntry> = if match_over {
        Vec::new()
    } else {
        chat_log
            .entries
            .iter()
            .filter(|entry| {
                if entry.is_system {
                    let age_ms = now_ms.saturating_sub(entry.timestamp_ms);
                    let expired = age_ms >= 10_000 || (entry.text.contains("Commander") && match_elapsed >= 10.0);
                    !expired
                } else {
                    true
                }
            })
            .collect()
    };

    // 1. Update chat history text and responsive font size
    for (mut text, mut font) in log_query.iter_mut() {
        font.font_size = if is_mobile { 9.5 } else { 12.0 };

        if match_over {
            text.0.clear();
        } else if visible_entries.is_empty() {
            if chat_log.is_input_active {
                text.0 = "> Type your message below...".to_string();
            } else if is_mobile {
                text.0.clear();
            } else {
                text.0 = "> Press [ENTER] to chat".to_string();
            }
        } else {
            let max_lines = if is_mobile { 3 } else { 6 };
            let start = visible_entries.len().saturating_sub(max_lines);
            let mut formatted = String::new();
            for entry in &visible_entries[start..] {
                if entry.is_system {
                    formatted.push_str(&format!("[SYS] {}\n", entry.text));
                } else {
                    formatted.push_str(&format!("[{}] {}: {}\n", entry.color.name(), entry.sender_name, entry.text));
                }
            }
            text.0 = formatted.trim_end().to_string();
        }
    }

    // 2. Update chat box container layout and visibility (hidden when match ends or on mobile when empty)
    for mut node in chat_box_query.iter_mut() {
        if match_over {
            node.display = Display::None;
        } else if is_mobile {
            node.width = Val::Px(220.0);
            node.left = Val::Px(10.0);
            node.bottom = Val::Px(85.0);
            node.padding = UiRect::axes(Val::Px(6.0), Val::Px(4.0));
            node.display = if visible_entries.is_empty() && !chat_log.is_input_active {
                Display::None
            } else {
                Display::Flex
            };
        } else {
            node.width = Val::Px(340.0);
            node.left = Val::Px(16.0);
            node.bottom = Val::Px(210.0);
            node.padding = UiRect::all(Val::Px(8.0));
            node.display = Display::Flex;
        }
    }

    // 3. Update chat input bar visibility and text
    for mut node in input_container_query.iter_mut() {
        node.display = if chat_log.is_input_active && !match_over {
            Display::Flex
        } else {
            Display::None
        };
    }

    for mut input_text in input_text_query.iter_mut() {
        let cursor = if chat_log.show_cursor { "█" } else { " " };
        input_text.0 = format!("Say: {}{}", chat_log.current_input, cursor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::protocol::ClientPlatform;

    #[test]
    fn test_chat_system_message_expires_after_10s_mobile() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChatLog>();
        app.init_resource::<NetClient>();
        app.init_resource::<MatchStats>();
        app.init_resource::<MatchOutcome>();
        app.insert_resource(ControlScheme::MobileTouch);

        let mut window = Window::default();
        window.resolution.set(800.0, 400.0);
        app.world_mut().spawn(window);

        let box_ent = app.world_mut().spawn((ChatBoxContainer, Node::default())).id();
        let log_text_ent = app.world_mut().spawn((ChatMessageLogText, Text::new(""), TextFont::default())).id();
        let _input_prompt_ent = app.world_mut().spawn((ChatInputPromptText, Text::new(""))).id();
        let _input_cont_ent = app.world_mut().spawn((ChatInputContainer, Node::default())).id();

        // Push initial commander message
        app.world_mut().resource_mut::<ChatLog>().entries.push(ChatEntry {
            sender_name: "SYSTEM".to_string(),
            faction: Faction::Neutral,
            color: FactionColor::Amber,
            text: "Commander Player 1 deployed to Sector 4. Defend your Base HQ against hostile assault waves!".to_string(),
            is_system: true,
            timestamp_ms: 0,
        });

        // 1. Early in the match (2.0s): Visible, compact mobile sizing, font size 9.5
        app.world_mut().resource_mut::<MatchStats>().elapsed_seconds = 2.0;
        app.add_systems(Update, update_chat_display_system);
        app.update();

        let box_node = app.world().get::<Node>(box_ent).unwrap();
        assert_eq!(box_node.display, Display::Flex);
        assert_eq!(box_node.width, Val::Px(220.0));
        assert_eq!(box_node.bottom, Val::Px(85.0));

        let log_font = app.world().get::<TextFont>(log_text_ent).unwrap();
        assert_eq!(log_font.font_size, 9.5);

        let log_text = app.world().get::<Text>(log_text_ent).unwrap();
        assert!(log_text.0.contains("[SYS] Commander Player 1"));

        // 2. Later in match (10.5s): System message expires -> chat box hides on mobile
        app.world_mut().resource_mut::<MatchStats>().elapsed_seconds = 10.5;
        app.update();

        let box_node_after = app.world().get::<Node>(box_ent).unwrap();
        assert_eq!(box_node_after.display, Display::None);
        let log_text_after = app.world().get::<Text>(log_text_ent).unwrap();
        assert_eq!(log_text_after.0, "");
    }

    #[test]
    fn test_chat_desktop_layout_and_expiration() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChatLog>();
        let mut net_client = NetClient::default();
        net_client.my_platform = ClientPlatform::Desktop;
        app.insert_resource(net_client);
        app.init_resource::<MatchStats>();
        app.init_resource::<MatchOutcome>();
        app.insert_resource(ControlScheme::DesktopMouseKeyboard);

        let mut window = Window::default();
        window.resolution.set(1920.0, 1080.0);
        app.world_mut().spawn(window);

        let box_ent = app.world_mut().spawn((ChatBoxContainer, Node::default())).id();
        let log_text_ent = app.world_mut().spawn((ChatMessageLogText, Text::new(""), TextFont::default())).id();
        let _input_prompt_ent = app.world_mut().spawn((ChatInputPromptText, Text::new(""))).id();
        let _input_cont_ent = app.world_mut().spawn((ChatInputContainer, Node::default())).id();

        app.world_mut().resource_mut::<ChatLog>().entries.push(ChatEntry {
            sender_name: "SYSTEM".to_string(),
            faction: Faction::Neutral,
            color: FactionColor::Amber,
            text: "Commander Player 1 deployed to Sector 4. Defend your Base HQ against hostile assault waves!".to_string(),
            is_system: true,
            timestamp_ms: 0,
        });

        app.world_mut().resource_mut::<MatchStats>().elapsed_seconds = 1.0;
        app.add_systems(Update, update_chat_display_system);
        app.update();

        let box_node = app.world().get::<Node>(box_ent).unwrap();
        assert_eq!(box_node.display, Display::Flex);
        assert_eq!(box_node.width, Val::Px(340.0));
        assert_eq!(box_node.bottom, Val::Px(210.0));

        let log_font = app.world().get::<TextFont>(log_text_ent).unwrap();
        assert_eq!(log_font.font_size, 12.0);

        // Advance to 11.0s: System message expires -> Desktop shows "> Press [ENTER] to chat"
        app.world_mut().resource_mut::<MatchStats>().elapsed_seconds = 11.0;
        app.update();

        let box_node_after = app.world().get::<Node>(box_ent).unwrap();
        assert_eq!(box_node_after.display, Display::Flex);
        let log_text_after = app.world().get::<Text>(log_text_ent).unwrap();
        assert_eq!(log_text_after.0, "> Press [ENTER] to chat");
    }

    #[test]
    fn test_chat_resets_when_game_ends() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChatLog>();
        app.init_resource::<NetClient>();
        app.init_resource::<MatchStats>();
        app.init_resource::<MatchOutcome>();
        app.insert_resource(ControlScheme::DesktopMouseKeyboard);

        let mut window = Window::default();
        window.resolution.set(1920.0, 1080.0);
        app.world_mut().spawn(window);

        let box_ent = app.world_mut().spawn((ChatBoxContainer, Node::default())).id();
        let log_text_ent = app.world_mut().spawn((ChatMessageLogText, Text::new(""), TextFont::default())).id();
        let _input_prompt_ent = app.world_mut().spawn((ChatInputPromptText, Text::new(""))).id();
        let _input_cont_ent = app.world_mut().spawn((ChatInputContainer, Node::default())).id();

        // 1. Add greeting from match 1
        app.world_mut().resource_mut::<ChatLog>().entries.push(ChatEntry {
            sender_name: "SYSTEM".to_string(),
            faction: Faction::Neutral,
            color: FactionColor::Amber,
            text: "Commander Player 1 deployed to Sector 4. Defend your Base HQ against hostile assault waves!".to_string(),
            is_system: true,
            timestamp_ms: 0,
        });

        app.add_systems(Update, update_chat_display_system);
        app.update();

        // Visible during match
        let box_node = app.world().get::<Node>(box_ent).unwrap();
        assert_eq!(box_node.display, Display::Flex);

        // 2. Match ends in Victory
        *app.world_mut().resource_mut::<MatchOutcome>() = MatchOutcome::Victory;
        app.update();

        // Chat immediately hides on match end
        let box_node_after = app.world().get::<Node>(box_ent).unwrap();
        assert_eq!(box_node_after.display, Display::None);

        // 3. Clear chat log on game end
        app.world_mut().resource_mut::<ChatLog>().clear();
        assert_eq!(app.world().resource::<ChatLog>().entries.len(), 0);

        // 4. Start match 2: Fresh chat log, only 1 new welcome message received
        *app.world_mut().resource_mut::<MatchOutcome>() = MatchOutcome::InProgress;
        app.world_mut().resource_mut::<MatchStats>().elapsed_seconds = 1.0;
        app.world_mut().resource_mut::<ChatLog>().entries.push(ChatEntry {
            sender_name: "SYSTEM".to_string(),
            faction: Faction::Neutral,
            color: FactionColor::Amber,
            text: "Commander Player 1 deployed to Sector 4. Defend your Base HQ against hostile assault waves!".to_string(),
            is_system: true,
            timestamp_ms: 1000,
        });
        app.update();

        // Only 1 message is in entries, not repeated!
        assert_eq!(app.world().resource::<ChatLog>().entries.len(), 1);
        let log_text = app.world().get::<Text>(log_text_ent).unwrap();
        assert_eq!(log_text.0, "[SYS] Commander Player 1 deployed to Sector 4. Defend your Base HQ against hostile assault waves!");
    }
}
