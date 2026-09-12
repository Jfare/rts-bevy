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
        text.0 = format!("🪙 Gold: {}", my_eco.minerals);
    }
    for mut text in &mut sup_query {
        text.0 = format!("⚡ Supply: {} / {}", my_eco.current_supply, my_eco.max_supply);
    }
    for mut text in &mut apm_query {
        text.0 = format!("⚡ APM: {}", stats.current_apm());
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
                text.0 = format!("{} 🟢 LIVE ({}ms){}", net_client.my_platform.icon(), net_client.rtt_ms, opp_badge);
                color.0 = Color::srgb(0.25, 0.95, 0.45);
            }
            NetStatus::InLobby => {
                text.0 = format!("{} 🟡 SEARCHING (1/2)", net_client.my_platform.icon());
                color.0 = Color::srgb(0.95, 0.85, 0.25);
            }
            NetStatus::Connected => {
                text.0 = format!("{} 🟢 CONNECTED", net_client.my_platform.icon());
                color.0 = Color::srgb(0.25, 0.95, 0.45);
            }
            NetStatus::Connecting => {
                text.0 = "🟡 CONNECTING...".to_string();
                color.0 = Color::srgb(0.95, 0.85, 0.25);
            }
            NetStatus::Disconnected => {
                text.0 = format!("{} ⚪ OFFLINE (SOLO)", net_client.my_platform.icon());
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
    control_scheme: Option<Res<crate::controls::ControlScheme>>,
) {
    let is_mobile = control_scheme.map_or(false, |s| *s == crate::controls::ControlScheme::MobileTouch);
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

    let mut title_str = "No Units Selected".to_string();
    let mut details_str = if is_mobile {
        "Tap unit to select | Tap ground to Move, enemy to Attack | Tap [⚔️ ALL ARMY] to select all".to_string()
    } else {
        "Drag left-click to select | Right-click Move / Attack | [S] Stop | [H] Hold".to_string()
    };
    let mut queue_str = String::new();

    if let Some((building, faction, health, prod_opt, turret_opt)) = selected_building {
        let fac_str = if *faction == Faction::Player1 { "Player 1" } else if *faction == Faction::Player2 { "Player 2" } else { "Hostile" };
        title_str = format!("🏢 {} ({}) - HP: {:.0}/{:.0}", building.name, fac_str, health.current, health.max);

        if !building.is_constructed {
            details_str = format!("⚠️ Under Construction... ({:.0}%)", building.progress() * 100.0);
        } else if turret_opt.is_some() {
            details_str = "Automated Twin-Cannon Defense | 360° Attack Arc (18 DMG, 220 Range)".to_string();
        } else if let Some(prod) = prod_opt {
            let train_prompt = if building.name.contains("Base HQ") {
                if is_mobile { "Train Worker (50 Gold, 1 Supply)" } else { "Press [V]/[W] to Train Worker (50 Gold, 1 Supply)" }
            } else if building.name.contains("Barracks") {
                if is_mobile { "Train Ranged (100 Gold) / Melee Fighter (75 Gold)" } else { "Press [R] Ranged Fighter (100 Gold) | [F] Melee Fighter (75 Gold)" }
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
                queue_str = format!("⚙️ Queue: {} ({:.0}%) | Queued: {}", queued_names.join(", "), progress, prod.queue.len());
            }
        }
    } else if let Some(resource) = selected_resource {
        title_str = "🪙 Gold Rock Deposit".to_string();
        details_str = format!("Remaining Gold: {} / {}", resource.remaining_minerals, resource.max_minerals);
    } else if !selected_units.is_empty() {
        if selected_units.len() == 1 {
            let (unit, _, health, worker_opt, soldier_opt, _, stance_opt) = selected_units[0];
            title_str = format!("👤 {} - HP: {:.0}/{:.0}", unit.name, health.current, health.max);

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
                    format!("Ranged Fighter (15 DMG, 150 Rng){} | Tap ground to Move, enemy to Attack", stance_suffix)
                } else {
                    format!("Ranged Fighter (15 DMG, 150 Rng){} | Right-Click Move/Attack | [S] Stop | [H] Hold", stance_suffix)
                };
            } else {
                details_str = if is_mobile {
                    "Combat Unit ready | Tap ground to Move, enemy to Attack".to_string()
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
            text.0 = format!("🏗️ Placing: {} ({}🪙) - {} | {}", kind.name(), placement_state.mineral_cost, status, cancel_hint);
        } else if is_mobile {
            text.0 = "Barracks (150🪙) | Turret (125🪙) | Depot (100🪙) | HQ (400🪙)".to_string();
        } else {
            text.0 = "[B] Barracks (150🪙) | [U] Turret (125🪙, Req Barracks) | [P] Supply Depot (100🪙) | [H] Base HQ (400🪙)".to_string();
        }
    }
}
