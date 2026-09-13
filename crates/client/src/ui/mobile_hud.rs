use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use shared::components::{Faction, MeleeFighter, Selectable, Soldier, Worker};

use crate::audio_sfx::SoundEffect;
use crate::controls::{BoxSelectMode, ControlScheme};
use crate::net::NetClient;
use crate::stats::MatchStats;

/// Quick action types for the mobile touch HUD overlay
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MobileQuickAction {
    SelectAllArmy,
    SelectWorkers,
    ToggleBuildMenu,
    ToggleBoxSelect,
    ClearSelection,
}

/// Resource tracking whether the mobile build menu drawer is toggled open
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct MobileBuildMenuOpen(pub bool);

/// Marker component for the container holding mobile quick action buttons
#[derive(Component)]
pub struct MobileQuickActionContainer;

/// Marker component for the Box Select button to update its active glow
#[derive(Component)]
pub struct BoxSelectToggleButton;

/// Marker component for the Build Menu button to update its active glow
#[derive(Component)]
pub struct BuildMenuToggleButton;

/// Spawns the mobile quick action thumb bar
pub fn spawn_mobile_quick_bar(parent: &mut ChildBuilder) {
    parent
        .spawn((
            MobileQuickActionContainer,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                bottom: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                align_items: AlignItems::FlexStart,
                display: Display::None, // Hidden by default on desktop, enabled on mobile
                ..default()
            },
            FocusPolicy::Pass,
        ))
        .with_children(|col| {
            spawn_quick_btn(
                col,
                MobileQuickAction::SelectAllArmy,
                "ARMY",
                Color::srgba(0.15, 0.40, 0.80, 0.85),
            );
            spawn_quick_btn(
                col,
                MobileQuickAction::SelectWorkers,
                "WORKERS",
                Color::srgba(0.12, 0.50, 0.40, 0.85),
            );
            spawn_quick_btn_build_menu(col);
            spawn_quick_btn_box_select(col);
            spawn_quick_btn(
                col,
                MobileQuickAction::ClearSelection,
                "CLEAR",
                Color::srgba(0.35, 0.20, 0.25, 0.85),
            );
        });
}

fn spawn_quick_btn(
    parent: &mut ChildBuilder,
    action: MobileQuickAction,
    label: &str,
    bg_col: Color,
) {
    parent
        .spawn((
            Button,
            action,
            Node {
                min_width: Val::Px(85.0),
                height: Val::Px(30.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(6.0)),
            BackgroundColor(bg_col),
            BorderColor(Color::srgba(0.40, 0.65, 0.90, 0.40)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                FocusPolicy::Pass,
            ));
        });
}

fn spawn_quick_btn_build_menu(parent: &mut ChildBuilder) {
    parent
        .spawn((
            Button,
            BuildMenuToggleButton,
            MobileQuickAction::ToggleBuildMenu,
            Node {
                min_width: Val::Px(85.0),
                height: Val::Px(30.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(6.0)),
            BackgroundColor(Color::srgba(0.20, 0.28, 0.45, 0.85)),
            BorderColor(Color::srgba(0.40, 0.60, 0.80, 0.40)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("BUILD"),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                FocusPolicy::Pass,
            ));
        });
}

fn spawn_quick_btn_box_select(parent: &mut ChildBuilder) {
    parent
        .spawn((
            Button,
            BoxSelectToggleButton,
            MobileQuickAction::ToggleBoxSelect,
            Node {
                min_width: Val::Px(85.0),
                height: Val::Px(30.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(6.0)),
            BackgroundColor(Color::srgba(0.18, 0.22, 0.32, 0.85)),
            BorderColor(Color::srgba(0.40, 0.60, 0.80, 0.40)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("BOX SELECT"),
                TextFont {
                    font_size: 10.5,
                    ..default()
                },
                TextColor(Color::WHITE),
                FocusPolicy::Pass,
            ));
        });
}

/// Updates visibility of the mobile quick-action thumb bar based on active ControlScheme or mobile platform
pub fn update_mobile_hud_visibility_system(
    scheme: Res<ControlScheme>,
    net_client: Res<NetClient>,
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut container_query: Query<&mut Node, With<MobileQuickActionContainer>>,
) {
    let win_mobile = window_query.get_single().map_or(false, |w| w.width() < 960.0 || w.height() < 550.0);
    let is_mobile = *scheme == ControlScheme::MobileTouch
        || net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_mobile;

    for mut node in &mut container_query {
        node.display = if is_mobile {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Updates the Box Select button visual when toggled
pub fn update_box_select_visuals_system(
    box_select: Res<BoxSelectMode>,
    mut btn_query: Query<(&mut BackgroundColor, &mut BorderColor), With<BoxSelectToggleButton>>,
) {
    for (mut bg, mut border) in &mut btn_query {
        if box_select.0 {
            bg.0 = Color::srgba(0.10, 0.70, 0.35, 0.95);
            border.0 = Color::srgba(0.60, 1.0, 0.70, 1.0);
        } else {
            bg.0 = Color::srgba(0.18, 0.22, 0.32, 0.85);
            border.0 = Color::srgba(0.40, 0.60, 0.80, 0.40);
        }
    }
}

/// Updates the Build Menu button visual when toggled
pub fn update_build_menu_visuals_system(
    build_menu: Res<MobileBuildMenuOpen>,
    mut btn_query: Query<(&mut BackgroundColor, &mut BorderColor), With<BuildMenuToggleButton>>,
) {
    for (mut bg, mut border) in &mut btn_query {
        if build_menu.0 {
            bg.0 = Color::srgba(0.15, 0.60, 0.85, 0.95);
            border.0 = Color::srgba(0.50, 0.90, 1.0, 1.0);
        } else {
            bg.0 = Color::srgba(0.20, 0.28, 0.45, 0.85);
            border.0 = Color::srgba(0.40, 0.60, 0.80, 0.40);
        }
    }
}

/// Handles touches/clicks on the mobile quick action thumb bar
pub fn handle_mobile_quick_action_interactions(
    mut interaction_query: Query<
        (&Interaction, &MobileQuickAction),
        (Changed<Interaction>, With<Button>),
    >,
    mut box_select: ResMut<BoxSelectMode>,
    mut mobile_build_menu: ResMut<MobileBuildMenuOpen>,
    net_client: Res<NetClient>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
    mut selectable_query: Query<(
        Entity,
        &Faction,
        &mut Selectable,
        Option<&Soldier>,
        Option<&MeleeFighter>,
        Option<&Worker>,
    )>,
) {
    for (interaction, action) in &mut interaction_query {
        info!("📱 [Quick Action] Interaction changed: {:?} for action {:?}", interaction, action);
        if *interaction == Interaction::Pressed {
            match action {
                MobileQuickAction::SelectAllArmy => {
                    mobile_build_menu.0 = false;
                    let mut any = false;
                    for (_, fac, mut sel, sol_opt, mel_opt, wrk_opt) in &mut selectable_query {
                        if *fac == net_client.my_faction {
                            if sol_opt.is_some() || mel_opt.is_some() {
                                sel.is_selected = true;
                                any = true;
                            } else if wrk_opt.is_some() {
                                sel.is_selected = false;
                            }
                        }
                    }
                    if any {
                        stats.record_action();
                        sound_events.send(SoundEffect::MarineSelect);
                    }
                }
                MobileQuickAction::SelectWorkers => {
                    mobile_build_menu.0 = false;
                    let mut any = false;
                    for (_, fac, mut sel, _, _, wrk_opt) in &mut selectable_query {
                        if *fac == net_client.my_faction {
                            if wrk_opt.is_some() {
                                sel.is_selected = true;
                                any = true;
                            } else {
                                sel.is_selected = false;
                            }
                        }
                    }
                    if any {
                        stats.record_action();
                        sound_events.send(SoundEffect::WorkerSelect);
                    }
                }
                MobileQuickAction::ToggleBuildMenu => {
                    mobile_build_menu.0 = !mobile_build_menu.0;
                    stats.record_action();
                    sound_events.send(SoundEffect::OrderIssued);
                    info!("📱 [Controls] Build Menu Open: {}", mobile_build_menu.0);
                }
                MobileQuickAction::ToggleBoxSelect => {
                    box_select.0 = !box_select.0;
                    stats.record_action();
                    sound_events.send(SoundEffect::OrderIssued);
                    info!("📱 [Controls] Box Select Mode: {}", box_select.0);
                }
                MobileQuickAction::ClearSelection => {
                    mobile_build_menu.0 = false;
                    for (_, _, mut sel, ..) in &mut selectable_query {
                        sel.is_selected = false;
                    }
                }
            }
        }
    }
}
