use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use super::{ApmText, LobbyButtonAction, MineralsText, NetworkStatusText, SupplyText};

/// Spawns the top HUD bar containing branding, Game Menu button, economy counters, APM, and network latency
pub fn spawn_top_bar(parent: &mut ChildBuilder) {
    parent
        .spawn((
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
            // Game Title & Matchmaking / Menu Button
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

                    // Lobby / Matchmaking / Game Menu button
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

            // Resource & Network Display (Gold, Supply, APM, Net Status)
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
                        Text::new("🪙 Gold: 200"),
                        TextFont {
                            font_size: 17.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.84, 0.18)),
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
}

