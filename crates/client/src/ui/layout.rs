use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use super::{
    spawn_bottom_bar, spawn_countdown_overlay, spawn_game_menu_modal,
    spawn_post_match_banner, spawn_top_bar,
};

/// High-level HUD root setup.
/// Orchestrates the top bar, center overlays/modals, and bottom control console.
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
            // Top HUD Bar (Title, APM, Resources, Network Status, Menu Button)
            spawn_top_bar(root);

            // Center Match Outcome Scoreboard (Hidden until Victory/Defeat)
            spawn_post_match_banner(root);

            // Center Countdown Overlay (Hidden until 3-second match start countdown)
            spawn_countdown_overlay(root);

            // ESC / Game Menu Modal Overlay (Hidden until toggled)
            spawn_game_menu_modal(root);

            // Bottom Control Bar (Minimap / Radar Frame, Selection Info Card, Command Card)
            spawn_bottom_bar(root);
        });
}
