use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bot_ai::WaveAiState;
use shared::components::{
    Building, Faction, Health, MatchOutcome, ProductionBuilding, ResourceNode, Selectable, Unit,
    Worker,
};
use shared::economy::PlayerEconomy;
use crate::net::{NetClient, NetStatus};
use crate::placement::PlacementState;

pub struct RtsUiPlugin;

impl Plugin for RtsUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_hud)
            .add_systems(
                Update,
                (
                    update_hud_economy_text,
                    update_hud_wave_text,
                    update_hud_network_status,
                    update_selection_info_text,
                    update_command_card_text,
                    update_match_outcome_banner,
                ),
            );
    }
}

#[derive(Component)]
struct NetworkStatusText;

#[derive(Component)]
struct MineralsText;

#[derive(Component)]
struct SupplyText;

#[derive(Component)]
struct WaveCountdownText;

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
                // Game Title & Session Badge
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
                        title_group.spawn((
                            Text::new("[SESSION B5: DEDICATED SERVER & MULTIPLAYER]"),
                            TextFont {
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.65, 0.72, 0.80)),
                            FocusPolicy::Pass,
                        ));
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
                            Text::new("⚡ Supply: 4 / 10"),
                            TextFont {
                                font_size: 17.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.95, 0.85, 0.25)),
                            SupplyText,
                            FocusPolicy::Pass,
                        ));
                        res_group.spawn((
                            Text::new("⏳ Wave 1 in: 40s"),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.95, 0.40, 0.30)),
                            WaveCountdownText,
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
            // CENTER MATCH OUTCOME BANNER (Hidden until Victory/Defeat)
            // ─────────────────────────────────────────────────────────────────
            root.spawn((
                Node {
                    align_self: AlignSelf::Center,
                    padding: UiRect::axes(Val::Px(32.0), Val::Px(16.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    display: Display::None, // Hidden by default
                    ..default()
                },
                BackgroundColor(Color::srgba(0.04, 0.06, 0.08, 0.96)),
                BorderColor(Color::srgb(0.3, 0.8, 1.0)),
                MatchBannerContainer,
                FocusPolicy::Pass,
            ))
            .with_children(|banner| {
                banner.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 28.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    MatchBannerText,
                    FocusPolicy::Pass,
                ));
            });

            // ─────────────────────────────────────────────────────────────────
            // BOTTOM HUD BAR (Selection Card, Production Queue, Build Menu)
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
                // Left Panel: Selection Info & Production Queue
                bottom_row
                    .spawn((
                        Node {
                            min_width: Val::Px(340.0),
                            padding: UiRect::all(Val::Px(14.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(6.0),
                            ..default()
                        },
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

                // Center-Right Panel: Build Commands & Shortcuts
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
                            Text::new("[B] Barracks (150 💎) | [P] Supply Depot (100 💎) | [H] Base HQ (400 💎)"),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.95, 0.85, 0.35)),
                            BuildMenuText,
                            FocusPolicy::Pass,
                        ));
                        legend.spawn((
                            Text::new("HQ: [V] Train SCV (50 💎, 1 ⚡) | Barracks: [M] Train Marine (100 💎, 2 ⚡)"),
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

fn update_hud_economy_text(
    economy: Res<PlayerEconomy>,
    mut min_query: Query<&mut Text, (With<MineralsText>, Without<SupplyText>)>,
    mut sup_query: Query<&mut Text, (With<SupplyText>, Without<MineralsText>)>,
) {
    let p1_eco = economy.get(Faction::Player1);
    for mut text in &mut min_query {
        text.0 = format!("💎 Minerals: {}", p1_eco.minerals);
    }
    for mut text in &mut sup_query {
        text.0 = format!("⚡ Supply: {} / {}", p1_eco.current_supply, p1_eco.max_supply);
    }
}

fn update_hud_wave_text(
    wave_state: Option<Res<WaveAiState>>,
    outcome: Option<Res<MatchOutcome>>,
    mut text_query: Query<&mut Text, With<WaveCountdownText>>,
) {
    let Some(wave_state) = wave_state else {
        return;
    };
    for mut text in &mut text_query {
        if !wave_state.is_active {
            if outcome.as_deref() == Some(&MatchOutcome::Victory) {
                text.0 = "🏆 Victory! Waves Cleared".to_string();
            } else if outcome.as_deref() == Some(&MatchOutcome::Defeat) {
                text.0 = "💥 Base Fallen".to_string();
            } else {
                text.0 = "🛑 Assault Ended".to_string();
            }
        } else {
            let secs = wave_state.time_until_next_wave.max(0.0) as u32;
            let wave_num = wave_state.current_wave + 1;
            text.0 = format!("⏳ Wave {} in: {}s", wave_num, secs);
        }
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
                text.0 = "🟡 IN LOBBY (WAITING)".to_string();
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
    unit_query: Query<(&Unit, &Faction, &Health, &Selectable, Option<&Worker>)>,
    building_query: Query<(&Building, &Faction, &Health, &Selectable, Option<&ProductionBuilding>)>,
    resource_query: Query<(&ResourceNode, &Selectable)>,
    mut title_query: Query<&mut Text, (With<SelectionTitleText>, Without<SelectionDetailsText>, Without<ProductionQueueText>)>,
    mut details_query: Query<&mut Text, (With<SelectionDetailsText>, Without<SelectionTitleText>, Without<ProductionQueueText>)>,
    mut queue_query: Query<&mut Text, (With<ProductionQueueText>, Without<SelectionTitleText>, Without<SelectionDetailsText>)>,
) {
    let mut selected_units = Vec::new();
    let mut selected_building = None;
    let mut selected_resource = None;

    for (unit, faction, health, selectable, worker_opt) in &unit_query {
        if selectable.is_selected {
            selected_units.push((unit, faction, health, worker_opt));
        }
    }

    for (building, faction, health, selectable, prod_opt) in &building_query {
        if selectable.is_selected {
            selected_building = Some((building, faction, health, prod_opt));
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
    let mut details_str = "Drag left-click to select | Right-click ground to Move, enemy to Attack".to_string();
    let mut queue_str = String::new();

    if let Some((building, faction, health, prod_opt)) = selected_building {
        let fac_str = if *faction == Faction::Player1 { "Player 1" } else { "Hostile" };
        title_str = format!("🏢 {} ({}) - HP: {:.0}/{:.0}", building.name, fac_str, health.current, health.max);

        if !building.is_constructed {
            details_str = format!("⚠️ Under Construction... ({:.0}%)", building.progress() * 100.0);
        } else if let Some(prod) = prod_opt {
            let train_prompt = if building.name.contains("Base HQ") {
                "Press [V] to Train SCV Worker (50 💎, 1 ⚡)"
            } else if building.name.contains("Barracks") {
                "Press [M] to Train Marine Soldier (100 💎, 2 ⚡)"
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
            let (unit, faction, health, worker_opt) = selected_units[0];
            let fac_str = if *faction == Faction::Player1 { "Player 1" } else { "Hostile" };
            title_str = format!("🎖️ {} ({}) - HP: {:.0}/{:.0}", unit.name, fac_str, health.current, health.max);

            if let Some(worker) = worker_opt {
                let state_str = match worker.state {
                    shared::components::WorkerState::Idle => "Idle",
                    shared::components::WorkerState::MovingToResource => "Moving to Mineral Patch",
                    shared::components::WorkerState::Mining => "Harvesting Minerals with Laser",
                    shared::components::WorkerState::MovingToBase => "Returning Minerals to Base HQ",
                };
                details_str = format!("Worker Status: {} | Carried: {} 💎", state_str, worker.carried_minerals);
            } else {
                details_str = "Combat Soldier ready | Right-click to Move / Attack".to_string();
            }
        } else {
            let friendly_count = selected_units.iter().filter(|(_, f, _, _)| **f == Faction::Player1).count();
            let hostile_count = selected_units.len() - friendly_count;
            title_str = format!("Selected: {} Units", selected_units.len());
            details_str = format!("Friendly: {} | Hostile: {} | Right-click to issue squad move / attack", friendly_count, hostile_count);
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
            let status = if placement_state.is_valid { "Valid Location (Left-Click to Place)" } else { "Blocked / Insufficient Funds" };
            text.0 = format!("🏗️ Placing: {} ($ {}) - {} | [Esc/Right-Click] Cancel", kind.name(), placement_state.mineral_cost, status);
        } else {
            text.0 = "[B] Barracks (150 💎) | [P] Supply Depot (100 💎) | [H] Base HQ (400 💎)".to_string();
        }
    }
}

fn update_match_outcome_banner(
    outcome: Option<Res<MatchOutcome>>,
    mut banner_query: Query<(&mut Node, &mut BorderColor), With<MatchBannerContainer>>,
    mut text_query: Query<(&mut Text, &mut TextColor), With<MatchBannerText>>,
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

    match *outcome {
        MatchOutcome::InProgress => {
            node.display = Display::None;
        }
        MatchOutcome::Victory => {
            node.display = Display::Flex;
            *border = BorderColor(Color::srgb(0.20, 0.95, 0.45));
            text.0 = "🏆 VICTORY! Hostile Base HQ Destroyed!".to_string();
            *color = TextColor(Color::srgb(0.25, 0.95, 0.50));
        }
        MatchOutcome::Defeat => {
            node.display = Display::Flex;
            *border = BorderColor(Color::srgb(0.95, 0.25, 0.25));
            text.0 = "💥 DEFEAT! Your Base HQ has Fallen!".to_string();
            *color = TextColor(Color::srgb(0.95, 0.35, 0.35));
        }
    }
}
