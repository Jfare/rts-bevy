use bevy::prelude::*;
use shared::components::{
    Building, Faction, GunTurret, Health, MeleeFighter, ProductionBuilding, ResourceNode, Selectable,
    Soldier, TacticalStance, Unit, Worker,
};
use shared::economy::PlayerEconomy;

use crate::net::{NetClient, NetStatus};
use crate::placement::PlacementState;
use crate::stats::MatchStats;
use super::{
    ApmText, BuildMenuText, MineralsText, NetworkStatusText, ProductionQueueText,
    SelectionDetailsText, SelectionTitleText, SupplyText,
};

pub fn update_hud_economy_text(
    economy: Res<PlayerEconomy>,
    net_client: Res<NetClient>,
    stats: Res<MatchStats>,
    mut min_query: Query<&mut Text, (With<MineralsText>, Without<SupplyText>, Without<ApmText>)>,
    mut sup_query: Query<&mut Text, (With<SupplyText>, Without<MineralsText>, Without<ApmText>)>,
    mut apm_query: Query<&mut Text, (With<ApmText>, Without<MineralsText>, Without<SupplyText>)>,
) {
    let my_eco = economy.get(net_client.my_faction);
    for mut text in &mut min_query {
        text.0 = format!("Gold: {}", my_eco.minerals);
    }
    for mut text in &mut sup_query {
        text.0 = format!("Supply: {} / {}", my_eco.current_supply, my_eco.max_supply);
    }
    for mut text in &mut apm_query {
        text.0 = format!("APM: {}", stats.current_apm());
    }
}

pub fn update_hud_network_status(
    net_client: Res<NetClient>,
    mut text_query: Query<(&mut Text, &mut TextColor), With<NetworkStatusText>>,
) {
    for (mut text, mut color) in &mut text_query {
        match net_client.status {
            NetStatus::InGame => {
                let opp_badge = net_client
                    .opponent_platform
                    .map(|p| format!(" vs {}", p.badge()))
                    .unwrap_or_default();
                text.0 = format!("LIVE ({}ms){}", net_client.rtt_ms, opp_badge);
                color.0 = Color::srgb(0.25, 0.95, 0.45);
            }
            NetStatus::InLobby => {
                text.0 = "SEARCHING (1/2)".to_string();
                color.0 = Color::srgb(0.95, 0.85, 0.25);
            }
            NetStatus::Connected => {
                text.0 = "CONNECTED".to_string();
                color.0 = Color::srgb(0.25, 0.95, 0.45);
            }
            NetStatus::Connecting => {
                text.0 = "CONNECTING...".to_string();
                color.0 = Color::srgb(0.95, 0.85, 0.25);
            }
            NetStatus::Disconnected => {
                text.0 = "OFFLINE (SOLO)".to_string();
                color.0 = Color::srgb(0.60, 0.65, 0.70);
            }
        }
    }
}

pub fn update_selection_info_text(
    unit_query: Query<(
        &Unit,
        &Faction,
        &Health,
        &Selectable,
        Option<&Worker>,
        Option<&Soldier>,
        Option<&MeleeFighter>,
        Option<&TacticalStance>,
    )>,
    building_query: Query<(&Building, &Faction, &Health, &Selectable, Option<&ProductionBuilding>, Option<&GunTurret>)>,
    resource_query: Query<(&ResourceNode, &Selectable)>,
    mut title_query: Query<&mut Text, (With<SelectionTitleText>, Without<SelectionDetailsText>, Without<ProductionQueueText>)>,
    mut details_query: Query<&mut Text, (With<SelectionDetailsText>, Without<SelectionTitleText>, Without<ProductionQueueText>)>,
    mut queue_query: Query<&mut Text, (With<ProductionQueueText>, Without<SelectionTitleText>, Without<SelectionDetailsText>)>,
    mut panel_query: Query<&mut Node, (With<super::bottom_bar::SelectionCardPanel>, Without<super::command_card::CommandCardRoot>)>,
    control_scheme: Option<Res<crate::controls::ControlScheme>>,
    net_client: Res<NetClient>,
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let win_mobile = window_query.get_single().map_or(false, |w| w.width() < 960.0 || w.height() < 550.0);
    let is_mobile = control_scheme.map_or(false, |s| *s == crate::controls::ControlScheme::MobileTouch)
        || net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_mobile;

    let mut selected_units = Vec::new();
    let mut selected_building = None;
    let mut selected_resource = None;

    for (unit, faction, health, selectable, worker_opt, soldier_opt, melee_opt, stance_opt) in &unit_query {
        if selectable.is_selected {
            selected_units.push((unit, faction, health, worker_opt, soldier_opt, melee_opt, stance_opt));
        }
    }

    for (building, faction, health, selectable, prod_opt, turret_opt) in &building_query {
        if selectable.is_selected {
            selected_building = Some((building, faction, health, prod_opt, turret_opt));
            break;
        }
    }

    for (resource, selectable) in &resource_query {
        if selectable.is_selected {
            selected_resource = Some(resource);
            break;
        }
    }

    let has_any_selection = !selected_units.is_empty() || selected_building.is_some() || selected_resource.is_some();

    // On mobile, hide the entire selection panel when nothing is selected to keep battlefield completely clear
    for mut node in &mut panel_query {
        node.display = if is_mobile {
            if has_any_selection { Display::Flex } else { Display::None }
        } else {
            Display::Flex
        };
    }

    let mut title_str = "No Units Selected".to_string();
    let mut details_str = if is_mobile {
        String::new()
    } else {
        "Drag left-click to select | Right-click Move / Attack | [S] Stop | [H] Hold".to_string()
    };
    let mut queue_str = String::new();

    if let Some((building, faction, health, prod_opt, turret_opt)) = selected_building {
        let fac_str = if *faction == Faction::Player1 { "Player 1" } else if *faction == Faction::Player2 { "Player 2" } else { "Hostile" };
        title_str = format!("{} ({}) - HP: {:.0}/{:.0}", building.name, fac_str, health.current, health.max);

        if !building.is_constructed {
            details_str = format!("Under Construction... ({:.0}%)", building.progress() * 100.0);
        } else if turret_opt.is_some() {
            details_str = "Automated Twin-Cannon Defense | 360° Arc (18 DMG, 220 Range)".to_string();
        } else if let Some(prod) = prod_opt {
            let train_prompt = if building.name.contains("Base HQ") {
                if is_mobile { "Train Worker (50 Gold, 1 Supply)" } else { "Press [V]/[W] to Train Worker (50 Gold, 1 Supply)" }
            } else if building.name.contains("Barracks") {
                if is_mobile { "Train Ranged (100 Gold) / Melee (75 Gold)" } else { "Press [R] Ranged (100 Gold) | [F] Melee (75 Gold)" }
            } else if is_mobile {
                "Tap ground to set Rally Point"
            } else {
                "Right-click ground to set Rally Point"
            };
            details_str = train_prompt.to_string();

            if !prod.queue.is_empty() {
                let first = &prod.queue[0];
                let progress = (prod.current_timer / first.build_duration).clamp(0.0, 1.0) * 100.0;
                let queued_names: Vec<_> = prod.queue.iter().map(|q| q.name.clone()).collect();
                queue_str = format!("Queue: {} ({:.0}%) | Queued: {}", queued_names.join(", "), progress, prod.queue.len());
            }
        }
    } else if let Some(resource) = selected_resource {
        title_str = "Gold Rock Deposit".to_string();
        details_str = format!("Remaining Gold: {} / {}", resource.remaining_minerals, resource.max_minerals);
    } else if !selected_units.is_empty() {
        if selected_units.len() == 1 {
            let (unit, _, health, worker_opt, soldier_opt, _, stance_opt) = selected_units[0];
            title_str = format!("{} - HP: {:.0}/{:.0}", unit.name, health.current, health.max);

            let stance_suffix = match stance_opt {
                Some(TacticalStance::HoldPosition) => " [HOLD]",
                _ => "",
            };

            if worker_opt.is_some() {
                details_str = if is_mobile {
                    "Worker Harvester | Tap mineral node to mine".to_string()
                } else {
                    "Worker Harvester | Right-Click mineral to harvest | [S] Stop".to_string()
                };
            } else if soldier_opt.is_some() {
                details_str = if is_mobile {
                    format!("Ranged Fighter (15 DMG, 150 Rng){}", stance_suffix)
                } else {
                    format!("Ranged Fighter (15 DMG, 150 Rng){} | Right-Click Move/Attack | [S] Stop | [H] Hold", stance_suffix)
                };
            } else {
                details_str = if is_mobile {
                    "Combat Unit ready".to_string()
                } else {
                    "Combat Unit ready | Right-Click Move/Attack | [S] Stop | [H] Hold".to_string()
                };
            }
        } else {
            title_str = format!("Selected: {} Units", selected_units.len());
            details_str = if is_mobile {
                "Squad Command: Tap ground to Move, enemy to Attack".to_string()
            } else {
                "Squad Command: Right-Click Move/Attack | [S] Stop | [H] Hold Position".to_string()
            };
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

pub fn update_command_card_text(
    placement_state: Res<PlacementState>,
    control_scheme: Option<Res<crate::controls::ControlScheme>>,
    mut text_query: Query<&mut Text, With<BuildMenuText>>,
) {
    let is_mobile = control_scheme.map_or(false, |s| *s == crate::controls::ControlScheme::MobileTouch);
    for mut text in &mut text_query {
        if let Some(kind) = placement_state.active_kind {
            let place_hint = if is_mobile { "Tap Ground to Place" } else { "Left-Click to Place" };
            let cancel_hint = if is_mobile { "Tap [Cancel] below" } else { "[Esc/Right-Click] Cancel" };
            let status = if placement_state.is_valid { place_hint } else { "Blocked / Invalid Location" };
            text.0 = format!("Placing: {} ({}) - {} | {}", kind.name(), placement_state.mineral_cost, status, cancel_hint);
        } else if is_mobile {
            text.0 = "Barracks (150) | Turret (125) | Depot (100) | HQ (400)".to_string();
        } else {
            text.0 = "[B] Barracks (150) | [U] Turret (125, Req Barracks) | [P] Supply Depot (100) | [H] Base HQ (400)".to_string();
        }
    }
}

/// Dynamically updates HUD elements (top bar, minimap frame, command card, selection panel)
/// to adapt seamlessly between Desktop (spacious 1080p layout) and Mobile/compact screens (low-profile HUD).
pub fn update_responsive_hud_layout_system(
    control_scheme: Option<Res<crate::controls::ControlScheme>>,
    net_client: Res<NetClient>,
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut root_query: Query<&mut Node, (With<super::layout::RootUiContainer>, Without<super::top_bar::TopBarContainer>, Without<super::layout::MinimapFrame>, Without<super::command_card::CommandCardRoot>, Without<super::bottom_bar::SelectionCardPanel>, Without<super::top_bar::TopBarTitleText>, Without<super::top_bar::TopBarResourceGroup>, Without<super::top_bar::TopBarMenuButton>, Without<super::ApmText>)>,
    mut top_bar_query: Query<(&mut Node, &mut BackgroundColor), (With<super::top_bar::TopBarContainer>, Without<super::layout::RootUiContainer>, Without<super::layout::MinimapFrame>, Without<super::command_card::CommandCardRoot>, Without<super::bottom_bar::SelectionCardPanel>, Without<super::top_bar::TopBarTitleText>, Without<super::top_bar::TopBarResourceGroup>, Without<super::top_bar::TopBarMenuButton>, Without<super::ApmText>)>,
    mut title_query: Query<&mut Node, (With<super::top_bar::TopBarTitleText>, Without<super::layout::RootUiContainer>, Without<super::top_bar::TopBarContainer>, Without<super::layout::MinimapFrame>, Without<super::command_card::CommandCardRoot>, Without<super::bottom_bar::SelectionCardPanel>, Without<super::top_bar::TopBarResourceGroup>, Without<super::top_bar::TopBarMenuButton>, Without<super::ApmText>)>,
    mut menu_btn_query: Query<&mut Node, (With<super::top_bar::TopBarMenuButton>, Without<super::layout::RootUiContainer>, Without<super::top_bar::TopBarContainer>, Without<super::layout::MinimapFrame>, Without<super::command_card::CommandCardRoot>, Without<super::bottom_bar::SelectionCardPanel>, Without<super::top_bar::TopBarTitleText>, Without<super::top_bar::TopBarResourceGroup>, Without<super::ApmText>)>,
    mut res_group_query: Query<&mut Node, (With<super::top_bar::TopBarResourceGroup>, Without<super::layout::RootUiContainer>, Without<super::top_bar::TopBarContainer>, Without<super::layout::MinimapFrame>, Without<super::command_card::CommandCardRoot>, Without<super::bottom_bar::SelectionCardPanel>, Without<super::top_bar::TopBarTitleText>, Without<super::top_bar::TopBarMenuButton>, Without<super::ApmText>)>,
    mut apm_query: Query<&mut Node, (With<super::ApmText>, Without<super::layout::RootUiContainer>, Without<super::top_bar::TopBarContainer>, Without<super::layout::MinimapFrame>, Without<super::command_card::CommandCardRoot>, Without<super::bottom_bar::SelectionCardPanel>, Without<super::top_bar::TopBarTitleText>, Without<super::top_bar::TopBarResourceGroup>, Without<super::top_bar::TopBarMenuButton>)>,
    mut minimap_frame_query: Query<&mut Node, (With<super::layout::MinimapFrame>, Without<super::layout::RootUiContainer>, Without<super::top_bar::TopBarContainer>, Without<super::command_card::CommandCardRoot>, Without<super::bottom_bar::SelectionCardPanel>, Without<super::top_bar::TopBarTitleText>, Without<super::top_bar::TopBarResourceGroup>, Without<super::top_bar::TopBarMenuButton>, Without<super::ApmText>)>,
    mut command_card_query: Query<&mut Node, (With<super::command_card::CommandCardRoot>, Without<super::layout::RootUiContainer>, Without<super::top_bar::TopBarContainer>, Without<super::layout::MinimapFrame>, Without<super::bottom_bar::SelectionCardPanel>, Without<super::top_bar::TopBarTitleText>, Without<super::top_bar::TopBarResourceGroup>, Without<super::top_bar::TopBarMenuButton>, Without<super::ApmText>)>,
    mut selection_panel_query: Query<&mut Node, (With<super::bottom_bar::SelectionCardPanel>, Without<super::layout::RootUiContainer>, Without<super::top_bar::TopBarContainer>, Without<super::layout::MinimapFrame>, Without<super::command_card::CommandCardRoot>, Without<super::top_bar::TopBarTitleText>, Without<super::top_bar::TopBarResourceGroup>, Without<super::top_bar::TopBarMenuButton>, Without<super::ApmText>)>,
    mut min_font_query: Query<&mut TextFont, (With<super::MineralsText>, Without<super::SupplyText>, Without<super::NetworkStatusText>)>,
    mut sup_font_query: Query<&mut TextFont, (With<super::SupplyText>, Without<super::MineralsText>, Without<super::NetworkStatusText>)>,
    mut net_font_query: Query<&mut TextFont, (With<super::NetworkStatusText>, Without<super::MineralsText>, Without<super::SupplyText>)>,
) {
    let win_mobile = window_query.get_single().map_or(false, |w| w.width() < 960.0 || w.height() < 550.0);
    let is_mobile = control_scheme.map_or(false, |s| *s == crate::controls::ControlScheme::MobileTouch)
        || net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_mobile;

    if is_mobile {
        for mut node in &mut root_query {
            node.padding = UiRect::all(Val::Px(6.0));
        }
        for (mut node, mut bg) in &mut top_bar_query {
            node.height = Val::Px(30.0);
            node.padding = UiRect::axes(Val::Px(8.0), Val::Px(4.0));
            bg.0 = Color::srgba(0.04, 0.06, 0.10, 0.75);
        }
        for mut node in &mut title_query {
            node.display = Display::None;
        }
        for mut node in &mut menu_btn_query {
            node.padding = UiRect::axes(Val::Px(6.0), Val::Px(3.0));
        }
        for mut node in &mut res_group_query {
            node.column_gap = Val::Px(10.0);
        }
        for mut node in &mut apm_query {
            node.display = Display::None;
        }
        for mut font in &mut min_font_query {
            font.font_size = 12.0;
        }
        for mut font in &mut sup_font_query {
            font.font_size = 12.0;
        }
        for mut font in &mut net_font_query {
            font.font_size = 11.0;
        }
        for mut node in &mut minimap_frame_query {
            node.right = Val::Px(8.0);
            node.top = Val::Px(36.0);
            node.width = Val::Px(95.0);
            node.height = Val::Px(95.0);
        }
        for mut node in &mut command_card_query {
            node.min_width = Val::Px(220.0);
            node.max_width = Val::Px(280.0);
            node.min_height = Val::Auto;
            node.padding = UiRect::all(Val::Px(6.0));
            node.row_gap = Val::Px(4.0);
        }
        for mut node in &mut selection_panel_query {
            node.max_width = Val::Px(240.0);
            node.padding = UiRect::all(Val::Px(6.0));
            node.margin = UiRect::left(Val::Px(105.0));
        }
    } else {
        for mut node in &mut root_query {
            node.padding = UiRect::all(Val::Px(12.0));
        }
        for (mut node, mut bg) in &mut top_bar_query {
            node.height = Val::Px(50.0);
            node.padding = UiRect::axes(Val::Px(20.0), Val::Px(10.0));
            bg.0 = Color::srgba(0.06, 0.08, 0.12, 0.92);
        }
        for mut node in &mut title_query {
            node.display = Display::Flex;
        }
        for mut node in &mut menu_btn_query {
            node.padding = UiRect::axes(Val::Px(12.0), Val::Px(6.0));
        }
        for mut node in &mut res_group_query {
            node.column_gap = Val::Px(24.0);
        }
        for mut node in &mut apm_query {
            node.display = Display::Flex;
        }
        for mut font in &mut min_font_query {
            font.font_size = 16.0;
        }
        for mut font in &mut sup_font_query {
            font.font_size = 16.0;
        }
        for mut font in &mut net_font_query {
            font.font_size = 13.0;
        }
        for mut node in &mut minimap_frame_query {
            node.right = Val::Px(12.0);
            node.top = Val::Px(70.0);
            node.width = Val::Px(170.0);
            node.height = Val::Px(170.0);
        }
        for mut node in &mut command_card_query {
            node.min_width = Val::Px(330.0);
            node.max_width = Val::Px(390.0);
            node.min_height = Val::Px(170.0);
            node.padding = UiRect::all(Val::Px(10.0));
            node.row_gap = Val::Px(6.0);
            node.margin = UiRect::default();
        }
        for mut node in &mut selection_panel_query {
            node.max_width = Val::Px(460.0);
            node.padding = UiRect::all(Val::Px(14.0));
            node.margin = UiRect::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::ControlScheme;
    use crate::ui::bottom_bar::SelectionCardPanel;
    use crate::ui::command_card::{
        BarracksActionSection, BuildStructuresSection, CommandCardRoot, HqActionSection,
        PlacementCancelSection, UnitTacticsSection,
    };
    use crate::ui::layout::{MinimapFrame, RootUiContainer};
    use crate::ui::mobile_hud::MobileBuildMenuOpen;
    use crate::ui::top_bar::{TopBarContainer, TopBarMenuButton, TopBarResourceGroup, TopBarTitleText};
    use shared::protocol::ClientPlatform;

    #[test]
    fn test_responsive_hud_layout_desktop_vs_mobile() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NetClient>();
        app.insert_resource(ControlScheme::DesktopMouseKeyboard);

        // Spawn test window
        let mut window = Window::default();
        window.resolution.set(1920.0, 1080.0);
        app.world_mut().spawn(window);

        // Spawn HUD entities
        let root = app.world_mut().spawn((RootUiContainer, Node::default())).id();
        let top_bar = app.world_mut().spawn((TopBarContainer, Node::default(), BackgroundColor::default())).id();
        let title = app.world_mut().spawn((TopBarTitleText, Node::default())).id();
        let _menu_btn = app.world_mut().spawn((TopBarMenuButton, Node::default())).id();
        let _res_grp = app.world_mut().spawn((TopBarResourceGroup, Node::default())).id();
        let min_text = app.world_mut().spawn((MineralsText, TextFont::default())).id();
        let sup_text = app.world_mut().spawn((SupplyText, TextFont::default())).id();
        let net_text = app.world_mut().spawn((NetworkStatusText, TextFont::default())).id();
        let apm = app.world_mut().spawn((ApmText, Node::default())).id();
        let mm_frame = app.world_mut().spawn((MinimapFrame, Node::default())).id();
        let cmd_card = app.world_mut().spawn((CommandCardRoot, Node::default())).id();
        let sel_panel = app.world_mut().spawn((SelectionCardPanel, Node::default())).id();

        // 1. Run for Desktop
        app.add_systems(Update, update_responsive_hud_layout_system);
        app.update();

        let root_node = app.world().get::<Node>(root).unwrap();
        assert_eq!(root_node.padding, UiRect::all(Val::Px(12.0)));
        let tb_node = app.world().get::<Node>(top_bar).unwrap();
        assert_eq!(tb_node.height, Val::Px(50.0));
        let title_node = app.world().get::<Node>(title).unwrap();
        assert_eq!(title_node.display, Display::Flex);
        assert_eq!(app.world().get::<TextFont>(min_text).unwrap().font_size, 16.0);
        assert_eq!(app.world().get::<TextFont>(sup_text).unwrap().font_size, 16.0);
        assert_eq!(app.world().get::<TextFont>(net_text).unwrap().font_size, 13.0);
        let mm_node = app.world().get::<Node>(mm_frame).unwrap();
        assert_eq!(mm_node.width, Val::Px(170.0));
        assert_eq!(mm_node.height, Val::Px(170.0));
        let cmd_node = app.world().get::<Node>(cmd_card).unwrap();
        assert_eq!(cmd_node.min_width, Val::Px(330.0));
        assert_eq!(cmd_node.min_height, Val::Px(170.0));
        let sel_node = app.world().get::<Node>(sel_panel).unwrap();
        assert_eq!(sel_node.max_width, Val::Px(460.0));

        // 2. Switch to Mobile Touch on a 800x400 mobile screen
        *app.world_mut().resource_mut::<ControlScheme>() = ControlScheme::MobileTouch;
        let mut net_client = app.world_mut().resource_mut::<NetClient>();
        net_client.my_platform = ClientPlatform::Mobile;

        app.update();

        let root_node_mob = app.world().get::<Node>(root).unwrap();
        assert_eq!(root_node_mob.padding, UiRect::all(Val::Px(6.0)));
        let tb_node_mob = app.world().get::<Node>(top_bar).unwrap();
        assert_eq!(tb_node_mob.height, Val::Px(30.0));
        let title_node_mob = app.world().get::<Node>(title).unwrap();
        assert_eq!(title_node_mob.display, Display::None);
        let apm_node_mob = app.world().get::<Node>(apm).unwrap();
        assert_eq!(apm_node_mob.display, Display::None);
        assert_eq!(app.world().get::<TextFont>(min_text).unwrap().font_size, 12.0);
        assert_eq!(app.world().get::<TextFont>(sup_text).unwrap().font_size, 12.0);
        assert_eq!(app.world().get::<TextFont>(net_text).unwrap().font_size, 11.0);
        let mm_node_mob = app.world().get::<Node>(mm_frame).unwrap();
        assert_eq!(mm_node_mob.width, Val::Px(95.0));
        assert_eq!(mm_node_mob.height, Val::Px(95.0));
        let cmd_node_mob = app.world().get::<Node>(cmd_card).unwrap();
        assert_eq!(cmd_node_mob.min_width, Val::Px(220.0));
        assert_eq!(cmd_node_mob.min_height, Val::Auto);
        let sel_node_mob = app.world().get::<Node>(sel_panel).unwrap();
        assert_eq!(sel_node_mob.max_width, Val::Px(240.0));
    }

    #[test]
    fn test_mobile_command_card_hidden_until_needed() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<NetClient>();
        app.init_resource::<PlacementState>();
        app.insert_resource(ControlScheme::MobileTouch);
        app.insert_resource(MobileBuildMenuOpen(false));

        let mut window = Window::default();
        window.resolution.set(800.0, 400.0);
        app.world_mut().spawn(window);

        let card_root = app.world_mut().spawn((CommandCardRoot, Node::default())).id();
        let build_sec = app.world_mut().spawn((BuildStructuresSection, Node::default())).id();
        let _hq_sec = app.world_mut().spawn((HqActionSection, Node::default())).id();
        let _barracks_sec = app.world_mut().spawn((BarracksActionSection, Node::default())).id();
        let _tactics_sec = app.world_mut().spawn((UnitTacticsSection, Node::default())).id();
        let _cancel_sec = app.world_mut().spawn((PlacementCancelSection, Node::default())).id();

        app.add_systems(Update, crate::ui::command_card::update_command_card_visibility_system);

        // 1. With nothing selected and build menu closed -> Command Card is HIDDEN on mobile!
        app.update();
        assert_eq!(app.world().get::<Node>(card_root).unwrap().display, Display::None);

        // 2. Open mobile build menu -> Command Card opens with structure options
        app.world_mut().resource_mut::<MobileBuildMenuOpen>().0 = true;
        app.update();
        assert_eq!(app.world().get::<Node>(card_root).unwrap().display, Display::Flex);
        assert_eq!(app.world().get::<Node>(build_sec).unwrap().display, Display::Flex);

        // 3. Close mobile build menu -> Collapses back to hidden
        app.world_mut().resource_mut::<MobileBuildMenuOpen>().0 = false;
        app.update();
        assert_eq!(app.world().get::<Node>(card_root).unwrap().display, Display::None);
    }
}
