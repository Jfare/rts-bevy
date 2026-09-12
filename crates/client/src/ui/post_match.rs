use bevy::prelude::*;
use shared::components::{AppState, MatchOutcome};
use shared::protocol::ClientMessage;

use crate::net::{NetClient, NetStatus};
use crate::stats::MatchStats;
use super::{MatchBannerContainer, MatchBannerText, MatchStatsSummaryText, PlayAgainButton, ReturnToLandingButton};

pub fn update_match_outcome_banner(
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
                 Gold Mined: {} | Spent: {}\n\
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
                 Gold Mined: {} | Spent: {}\n\
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

pub fn handle_play_again_button_interaction(
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

pub fn handle_return_to_landing_button_interaction(
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

/// Spawns the match outcome banner (Victory / Defeat and post-match scoreboard)
pub fn spawn_post_match_banner(parent: &mut ChildBuilder) {
    parent
        .spawn((
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
}

