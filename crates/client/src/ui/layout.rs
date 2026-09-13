use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use super::{
    spawn_bottom_bar, spawn_countdown_overlay, spawn_game_menu_modal,
    spawn_mobile_quick_bar, spawn_post_match_banner, spawn_top_bar,
};

/// Marker component for the high-level HUD root UI overlay container
#[derive(Component)]
pub struct RootUiContainer;

/// Marker component for the top-right minimap backdrop container
#[derive(Component)]
pub struct MinimapFrame;

/// High-level HUD root setup.
/// Orchestrates the top bar, center overlays/modals, and bottom control console.
pub fn setup_hud(mut commands: Commands) {
    // Root UI container overlay (FocusPolicy::Pass allows mouse clicks to pass to the 2D world)
    commands
        .spawn((
            RootUiContainer,
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
            // Top HUD Bar (Title, APM, Resources, Network Status, Menu Button)
            spawn_top_bar(root);

            // Top-Right Minimap Frame (Clean container without in-game headline)
            spawn_minimap_frame(root);

            // Left Thumb Quick Action Bar (Mobile Touch only)
            spawn_mobile_quick_bar(root);

            // Center Match Outcome Scoreboard (Hidden until Victory/Defeat)
            spawn_post_match_banner(root);

            // Center Countdown Overlay (Hidden until 3-second match start countdown)
            spawn_countdown_overlay(root);

            // ESC / Game Menu Modal Overlay (Hidden until toggled)
            spawn_game_menu_modal(root);

            // Bottom Control Bar (Selection Info Card, Command Card)
            spawn_bottom_bar(root);
        });
}

/// Spawns the top-right minimap backdrop frame without an in-game headline
pub fn spawn_minimap_frame(parent: &mut ChildBuilder) {
    parent.spawn((
        MinimapFrame,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            top: Val::Px(70.0),
            width: Val::Px(170.0),
            height: Val::Px(170.0),
            border: UiRect::all(Val::Px(1.5)),
            ..default()
        },
        BorderRadius::all(Val::Px(4.0)),
        BackgroundColor(Color::srgba(0.04, 0.07, 0.10, 0.85)),
        BorderColor(Color::srgba(0.20, 0.45, 0.70, 0.90)),
        FocusPolicy::Pass,
    ));
}
