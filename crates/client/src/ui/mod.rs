pub mod command_card;
pub mod countdown;
pub mod hud_systems;
pub mod layout;
pub mod menu_modal;
pub mod post_match;

use bevy::prelude::*;
use shared::components::AppState;
use shared::protocol::FactionColor;

pub use command_card::{
    handle_command_card_interactions_system, spawn_command_card_ui,
    update_attack_move_visuals_system, update_command_card_visibility_system,
    AttackMovePending,
};
pub use countdown::update_match_countdown_system;
pub use hud_systems::{
    update_command_card_text, update_hud_economy_text, update_hud_network_status,
    update_selection_info_text,
};
pub use layout::setup_hud;
pub use menu_modal::{
    close_menu_on_game_start, handle_lobby_button_interactions, update_lobby_modal_status_text,
};
pub use post_match::{
    handle_play_again_button_interaction, handle_return_to_landing_button_interaction,
    update_match_outcome_banner,
};

pub struct RtsUiPlugin;

impl Plugin for RtsUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchCountdown>()
            .init_resource::<AttackMovePending>()
            .add_systems(Startup, setup_hud)
            .add_systems(OnEnter(AppState::InGame), close_menu_on_game_start)
            .add_systems(
                Update,
                (
                    update_hud_economy_text,
                    update_hud_network_status,
                    update_selection_info_text,
                    update_command_card_text,
                    update_command_card_visibility_system,
                    handle_command_card_interactions_system,
                    update_attack_move_visuals_system,
                    update_match_outcome_banner,
                    update_match_countdown_system,
                    handle_lobby_button_interactions,
                    handle_play_again_button_interaction,
                    handle_return_to_landing_button_interaction,
                    update_lobby_modal_status_text,
                ),
            );
    }
}

#[derive(Resource, Default, Debug, Clone)]
pub struct MatchCountdown {
    pub is_active: bool,
    pub remaining_seconds: f32,
    pub opponent_name: String,
    pub opponent_color: FactionColor,
    pub last_announced_second: i32,
    pub has_played_go_sound: bool,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum LobbyButtonAction {
    ToggleModal,
    CloseModal,
    ForfeitMatch,
}

#[derive(Component)]
pub struct CountdownOverlayContainer;

#[derive(Component)]
pub struct CountdownNumberText;

#[derive(Component)]
pub struct CountdownSubText;

#[derive(Component)]
pub struct LobbyModalContainer;

#[derive(Component)]
pub struct LobbyStatusText;

#[derive(Component)]
pub struct NetworkStatusText;

#[derive(Component)]
pub struct MineralsText;

#[derive(Component)]
pub struct SupplyText;

#[derive(Component)]
pub struct ApmText;

#[derive(Component)]
pub struct SelectionTitleText;

#[derive(Component)]
pub struct SelectionDetailsText;

#[derive(Component)]
pub struct ProductionQueueText;

#[derive(Component)]
pub struct BuildMenuText;

#[derive(Component)]
pub struct MatchBannerContainer;

#[derive(Component)]
pub struct MatchBannerText;

#[derive(Component)]
pub struct MatchStatsSummaryText;

#[derive(Component)]
pub struct PlayAgainButton;

#[derive(Component)]
pub struct ReturnToLandingButton;

