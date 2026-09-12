use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use super::{
    spawn_command_card_ui, ProductionQueueText, SelectionDetailsText, SelectionTitleText,
};

/// Spawns the bottom HUD bar containing the selection/queue info card and command card
pub fn spawn_bottom_bar(parent: &mut ChildBuilder) {
    parent
        .spawn((
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

            // Right Panel: Interactive Context-Sensitive Command Card
            spawn_command_card_ui(bottom_row);
        });
}

