use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use super::{
    ApmText, BuildMenuText, CountdownNumberText, CountdownOverlayContainer, CountdownSubText,
    LobbyButtonAction, LobbyModalContainer, LobbyStatusText, MatchBannerContainer,
    MatchBannerText, MatchStatsSummaryText, MineralsText, NetworkStatusText,
    PlayAgainButton, ProductionQueueText, ReturnToLandingButton, SelectionDetailsText,
    SelectionTitleText, SupplyText,
};

pub fn setup_hud(mut commands: Commands) {
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
                            Text::new("HQ: [V]/[W] Worker (50 💎) | Barracks: [R] Ranged Fighter (100 💎) [F] Melee Fighter (75 💎)"),
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

