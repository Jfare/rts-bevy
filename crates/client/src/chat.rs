use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use shared::components::Faction;
use shared::protocol::{ClientMessage, FactionColor};
use crate::net::NetClient;

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatLog>()
            .add_systems(Startup, setup_chat_ui)
            .add_systems(
                Update,
                (
                    handle_chat_keyboard_input,
                    update_chat_display_system,
                ),
            );
    }
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

#[derive(Component)]
struct ChatMessageLogText;

#[derive(Component)]
struct ChatInputContainer;

#[derive(Component)]
struct ChatInputPromptText;

fn setup_chat_ui(mut commands: Commands) {
    // Chat box overlay container positioned at bottom-left above the radar
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(210.0),
                width: Val::Px(360.0),
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
                Text::new("💬 Press [ENTER] to chat"),
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
    if keyboard.just_pressed(KeyCode::Space) {
        if chat_log.current_input.len() < 120 {
            chat_log.current_input.push(' ');
        }
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
        if keyboard.just_pressed(code) {
            if chat_log.current_input.len() < 120 {
                let ch = if shift { upper } else { lower };
                chat_log.current_input.push(ch);
            }
        }
    }
}

fn update_chat_display_system(
    chat_log: Res<ChatLog>,
    mut log_query: Query<&mut Text, (With<ChatMessageLogText>, Without<ChatInputPromptText>)>,
    mut input_text_query: Query<&mut Text, (With<ChatInputPromptText>, Without<ChatMessageLogText>)>,
    mut input_container_query: Query<&mut Node, With<ChatInputContainer>>,
) {
    // 1. Update chat history text
    for mut text in log_query.iter_mut() {
        if chat_log.entries.is_empty() {
            text.0 = if chat_log.is_input_active {
                "💬 Type your message below...".to_string()
            } else {
                "💬 Press [ENTER] to chat".to_string()
            };
        } else {
            let start = chat_log.entries.len().saturating_sub(6);
            let mut formatted = String::new();
            for entry in &chat_log.entries[start..] {
                if entry.is_system {
                    formatted.push_str(&format!("🔔 {}\n", entry.text));
                } else {
                    formatted.push_str(&format!("[{}] {}: {}\n", entry.color.name(), entry.sender_name, entry.text));
                }
            }
            text.0 = formatted.trim_end().to_string();
        }
    }

    // 2. Update chat input bar visibility and text
    for mut node in input_container_query.iter_mut() {
        node.display = if chat_log.is_input_active {
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
