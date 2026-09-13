use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use shared::components::{MeleeFighter, Selectable, Soldier, Unit, Worker};
use shared::grid::BuildingKind;

use crate::audio_sfx::SoundEffect;
use crate::controls::ControlScheme;
use crate::net::NetClient;
use crate::placement::PlacementState;
use crate::stats::MatchStats;

/// Quick action types for the mobile touch HUD overlay
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MobileQuickAction {
    ToggleBuildMenu,
}

/// Marker component for the mobile round Deselect "X" button
#[derive(Component)]
pub struct MobileDeselectButton;

/// Resource tracking whether the mobile build menu drawer is toggled open
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct MobileBuildMenuOpen(pub bool);

/// Marker component for the container holding mobile quick action buttons (Build button)
#[derive(Component)]
pub struct MobileQuickActionContainer;

/// Marker component for the Build Menu button to update its active glow
#[derive(Component)]
pub struct BuildMenuToggleButton;

/// Marker component for the mobile build menu drawer panel
#[derive(Component)]
pub struct MobileBuildMenuPanel;

/// Action component attached to each building button in the mobile build menu
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct MobileBuildOption(pub BuildingKind);

/// Spawns the mobile BUILD action button on the right side, positioned above the round Deselect "X" button
pub fn spawn_mobile_quick_bar(parent: &mut ChildBuilder) {
    parent
        .spawn((
            MobileQuickActionContainer,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(10.0),
                bottom: Val::Px(64.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                display: Display::None, // Hidden by default on desktop, enabled on mobile
                ..default()
            },
            FocusPolicy::Pass,
        ))
        .with_children(|col| {
            spawn_quick_btn_build_menu(col);
        });
}

fn spawn_quick_btn_build_menu(parent: &mut ChildBuilder) {
    parent
        .spawn((
            Button,
            BuildMenuToggleButton,
            MobileQuickAction::ToggleBuildMenu,
            Node {
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                padding: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(10.0)),
            BackgroundColor(Color::srgba(0.18, 0.28, 0.45, 0.92)),
            BorderColor(Color::srgba(0.35, 0.65, 0.95, 0.80)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("BUILD"),
                TextFont {
                    font_size: 10.5,
                    ..default()
                },
                TextColor(Color::WHITE),
                FocusPolicy::Pass,
            ));
        });
}

/// Spawns the mobile build menu panel with all possible buildings
pub fn spawn_mobile_build_menu(parent: &mut ChildBuilder) {
    parent
        .spawn((
            MobileBuildMenuPanel,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(64.0),
                bottom: Val::Px(16.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(6.0),
                border: UiRect::all(Val::Px(1.5)),
                display: Display::None,
                ..default()
            },
            BorderRadius::all(Val::Px(8.0)),
            BackgroundColor(Color::srgba(0.06, 0.09, 0.14, 0.95)),
            BorderColor(Color::srgba(0.25, 0.50, 0.75, 0.90)),
            FocusPolicy::Pass,
        ))
        .with_children(|panel| {
            // Header
            panel.spawn((
                Text::new("BUILD STRUCTURES"),
                TextFont {
                    font_size: 11.5,
                    ..default()
                },
                TextColor(Color::srgb(0.35, 0.85, 1.0)),
                FocusPolicy::Pass,
            ));

            // Grid of all 4 possible buildings: 2 rows of 2
            let buildings = [
                (BuildingKind::BaseHQ, "Base HQ", 400),
                (BuildingKind::Barracks, "Barracks", 150),
                (BuildingKind::SupplyDepot, "Supply Depot", 100),
                (BuildingKind::Turret, "Gun Turret", 125),
            ];

            // Row 1: HQ and Barracks
            panel
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ))
                .with_children(|row| {
                    spawn_build_option_btn(row, buildings[0].0, buildings[0].1, buildings[0].2);
                    spawn_build_option_btn(row, buildings[1].0, buildings[1].1, buildings[1].2);
                });

            // Row 2: Supply Depot and Turret
            panel
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ))
                .with_children(|row| {
                    spawn_build_option_btn(row, buildings[2].0, buildings[2].1, buildings[2].2);
                    spawn_build_option_btn(row, buildings[3].0, buildings[3].1, buildings[3].2);
                });
        });
}

fn spawn_build_option_btn(
    parent: &mut ChildBuilder,
    kind: BuildingKind,
    name: &str,
    cost: u32,
) {
    parent
        .spawn((
            Button,
            MobileBuildOption(kind),
            Node {
                width: Val::Px(112.0),
                height: Val::Px(42.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(6.0)),
            BackgroundColor(Color::srgba(0.12, 0.18, 0.28, 0.95)),
            BorderColor(Color::srgba(0.30, 0.50, 0.70, 0.80)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(name),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                FocusPolicy::Pass,
            ));
            btn.spawn((
                Text::new(format!("{} Min", cost)),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.85, 0.30)),
                FocusPolicy::Pass,
            ));
        });
}

/// Updates visibility of the mobile build menu panel based on MobileBuildMenuOpen and platform
pub fn update_mobile_build_menu_visibility_system(
    build_menu: Res<MobileBuildMenuOpen>,
    scheme: Res<ControlScheme>,
    net_client: Res<NetClient>,
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut panel_query: Query<&mut Node, With<MobileBuildMenuPanel>>,
) {
    let win_mobile = window_query
        .get_single()
        .map_or(false, |w| w.width() < 960.0 || w.height() < 550.0);
    let is_mobile = *scheme == ControlScheme::MobileTouch
        || net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_mobile;

    let should_show = is_mobile && build_menu.0;

    for mut node in &mut panel_query {
        node.display = if should_show {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Handles touches/clicks on buildings within the mobile build menu
pub fn handle_mobile_build_menu_interactions(
    mut interaction_query: Query<
        (&Interaction, &MobileBuildOption, &mut BackgroundColor, &mut BorderColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut mobile_build_menu: ResMut<MobileBuildMenuOpen>,
    mut placement_state: ResMut<PlacementState>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
) {
    for (interaction, option, mut bg, mut border) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                let kind = option.0;
                placement_state.active_kind = Some(kind);
                placement_state.mineral_cost = kind.mineral_cost();
                placement_state.awaiting_initial_release = true;
                placement_state.has_preview_position = false;

                // Close build menu so full battlefield is visible for choosing placement spot
                mobile_build_menu.0 = false;
                stats.record_action();
                sound_events.send(SoundEffect::OrderIssued);
                info!("🏗️ [Mobile Build] Selected {:?} ($ {}) for placement", kind, kind.mineral_cost());
            }
            Interaction::Hovered => {
                bg.0 = Color::srgba(0.20, 0.35, 0.55, 0.95);
                border.0 = Color::srgba(0.45, 0.75, 1.0, 1.0);
            }
            Interaction::None => {
                bg.0 = Color::srgba(0.12, 0.18, 0.28, 0.95);
                border.0 = Color::srgba(0.30, 0.50, 0.70, 0.80);
            }
        }
    }
}

/// Updates visibility of the mobile BUILD button based on active ControlScheme or mobile platform
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

/// Updates the Build Menu button visual when toggled
pub fn update_build_menu_visuals_system(
    build_menu: Res<MobileBuildMenuOpen>,
    placement_state: Res<PlacementState>,
    mut btn_query: Query<(&mut BackgroundColor, &mut BorderColor), With<BuildMenuToggleButton>>,
) {
    for (mut bg, mut border) in &mut btn_query {
        if build_menu.0 || placement_state.active_kind.is_some() {
            bg.0 = Color::srgba(0.15, 0.60, 0.85, 0.95);
            border.0 = Color::srgba(0.50, 0.90, 1.0, 1.0);
        } else {
            bg.0 = Color::srgba(0.18, 0.28, 0.45, 0.92);
            border.0 = Color::srgba(0.35, 0.65, 0.95, 0.80);
        }
    }
}

/// Handles touches/clicks on the mobile BUILD action button
pub fn handle_mobile_quick_action_interactions(
    mut interaction_query: Query<
        (&Interaction, &MobileQuickAction),
        (Changed<Interaction>, With<Button>),
    >,
    mut mobile_build_menu: ResMut<MobileBuildMenuOpen>,
    mut placement_state: ResMut<PlacementState>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
) {
    for (interaction, action) in &mut interaction_query {
        info!("📱 [Quick Action] Interaction changed: {:?} for action {:?}", interaction, action);
        if *interaction == Interaction::Pressed {
            match action {
                MobileQuickAction::ToggleBuildMenu => {
                    // If building placement is active, pressing BUILD cancels placement and toggles menu
                    if placement_state.active_kind.is_some() {
                        placement_state.active_kind = None;
                        placement_state.has_preview_position = false;
                    }
                    mobile_build_menu.0 = !mobile_build_menu.0;
                    stats.record_action();
                    sound_events.send(SoundEffect::OrderIssued);
                    info!("📱 [Controls] Build Menu Open: {}", mobile_build_menu.0);
                }
            }
        }
    }
}

/// Spawns the mobile round Deselect "X" button in the bottom right corner
pub fn spawn_mobile_deselect_button(parent: &mut ChildBuilder) {
    parent
        .spawn((
            Button,
            MobileDeselectButton,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(10.0),
                bottom: Val::Px(12.0),
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                display: Display::None, // Hidden by default, shown when unit selected or placement active on mobile
                ..default()
            },
            BorderRadius::all(Val::Percent(50.0)),
            BackgroundColor(Color::srgba(0.65, 0.12, 0.16, 0.92)),
            BorderColor(Color::srgba(0.95, 0.30, 0.35, 0.90)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("X"),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                FocusPolicy::Pass,
            ));
        });
}

/// Updates visibility of the mobile round Deselect button based on platform, unit selection, and placement state
pub fn update_mobile_deselect_button_visibility_system(
    scheme: Res<ControlScheme>,
    net_client: Res<NetClient>,
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
    selectable_query: Query<(
        &Selectable,
        Option<&Unit>,
        Option<&Worker>,
        Option<&Soldier>,
        Option<&MeleeFighter>,
    )>,
    placement_state: Res<PlacementState>,
    mut button_query: Query<&mut Node, With<MobileDeselectButton>>,
) {
    let win_mobile = window_query
        .get_single()
        .map_or(false, |w| w.width() < 960.0 || w.height() < 550.0);
    let is_mobile = *scheme == ControlScheme::MobileTouch
        || net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_mobile;

    let has_selected_unit = selectable_query.iter().any(|(sel, u, w, s, m)| {
        sel.is_selected && (u.is_some() || w.is_some() || s.is_some() || m.is_some())
    });

    let is_placing = placement_state.active_kind.is_some();
    let should_show = is_mobile && (has_selected_unit || is_placing);

    for mut node in &mut button_query {
        node.display = if should_show {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Handles interactions with the mobile round Deselect "X" button
pub fn handle_mobile_deselect_button_interaction(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (Changed<Interaction>, With<Button>, With<MobileDeselectButton>),
    >,
    mut selectable_query: Query<&mut Selectable>,
    mut attack_move_pending: Option<ResMut<crate::ui::command_card::AttackMovePending>>,
    mut placement_state: ResMut<PlacementState>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
) {
    for (interaction, mut bg, mut border) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                bg.0 = Color::srgba(0.90, 0.20, 0.25, 1.0);
                border.0 = Color::srgba(1.0, 0.60, 0.65, 1.0);

                let mut action_taken = false;

                // 1. Cancel building placement if active
                if placement_state.active_kind.is_some() {
                    placement_state.active_kind = None;
                    placement_state.has_preview_position = false;
                    action_taken = true;
                    info!("❌ [Build Mode] Placement cancelled via mobile Deselect button");
                }

                // 2. Clear unit selections
                for mut selectable in &mut selectable_query {
                    if selectable.is_selected {
                        selectable.is_selected = false;
                        action_taken = true;
                    }
                }

                if let Some(ref mut amp) = attack_move_pending {
                    amp.0 = false;
                }

                if action_taken {
                    stats.record_action();
                    sound_events.send(SoundEffect::OrderIssued);
                    info!("📱 [Controls] Units de-selected / build cancelled via mobile Deselect button");
                }
            }
            Interaction::Hovered => {
                bg.0 = Color::srgba(0.78, 0.18, 0.22, 0.95);
                border.0 = Color::srgba(1.0, 0.45, 0.50, 1.0);
            }
            Interaction::None => {
                bg.0 = Color::srgba(0.65, 0.12, 0.16, 0.92);
                border.0 = Color::srgba(0.95, 0.30, 0.35, 0.90);
            }
        }
    }
}

/// Marker component for the mobile placement prompt pill banner
#[derive(Component)]
pub struct MobilePlacementPrompt;

/// Marker component for the mobile placement prompt banner text
#[derive(Component)]
pub struct MobilePlacementPromptText;

/// Spawns the sleek mobile placement prompt banner
pub fn spawn_mobile_placement_prompt(parent: &mut ChildBuilder) {
    parent
        .spawn((
            MobilePlacementPrompt,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(40.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-140.0)),
                width: Val::Px(280.0),
                height: Val::Px(26.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                display: Display::None,
                ..default()
            },
            BorderRadius::all(Val::Px(13.0)),
            BackgroundColor(Color::srgba(0.08, 0.14, 0.22, 0.92)),
            BorderColor(Color::srgba(0.35, 0.75, 1.0, 0.85)),
            FocusPolicy::Pass,
        ))
        .with_children(|prompt| {
            prompt.spawn((
                MobilePlacementPromptText,
                Text::new(""),
                TextFont {
                    font_size: 11.5,
                    ..default()
                },
                TextColor(Color::srgb(0.40, 0.85, 1.0)),
                FocusPolicy::Pass,
            ));
        });
}

/// Dynamically updates the mobile placement prompt banner text and visibility
pub fn update_mobile_placement_prompt_system(
    scheme: Res<ControlScheme>,
    net_client: Res<NetClient>,
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
    placement_state: Res<PlacementState>,
    mut prompt_query: Query<&mut Node, With<MobilePlacementPrompt>>,
    mut text_query: Query<&mut Text, With<MobilePlacementPromptText>>,
) {
    let win_mobile = window_query.get_single().map_or(false, |w| w.width() < 960.0 || w.height() < 550.0);
    let is_mobile = *scheme == ControlScheme::MobileTouch
        || net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_mobile;

    let is_placing = is_mobile && placement_state.active_kind.is_some();

    for mut node in &mut prompt_query {
        node.display = if is_placing { Display::Flex } else { Display::None };
    }

    if is_placing {
        if let Some(kind) = placement_state.active_kind {
            for mut text in &mut text_query {
                text.0 = format!("Tap map to place {} (${})", kind.name(), placement_state.mineral_cost);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::window::{PrimaryWindow, WindowResolution};

    #[test]
    fn test_mobile_deselect_button_visibility() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(ControlScheme::MobileTouch);
        app.insert_resource(NetClient::default());
        app.init_resource::<PlacementState>();

        let mut window = Window::default();
        window.resolution = WindowResolution::new(955.0, 440.0);
        app.world_mut().spawn((window, PrimaryWindow));

        // Spawn deselect button
        let btn_entity = app
            .world_mut()
            .spawn((
                MobileDeselectButton,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();

        // Spawn a Worker (unselected initially)
        let worker_entity = app
            .world_mut()
            .spawn((
                Selectable { is_selected: false },
                Worker::default(),
                Unit {
                    name: "Worker".to_string(),
                    supply_cost: 1,
                },
            ))
            .id();

        // 1. Unselected unit and no placement -> Button should remain hidden (Display::None)
        app.world_mut()
            .run_system_once(update_mobile_deselect_button_visibility_system)
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(btn_entity).unwrap().display,
            Display::None
        );

        // 2. Select the worker -> Button should become visible (Display::Flex)
        app.world_mut()
            .get_mut::<Selectable>(worker_entity)
            .unwrap()
            .is_selected = true;
        app.world_mut()
            .run_system_once(update_mobile_deselect_button_visibility_system)
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(btn_entity).unwrap().display,
            Display::Flex
        );

        // 3. Deselect unit but activate placement -> Button should stay visible
        app.world_mut()
            .get_mut::<Selectable>(worker_entity)
            .unwrap()
            .is_selected = false;
        app.world_mut()
            .resource_mut::<PlacementState>()
            .active_kind = Some(BuildingKind::Barracks);
        app.world_mut()
            .run_system_once(update_mobile_deselect_button_visibility_system)
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(btn_entity).unwrap().display,
            Display::Flex
        );

        // 4. Desktop mode -> Button should stay hidden even when placing or selected
        app.world_mut()
            .insert_resource(ControlScheme::DesktopMouseKeyboard);
        let mut win = app
            .world_mut()
            .query::<&mut Window>()
            .single_mut(app.world_mut());
        win.resolution = WindowResolution::new(1920.0, 1080.0);

        app.world_mut()
            .run_system_once(update_mobile_deselect_button_visibility_system)
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(btn_entity).unwrap().display,
            Display::None
        );
    }

    #[test]
    fn test_mobile_deselect_button_clears_selection_and_placement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<MatchStats>();
        app.init_resource::<PlacementState>();
        app.add_event::<SoundEffect>();

        let _btn_entity = app
            .world_mut()
            .spawn((
                MobileDeselectButton,
                Button,
                Interaction::Pressed,
                BackgroundColor(Color::srgba(0.65, 0.12, 0.16, 0.92)),
                BorderColor(Color::srgba(0.95, 0.30, 0.35, 0.90)),
            ))
            .id();

        let w1 = app
            .world_mut()
            .spawn((
                Selectable { is_selected: true },
                Worker::default(),
            ))
            .id();

        app.world_mut()
            .resource_mut::<PlacementState>()
            .active_kind = Some(BuildingKind::Turret);

        // Run the interaction system
        app.world_mut()
            .run_system_once(handle_mobile_deselect_button_interaction)
            .unwrap();

        // Unit should be deselected and placement cancelled
        assert!(!app.world().get::<Selectable>(w1).unwrap().is_selected);
        assert!(app.world().resource::<PlacementState>().active_kind.is_none());

        // Action was recorded in MatchStats
        assert_eq!(app.world().resource::<MatchStats>().total_commands, 1);
    }

    #[test]
    fn test_mobile_build_menu_selection_initiates_placement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<MatchStats>();
        app.init_resource::<PlacementState>();
        app.insert_resource(MobileBuildMenuOpen(true));
        app.add_event::<SoundEffect>();

        // Spawn 2D Camera
        app.world_mut().spawn((
            Camera::default(),
            Camera2d,
            Transform::from_xyz(100.0, 200.0, 0.0),
        ));

        // Spawn Barracks build button in pressed state
        let _btn = app
            .world_mut()
            .spawn((
                Button,
                MobileBuildOption(BuildingKind::Barracks),
                Interaction::Pressed,
                BackgroundColor(Color::srgba(0.12, 0.18, 0.28, 0.95)),
                BorderColor(Color::srgba(0.30, 0.50, 0.70, 0.80)),
            ))
            .id();

        app.world_mut()
            .run_system_once(handle_mobile_build_menu_interactions)
            .unwrap();

        // Build menu should close
        assert!(!app.world().resource::<MobileBuildMenuOpen>().0);

        // Placement state should be active for Barracks
        let ps = app.world().resource::<PlacementState>();
        assert_eq!(ps.active_kind, Some(BuildingKind::Barracks));
        assert_eq!(ps.mineral_cost, 150);
        assert!(ps.awaiting_initial_release);
        // Ghost must NOT be centered at camera pos / Command Center before player touches map
        assert!(!ps.has_preview_position);
    }

    #[test]
    fn test_mobile_placement_prompt_visibility_and_text() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(ControlScheme::MobileTouch);
        app.insert_resource(NetClient::default());
        app.init_resource::<PlacementState>();

        let mut window = Window::default();
        window.resolution = WindowResolution::new(955.0, 440.0);
        app.world_mut().spawn((window, PrimaryWindow));

        // Spawn prompt banner and text
        let prompt_entity = app
            .world_mut()
            .spawn((
                MobilePlacementPrompt,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();

        let text_entity = app
            .world_mut()
            .spawn((
                MobilePlacementPromptText,
                Text::new(""),
            ))
            .id();

        // 1. When not placing, prompt remains hidden
        app.world_mut()
            .run_system_once(update_mobile_placement_prompt_system)
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(prompt_entity).unwrap().display,
            Display::None
        );

        // 2. When placing a building on mobile, prompt is shown with instruction
        app.world_mut()
            .resource_mut::<PlacementState>()
            .active_kind = Some(BuildingKind::Barracks);
        app.world_mut()
            .resource_mut::<PlacementState>()
            .mineral_cost = 150;

        app.world_mut()
            .run_system_once(update_mobile_placement_prompt_system)
            .unwrap();

        assert_eq!(
            app.world().get::<Node>(prompt_entity).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            app.world().get::<Text>(text_entity).unwrap().0,
            "Tap map to place Barracks ($150)"
        );
    }
}
