use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use shared::components::{
    AppState, Building, Faction, GunTurret, Health, MatchOutcome, ProductionBuilding, ResourceNode, Selectable,
    SiegeTank, Soldier, Stimpack, TacticalStance, Unit, Worker,
};
use shared::economy::PlayerEconomy;
use shared::protocol::{ClientMessage, FactionColor};
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::placement::PlacementState;
use crate::stats::MatchStats;

pub struct RtsUiPlugin;

impl Plugin for RtsUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchCountdown>()
            .add_systems(Startup, setup_hud)
            .add_systems(OnEnter(AppState::InGame), close_menu_on_game_start)
            .add_systems(
                Update,
                (
                    update_hud_economy_text,
                    update_hud_network_status,
                    update_selection_info_text,
                    update_command_card_text,
                    update_match_outcome_banner,
                    update_match_countdown_system,
                    handle_lobby_button_interactions,
                    handle_play_again_button_interaction,
                    handle_return_to_landing_button_interaction,
                    update_lobby_modal_status_text,
                ),
            );
    }
}

fn close_menu_on_game_start(mut modal_query: Query<&mut Node, With<LobbyModalContainer>>) {
    for mut node in &mut modal_query {
        node.display = Display::None;
    }
}

#[derive(Resource, Default, Debug, Clone)]
pub struct MatchCountdown {
    pub is_active: bool,
    pub remaining_seconds: f32,
    pub opponent_name: String,
    pub opponent_color: FactionColor,
    pub last_announced_second: i32,
    pub has_played_go_sound: bool,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum LobbyButtonAction {
    ToggleModal,
    CloseModal,
    ForfeitMatch,
}

#[derive(Component)]
pub struct CountdownOverlayContainer;

#[derive(Component)]
pub struct CountdownNumberText;

#[derive(Component)]
pub struct CountdownSubText;

#[derive(Component)]
pub struct LobbyModalContainer;

#[derive(Component)]
pub struct LobbyStatusText;

#[derive(Component)]
struct NetworkStatusText;

#[derive(Component)]
struct MineralsText;

#[derive(Component)]
struct SupplyText;

#[derive(Component)]
struct ApmText;

#[derive(Component)]
struct SelectionTitleText;

#[derive(Component)]
struct SelectionDetailsText;

#[derive(Component)]
struct ProductionQueueText;

#[derive(Component)]
struct BuildMenuText;

#[derive(Component)]
struct MatchBannerContainer;

#[derive(Component)]
struct MatchBannerText;

#[derive(Component)]
struct MatchStatsSummaryText;

#[derive(Component)]
struct PlayAgainButton;

#[derive(Component)]
struct ReturnToLandingButton;

fn setup_hud(mut commands: Commands) {
    // Root UI container overlay (FocusPolicy::Pass allows mouse clicks to pass to the 2D world)
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            FocusPolicy::Pass,
        ))
        .with_children(|root| {
            // ─────────────────────────────────────────────────────────────────
            // TOP HUD BAR
            // ─────────────────────────────────────────────────────────────────
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(50.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(20.0), Val::Px(10.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.06, 0.08, 0.12, 0.92)),
                BorderColor(Color::srgba(0.20, 0.35, 0.45, 0.85)),
                FocusPolicy::Pass,
            ))
            .with_children(|top_bar| {
                // Game Title & Matchmaking Button
                top_bar
                    .spawn((
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(14.0),
                            ..default()
                        },
                        FocusPolicy::Pass,
                    ))
                    .with_children(|title_group| {
                        title_group.spawn((
                            Text::new("⚔️ MINI-RTS"),
                            TextFont {
                                font_size: 22.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.35, 0.82, 1.0)),
                            FocusPolicy::Pass,
                        ));

                        // Lobby / Matchmaking button
                        title_group
                            .spawn((
                                Button,
                                Node {
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BorderRadius::all(Val::Px(4.0)),
                                BackgroundColor(Color::srgba(0.12, 0.22, 0.32, 0.95)),
                                BorderColor(Color::srgb(0.35, 0.75, 1.0)),
                                LobbyButtonAction::ToggleModal,
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("⚙️ GAME MENU"),
                                    TextFont {
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.85, 0.95, 1.0)),
                                    FocusPolicy::Pass,
                                ));
                            });
                    });

                // Resource & Network Display (Minerals, Supply, Wave Timer, Net Status)
                top_bar
                    .spawn((
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(24.0),
                            ..default()
                        },
                        FocusPolicy::Pass,
                    ))
                    .with_children(|res_group| {
                        res_group.spawn((
                            Text::new("💎 Minerals: 200"),
                            TextFont {
                                font_size: 17.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.25, 0.95, 1.0)),
                            MineralsText,
                            FocusPolicy::Pass,
                        ));
                        res_group.spawn((
                            Text::new("⚡ Supply: 11 / 20"),
                            TextFont {
                                font_size: 17.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.95, 0.85, 0.25)),
                            SupplyText,
                            FocusPolicy::Pass,
                        ));
                        res_group.spawn((
                            Text::new("⚡ APM: 0"),
                            TextFont {
                                font_size: 15.0,
                                ..default()
                            },
                            TextColor(Color::srgb(1.0, 0.75, 0.2)),
                            ApmText,
                            FocusPolicy::Pass,
                        ));
                        res_group.spawn((
                            Text::new("🌐 Connecting..."),
                            TextFont {
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.40, 0.85, 0.45)),
                            NetworkStatusText,
                            FocusPolicy::Pass,
                        ));
                    });
            });

            // ─────────────────────────────────────────────────────────────────
            // CENTER MATCH OUTCOME SCOREBOARD (Hidden until Victory/Defeat)
            // ─────────────────────────────────────────────────────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(50.0),
                    margin: UiRect {
                        left: Val::Px(-270.0),
                        top: Val::Px(-200.0),
                        right: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                    },
                    width: Val::Px(540.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(16.0),
                    display: Display::None,
                    ..default()
                },
                BorderRadius::all(Val::Px(8.0)),
                BackgroundColor(Color::srgba(0.04, 0.06, 0.09, 0.98)),
                BorderColor(Color::srgb(0.3, 0.8, 1.0)),
                MatchBannerContainer,
            ))
            .with_children(|banner| {
                banner.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 26.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    MatchBannerText,
                ));

                banner.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.90, 0.95)),
                    MatchStatsSummaryText,
                ));

                // Action Buttons Row
                banner
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(14.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        // Play Again / Restart Button
                        row.spawn((
                            Button,
                            Node {
                                padding: UiRect::axes(Val::Px(24.0), Val::Px(10.0)),
                                border: UiRect::all(Val::Px(1.5)),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BorderRadius::all(Val::Px(6.0)),
                            BackgroundColor(Color::srgba(0.15, 0.35, 0.55, 0.95)),
                            BorderColor(Color::srgb(0.35, 0.85, 1.0)),
                            PlayAgainButton,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("🔄 PLAY AGAIN"),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });

                        // Return to Landing Button
                        row.spawn((
                            Button,
                            Node {
                                padding: UiRect::axes(Val::Px(24.0), Val::Px(10.0)),
                                border: UiRect::all(Val::Px(1.5)),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BorderRadius::all(Val::Px(6.0)),
                            BackgroundColor(Color::srgba(0.22, 0.26, 0.34, 0.95)),
                            BorderColor(Color::srgb(0.60, 0.70, 0.85)),
                            ReturnToLandingButton,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("🏠 LANDING PAGE"),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                    });
            });

            // ─────────────────────────────────────────────────────────────────
            // CENTER COUNTDOWN OVERLAY (3, 2, 1, ENGAGE!)
            // ─────────────────────────────────────────────────────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(38.0),
                    margin: UiRect {
                        left: Val::Px(-180.0),
                        top: Val::Px(-80.0),
                        ..default()
                    },
                    width: Val::Px(360.0),
                    height: Val::Px(160.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    display: Display::None,
                    padding: UiRect::all(Val::Px(16.0)),
                    row_gap: Val::Px(6.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderRadius::all(Val::Px(12.0)),
                BackgroundColor(Color::srgba(0.04, 0.07, 0.12, 0.95)),
                BorderColor(Color::srgb(0.22, 0.74, 0.97)),
                CountdownOverlayContainer,
            ))
            .with_children(|cd| {
                cd.spawn((
                    Text::new("3"),
                    TextFont {
                        font_size: 56.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.22, 0.74, 0.97)),
                    CountdownNumberText,
                ));
                cd.spawn((
                    Text::new("PREPARE FOR BATTLE"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.70, 0.85, 0.95)),
                    CountdownSubText,
                ));
            });

            // ─────────────────────────────────────────────────────────────────
            // IN-GAME GAME MENU (Resume Match, Quick Guide, Forfeit)
            // ─────────────────────────────────────────────────────────────────
            root.spawn((
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
                            Text::new("• Select Units: Left-Click / Drag Selection Box\n• Issue Orders: Right-Click (Move / Attack / Harvest)\n• Unit Tactics: [S] Stop | [H] Hold Position\n• Unit Abilities: [T] Stimpack | [E] Siege Mode\n• Structures: [B] Build Menu (HQ, Barracks, Supply Depot, Turret)\n• Game Menu: [Tab] / [F1] / [Esc]"),
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

            // ─────────────────────────────────────────────────────────────────
            // BOTTOM HUD BAR (Radar Minimap, Selection Card, Build Menu)
            // ─────────────────────────────────────────────────────────────────
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::FlexEnd,
                    column_gap: Val::Px(16.0),
                    ..default()
                },
                FocusPolicy::Pass,
            ))
            .with_children(|bottom_row| {
                // Left Panel: Radar Minimap Frame
                bottom_row
                    .spawn((
                        Node {
                            width: Val::Px(170.0),
                            height: Val::Px(170.0),
                            padding: UiRect::all(Val::Px(8.0)),
                            border: UiRect::all(Val::Px(1.5)),
                            justify_content: JustifyContent::FlexStart,
                            align_items: AlignItems::FlexStart,
                            ..default()
                        },
                        BorderRadius::all(Val::Px(4.0)),
                        BackgroundColor(Color::srgba(0.04, 0.07, 0.10, 0.85)),
                        BorderColor(Color::srgba(0.20, 0.45, 0.70, 0.90)),
                        FocusPolicy::Pass,
                    ))
                    .with_children(|radar| {
                        radar.spawn((
                            Text::new("📡 RADAR MAP"),
                            TextFont {
                                font_size: 10.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.35, 0.80, 1.0, 0.8)),
                            FocusPolicy::Pass,
                        ));
                    });

                // Center Panel: Selection Info & Production Queue
                bottom_row
                    .spawn((
                        Node {
                            flex_grow: 1.0,
                            max_width: Val::Px(460.0),
                            padding: UiRect::all(Val::Px(14.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(6.0),
                            ..default()
                        },
                        BorderRadius::all(Val::Px(4.0)),
                        BackgroundColor(Color::srgba(0.06, 0.08, 0.12, 0.92)),
                        BorderColor(Color::srgba(0.20, 0.35, 0.45, 0.85)),
                        FocusPolicy::Pass,
                    ))
                    .with_children(|card| {
                        card.spawn((
                            Text::new("No Units Selected"),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.92, 0.95, 0.98)),
                            SelectionTitleText,
                            FocusPolicy::Pass,
                        ));
                        card.spawn((
                            Text::new("Drag left-click to select | Right-click ground to Move, enemy to Attack"),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.60, 0.68, 0.75)),
                            SelectionDetailsText,
                            FocusPolicy::Pass,
                        ));
                        card.spawn((
                            Text::new(""),
                            TextFont {
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.35, 0.85, 1.0)),
                            ProductionQueueText,
                            FocusPolicy::Pass,
                        ));
                    });

                // Right Panel: Build Commands & Shortcuts
                bottom_row
                    .spawn((
                        Node {
                            padding: UiRect::all(Val::Px(12.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(4.0),
                            align_items: AlignItems::FlexEnd,
                            ..default()
                        },
                        BorderRadius::all(Val::Px(4.0)),
                        BackgroundColor(Color::srgba(0.06, 0.08, 0.12, 0.90)),
                        BorderColor(Color::srgba(0.20, 0.35, 0.45, 0.85)),
                        FocusPolicy::Pass,
                    ))
                    .with_children(|legend| {
                        legend.spawn((
                            Text::new("COMMAND & BUILD MENU"),
                            TextFont {
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.35, 0.82, 1.0)),
                            FocusPolicy::Pass,
                        ));
                        legend.spawn((
                            Text::new("[B] Barracks (150 💎) | [U] Turret (125 💎) | [P] Depot (100 💎) | [H] HQ (400 💎)"),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.95, 0.85, 0.35)),
                            BuildMenuText,
                            FocusPolicy::Pass,
                        ));
                        legend.spawn((
                            Text::new("HQ: [V] SCV (50 💎) | Barracks: [M] Marine (100 💎) [T] Tank (200 💎)"),
                            TextFont {
                                font_size: 11.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.70, 0.78, 0.85)),
                            FocusPolicy::Pass,
                        ));
                    });
            });
        });
}

fn handle_lobby_button_interactions(
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

fn update_lobby_modal_status_text(
    net_client: Res<NetClient>,
    telemetry: Option<Res<crate::net::ServerTelemetry>>,
    mut text_query: Query<&mut Text, With<LobbyStatusText>>,
) {
    let telem_str = if let Some(t) = telemetry {
        format!(
            " | 👥 Queue: {} | ⚔️ 1v1: {}/{} | 🤖 Solo: {}/{}",
            t.queue_1v1, t.active_1v1_matches, t.max_1v1_matches, t.active_solo_matches, t.max_solo_matches
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

fn update_hud_economy_text(
    economy: Res<PlayerEconomy>,
    net_client: Res<NetClient>,
    stats: Res<MatchStats>,
    mut min_query: Query<&mut Text, (With<MineralsText>, Without<SupplyText>, Without<ApmText>)>,
    mut sup_query: Query<&mut Text, (With<SupplyText>, Without<MineralsText>, Without<ApmText>)>,
    mut apm_query: Query<&mut Text, (With<ApmText>, Without<MineralsText>, Without<SupplyText>)>,
) {
    let my_eco = economy.get(net_client.my_faction);
    for mut text in &mut min_query {
        text.0 = format!("💎 Minerals: {}", my_eco.minerals);
    }
    for mut text in &mut sup_query {
        text.0 = format!("⚡ Supply: {} / {}", my_eco.current_supply, my_eco.max_supply);
    }
    for mut text in &mut apm_query {
        text.0 = format!("⚡ APM: {}", stats.current_apm());
    }
}

fn update_hud_network_status(
    net_client: Res<NetClient>,
    mut text_query: Query<(&mut Text, &mut TextColor), With<NetworkStatusText>>,
) {
    for (mut text, mut color) in &mut text_query {
        match net_client.status {
            NetStatus::InGame => {
                text.0 = format!("🟢 LIVE ({}ms)", net_client.rtt_ms);
                color.0 = Color::srgb(0.25, 0.95, 0.45);
            }
            NetStatus::InLobby => {
                text.0 = "🟡 SEARCHING (1/2)".to_string();
                color.0 = Color::srgb(0.95, 0.85, 0.25);
            }
            NetStatus::Connected => {
                text.0 = "🟢 CONNECTED".to_string();
                color.0 = Color::srgb(0.25, 0.95, 0.45);
            }
            NetStatus::Connecting => {
                text.0 = "🟡 CONNECTING...".to_string();
                color.0 = Color::srgb(0.95, 0.85, 0.25);
            }
            NetStatus::Disconnected => {
                text.0 = "⚪ OFFLINE (SOLO)".to_string();
                color.0 = Color::srgb(0.60, 0.65, 0.70);
            }
        }
    }
}

fn update_selection_info_text(
    unit_query: Query<(
        &Unit,
        &Faction,
        &Health,
        &Selectable,
        Option<&Worker>,
        Option<&Soldier>,
        Option<&SiegeTank>,
        Option<&Stimpack>,
        Option<&TacticalStance>,
    )>,
    building_query: Query<(&Building, &Faction, &Health, &Selectable, Option<&ProductionBuilding>, Option<&GunTurret>)>,
    resource_query: Query<(&ResourceNode, &Selectable)>,
    mut title_query: Query<&mut Text, (With<SelectionTitleText>, Without<SelectionDetailsText>, Without<ProductionQueueText>)>,
    mut details_query: Query<&mut Text, (With<SelectionDetailsText>, Without<SelectionTitleText>, Without<ProductionQueueText>)>,
    mut queue_query: Query<&mut Text, (With<ProductionQueueText>, Without<SelectionTitleText>, Without<SelectionDetailsText>)>,
) {
    let mut selected_units = Vec::new();
    let mut selected_building = None;
    let mut selected_resource = None;

    for (unit, faction, health, selectable, worker_opt, soldier_opt, tank_opt, stim_opt, stance_opt) in &unit_query {
        if selectable.is_selected {
            selected_units.push((unit, faction, health, worker_opt, soldier_opt, tank_opt, stim_opt, stance_opt));
        }
    }

    for (building, faction, health, selectable, prod_opt, turret_opt) in &building_query {
        if selectable.is_selected {
            selected_building = Some((building, faction, health, prod_opt, turret_opt));
            break;
        }
    }

    for (resource, selectable) in &resource_query {
        if selectable.is_selected {
            selected_resource = Some(resource);
            break;
        }
    }

    let mut title_str = "No Units Selected".to_string();
    let mut details_str = "Drag left-click to select | Right-click Move / Attack | [S] Stop | [H] Hold".to_string();
    let mut queue_str = String::new();

    if let Some((building, faction, health, prod_opt, turret_opt)) = selected_building {
        let fac_str = if *faction == Faction::Player1 { "Player 1" } else if *faction == Faction::Player2 { "Player 2" } else { "Hostile" };
        title_str = format!("🏢 {} ({}) - HP: {:.0}/{:.0}", building.name, fac_str, health.current, health.max);

        if !building.is_constructed {
            details_str = format!("⚠️ Under Construction... ({:.0}%)", building.progress() * 100.0);
        } else if turret_opt.is_some() {
            details_str = "Automated Twin-Cannon Defense | 360° Attack Arc (18 DMG, 220 Range)".to_string();
        } else if let Some(prod) = prod_opt {
            let train_prompt = if building.name.contains("Base HQ") {
                "Press [V] to Train SCV Worker (50 💎, 1 ⚡)"
            } else if building.name.contains("Barracks") {
                "Press [M] Marine (100 💎, 2 ⚡) | [T] / [S] Siege Tank (200 💎, 3 ⚡)"
            } else {
                "Right-click ground to set Rally Point"
            };
            details_str = train_prompt.to_string();

            if !prod.queue.is_empty() {
                let first = &prod.queue[0];
                let progress = (prod.current_timer / first.build_duration).clamp(0.0, 1.0) * 100.0;
                let queued_names: Vec<_> = prod.queue.iter().map(|q| q.name.clone()).collect();
                queue_str = format!("⚙️ Queue: {} ({:.0}%) | Queued: {}", queued_names.join(", "), progress, prod.queue.len());
            }
        }
    } else if let Some(resource) = selected_resource {
        title_str = "💎 Mineral Resource Patch".to_string();
        details_str = format!("Remaining Minerals: {} / {}", resource.remaining_minerals, resource.max_minerals);
    } else if !selected_units.is_empty() {
        if selected_units.len() == 1 {
            let (unit, faction, health, worker_opt, soldier_opt, tank_opt, stim_opt, stance_opt) = selected_units[0];
            let fac_str = if *faction == Faction::Player1 { "Player 1" } else if *faction == Faction::Player2 { "Player 2" } else { "Hostile" };
            title_str = format!("🎖️ {} ({}) - HP: {:.0}/{:.0}", unit.name, fac_str, health.current, health.max);

            let stance_suffix = match stance_opt {
                Some(shared::components::TacticalStance::HoldPosition) => " [HOLDING POSITION]",
                _ => "",
            };

            if let Some(worker) = worker_opt {
                let state_str = match worker.state {
                    shared::components::WorkerState::Idle => "Idle",
                    shared::components::WorkerState::MovingToResource => "Moving to Mineral Patch",
                    shared::components::WorkerState::Mining => "Harvesting Minerals with Laser",
                    shared::components::WorkerState::MovingToBase => "Returning Minerals to Base HQ",
                };
                details_str = format!("Worker: {}{} | Carried: {} 💎 | [S] Stop", state_str, stance_suffix, worker.carried_minerals);
            } else if let Some(tank) = tank_opt {
                match tank.mode {
                    shared::components::TankMode::Tank => {
                        details_str = format!("Mobile Tank (35 DMG, 240 Rng){} | [E] Deploy Siege Mode | [S] Stop | [H] Hold", stance_suffix);
                    }
                    shared::components::TankMode::Siege => {
                        details_str = "🛡️ SIEGE MODE (70 DMG + 45px Splash, 380 Rng, Immobile) | [E] Mobile Mode".to_string();
                    }
                    shared::components::TankMode::TransformingToSiege => {
                        details_str = "⚙️ Deploying Stabilizers & Artillery Cannon...".to_string();
                    }
                    shared::components::TankMode::TransformingToTank => {
                        details_str = "⚙️ Retracting Stabilizers...".to_string();
                    }
                }
            } else if soldier_opt.is_some() {
                let stim_status = if let Some(stim) = stim_opt {
                    if stim.is_active {
                        format!(" | 💉 STIMPACK ACTIVE ({:.1}s)", stim.timer)
                    } else {
                        " | [T] Stimpack (+50% Spd/Fire, -15 HP)".to_string()
                    }
                } else {
                    " | [T] Stimpack".to_string()
                };
                details_str = format!("Marine Rifleman (15 DMG){} | Right-Click Move/Attack | [S] Stop | [H] Hold{}", stance_suffix, stim_status);
            } else {
                details_str = "Combat Unit ready | Right-Click Move/Attack | [S] Stop | [H] Hold".to_string();
            }
        } else {
            title_str = format!("Selected: {} Units", selected_units.len());
            details_str = "Squad Command: Right-Click Move/Attack | [S] Stop | [H] Hold Position | [T] Stimpack | [E] Siege".to_string();
        }
    }

    for mut text in &mut title_query {
        text.0 = title_str.clone();
    }
    for mut text in &mut details_query {
        text.0 = details_str.clone();
    }
    for mut text in &mut queue_query {
        text.0 = queue_str.clone();
    }
}

fn update_command_card_text(
    placement_state: Res<PlacementState>,
    mut text_query: Query<&mut Text, With<BuildMenuText>>,
) {
    for mut text in &mut text_query {
        if let Some(kind) = placement_state.active_kind {
            let status = if placement_state.is_valid { "Valid Location (Left-Click to Place)" } else { "Blocked / Insufficient Tech or Minerals" };
            text.0 = format!("🏗️ Placing: {} ($ {}) - {} | [Esc/Right-Click] Cancel", kind.name(), placement_state.mineral_cost, status);
        } else {
            text.0 = "[B] Barracks (150 💎) | [U] Turret (125 💎, Req Barracks) | [P] Supply Depot (100 💎) | [H] Base HQ (400 💎)".to_string();
        }
    }
}

fn update_match_outcome_banner(
    outcome: Option<Res<MatchOutcome>>,
    stats: Res<MatchStats>,
    mut banner_query: Query<(&mut Node, &mut BorderColor), With<MatchBannerContainer>>,
    mut text_query: Query<(&mut Text, &mut TextColor), (With<MatchBannerText>, Without<MatchStatsSummaryText>)>,
    mut summary_query: Query<&mut Text, (With<MatchStatsSummaryText>, Without<MatchBannerText>)>,
) {
    let Some(outcome) = outcome else {
        return;
    };

    let Ok((mut node, mut border)) = banner_query.get_single_mut() else {
        return;
    };
    let Ok((mut text, mut color)) = text_query.get_single_mut() else {
        return;
    };
    let Ok(mut summary_text) = summary_query.get_single_mut() else {
        return;
    };

    match *outcome {
        MatchOutcome::InProgress => {
            node.display = Display::None;
        }
        MatchOutcome::Victory => {
            node.display = Display::Flex;
            *border = BorderColor(Color::srgb(0.20, 0.95, 0.45));
            text.0 = "🏆 VICTORY - MISSION COMPLETE!".to_string();
            *color = TextColor(Color::srgb(0.25, 0.95, 0.50));

            let mins = (stats.elapsed_seconds / 60.0) as u32;
            let secs = (stats.elapsed_seconds % 60.0) as u32;

            summary_text.0 = format!(
                "⏱️ Match Duration: {:02}:{:02} | ⚡ APM: {} ({} Actions)\n\
                 💎 Minerals Mined: {} | Spent: {}\n\
                 🎖️ Units Trained: {} | Units Lost: {} | Kills: {}\n\
                 💥 Enemy Bases Destroyed: {} | Damage Dealt: {:.0}\n\
                 🎯 Kill / Death Ratio: {:.2}",
                mins,
                secs,
                stats.current_apm(),
                stats.total_commands,
                stats.minerals_mined,
                stats.minerals_spent,
                stats.units_trained,
                stats.units_lost,
                stats.enemy_units_killed,
                stats.enemy_buildings_destroyed,
                stats.damage_dealt,
                stats.kd_ratio()
            );
        }
        MatchOutcome::Defeat => {
            node.display = Display::Flex;
            *border = BorderColor(Color::srgb(0.95, 0.25, 0.25));
            text.0 = "💥 DEFEAT - BASE OVERRUN!".to_string();
            *color = TextColor(Color::srgb(0.95, 0.35, 0.35));

            let mins = (stats.elapsed_seconds / 60.0) as u32;
            let secs = (stats.elapsed_seconds % 60.0) as u32;

            summary_text.0 = format!(
                "⏱️ Match Duration: {:02}:{:02} | ⚡ APM: {} ({} Actions)\n\
                 💎 Minerals Mined: {} | Spent: {}\n\
                 🎖️ Units Trained: {} | Units Lost: {} | Kills: {}\n\
                 💥 Enemy Bases Destroyed: {} | Damage Dealt: {:.0}\n\
                 🎯 Kill / Death Ratio: {:.2}",
                mins,
                secs,
                stats.current_apm(),
                stats.total_commands,
                stats.minerals_mined,
                stats.minerals_spent,
                stats.units_trained,
                stats.units_lost,
                stats.enemy_units_killed,
                stats.enemy_buildings_destroyed,
                stats.damage_dealt,
                stats.kd_ratio()
            );
        }
    }
}

fn handle_play_again_button_interaction(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<PlayAgainButton>),
    >,
    mut net_client: ResMut<NetClient>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for (interaction, mut bg_color) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                info!("🔄 Play Again / Return to lobby requested.");
                net_client.send(&ClientMessage::ForfeitMatch);
                net_client.status = NetStatus::Connected;
                next_state.set(AppState::Lobby);
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = js_sys::eval("if (window.__rts_return_to_lobby) { window.__rts_return_to_lobby(); }");
                }
            }
            Interaction::Hovered => {
                bg_color.0 = Color::srgba(0.25, 0.55, 0.85, 0.95);
            }
            Interaction::None => {
                bg_color.0 = Color::srgba(0.15, 0.35, 0.55, 0.95);
            }
        }
    }
}

fn handle_return_to_landing_button_interaction(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<ReturnToLandingButton>),
    >,
    mut net_client: ResMut<NetClient>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for (interaction, mut bg_color) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                info!("🏠 Return to Landing Page requested.");
                net_client.send(&ClientMessage::ForfeitMatch);
                net_client.status = NetStatus::Connected;
                next_state.set(AppState::Lobby);
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = js_sys::eval("if (window.__rts_return_to_lobby) { window.__rts_return_to_lobby(); }");
                }
            }
            Interaction::Hovered => {
                bg_color.0 = Color::srgba(0.35, 0.40, 0.50, 0.95);
            }
            Interaction::None => {
                bg_color.0 = Color::srgba(0.22, 0.26, 0.34, 0.95);
            }
        }
    }
}

fn update_match_countdown_system(
    time: Res<Time>,
    mut countdown: ResMut<MatchCountdown>,
    mut sound_events: EventWriter<SoundEffect>,
    mut container_query: Query<&mut Node, With<CountdownOverlayContainer>>,
    mut text_query: Query<&mut Text, (With<CountdownNumberText>, Without<CountdownSubText>)>,
    mut subtext_query: Query<&mut Text, (With<CountdownSubText>, Without<CountdownNumberText>)>,
    mut color_query: Query<&mut TextColor, With<CountdownNumberText>>,
) {
    if !countdown.is_active {
        for mut node in &mut container_query {
            node.display = Display::None;
        }
        return;
    }

    countdown.remaining_seconds -= time.delta_secs();

    let sec = countdown.remaining_seconds.ceil() as i32;
    if sec > 0 && sec < countdown.last_announced_second {
        countdown.last_announced_second = sec;
        sound_events.send(SoundEffect::CountdownBeep);
    }

    if countdown.remaining_seconds <= 0.0 && !countdown.has_played_go_sound {
        countdown.has_played_go_sound = true;
        sound_events.send(SoundEffect::MatchStart);
    }

    if countdown.remaining_seconds <= -1.2 {
        countdown.is_active = false;
        for mut node in &mut container_query {
            node.display = Display::None;
        }
        return;
    }

    for mut node in &mut container_query {
        node.display = Display::Flex;
    }

    for mut text in &mut text_query {
        if countdown.remaining_seconds > 2.0 {
            text.0 = "3".to_string();
        } else if countdown.remaining_seconds > 1.0 {
            text.0 = "2".to_string();
        } else if countdown.remaining_seconds > 0.0 {
            text.0 = "1".to_string();
        } else {
            text.0 = "⚡ ENGAGE! ⚡".to_string();
        }
    }

    for mut color in &mut color_query {
        if countdown.remaining_seconds > 0.0 {
            color.0 = Color::srgb(0.22, 0.74, 0.97); // Cyan
        } else {
            color.0 = Color::srgb(0.29, 0.87, 0.50); // Emerald Green
        }
    }

    for mut subtext in &mut subtext_query {
        if countdown.remaining_seconds > 0.0 {
            if !countdown.opponent_name.is_empty() {
                subtext.0 = format!("VS {}", countdown.opponent_name);
            } else {
                subtext.0 = "PREPARE FOR BATTLE".to_string();
            }
        } else {
            subtext.0 = "COMMAND PROTOCOLS ENGAGED".to_string();
        }
    }
}
