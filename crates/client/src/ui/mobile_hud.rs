use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use shared::components::{MeleeFighter, Selectable, Soldier, Unit, Worker};

use crate::audio_sfx::SoundEffect;
use crate::controls::ControlScheme;
use crate::net::NetClient;
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

/// Marker component for the container holding mobile quick action buttons
#[derive(Component)]
pub struct MobileQuickActionContainer;

/// Marker component for the Build Menu button to update its active glow
#[derive(Component)]
pub struct BuildMenuToggleButton;

/// Spawns the mobile quick action thumb bar with the BUILD toggle button
pub fn spawn_mobile_quick_bar(parent: &mut ChildBuilder) {
    parent
        .spawn((
            MobileQuickActionContainer,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                bottom: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
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
    mut mobile_build_menu: ResMut<MobileBuildMenuOpen>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
) {
    for (interaction, action) in &mut interaction_query {
        info!("📱 [Quick Action] Interaction changed: {:?} for action {:?}", interaction, action);
        if *interaction == Interaction::Pressed {
            match action {
                MobileQuickAction::ToggleBuildMenu => {
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
                display: Display::None, // Hidden by default, shown when unit selected on mobile
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

/// Updates visibility of the mobile round Deselect button based on platform and unit selection
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

    let should_show = is_mobile && has_selected_unit;

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
        (Changed<Interaction>, With<MobileDeselectButton>),
    >,
    mut selectable_query: Query<&mut Selectable>,
    mut attack_move_pending: Option<ResMut<crate::ui::command_card::AttackMovePending>>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
) {
    for (interaction, mut bg, mut border) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                bg.0 = Color::srgba(0.90, 0.20, 0.25, 1.0);
                border.0 = Color::srgba(1.0, 0.60, 0.65, 1.0);

                let mut any_deselected = false;
                for mut selectable in &mut selectable_query {
                    if selectable.is_selected {
                        selectable.is_selected = false;
                        any_deselected = true;
                    }
                }

                if let Some(ref mut amp) = attack_move_pending {
                    amp.0 = false;
                }

                if any_deselected {
                    stats.record_action();
                    sound_events.send(SoundEffect::OrderIssued);
                    info!("📱 [Controls] Units de-selected via mobile Deselect button");
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

        // 1. Unselected unit -> Button should remain hidden (Display::None)
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

        // 3. Desktop mode -> Button should stay hidden even when unit is selected
        app.world_mut()
            .insert_resource(ControlScheme::DesktopMouseKeyboard);
        // Set window to desktop size so win_mobile is false
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
    fn test_mobile_deselect_button_clears_selection() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<MatchStats>();
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

        let s1 = app
            .world_mut()
            .spawn((
                Selectable { is_selected: true },
                Soldier::default(),
            ))
            .id();

        // Run the interaction system
        app.world_mut()
            .run_system_once(handle_mobile_deselect_button_interaction)
            .unwrap();

        // Both units should now be deselected
        assert!(!app.world().get::<Selectable>(w1).unwrap().is_selected);
        assert!(!app.world().get::<Selectable>(s1).unwrap().is_selected);

        // Action was recorded in MatchStats
        assert_eq!(app.world().resource::<MatchStats>().total_commands, 1);
    }
}
