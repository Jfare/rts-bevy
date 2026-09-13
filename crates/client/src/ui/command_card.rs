use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use shared::components::{
    Barracks, BaseHQ, Building, Faction, MatchOutcome, MeleeFighter,
    MoveTarget, NetEntity, ProductionBuilding, QueuedUnit, Selectable, Soldier,
    SoldierState, TacticalStance, Worker,
};
use shared::economy::PlayerEconomy;
use shared::grid::BuildingKind;
use shared::protocol::{ClientMessage, UnitKind};

use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::placement::PlacementState;
use crate::stats::MatchStats;
use super::BuildMenuText;

/// Action triggers associated with interactive buttons on the Command Card
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandCardAction {
    // Production
    TrainWorker,
    TrainRangedFighter,
    TrainMeleeFighter,
    // Structure Placement
    BuildHQ,
    BuildBarracks,
    BuildSupplyDepot,
    BuildTurret,
    // Placement State
    CancelPlacement,
    // Tactical Orders
    Stop,
    HoldPosition,
    AttackMove,
}

/// Marker component for the Base HQ production action sub-panel
#[derive(Component)]
pub struct HqActionSection;

/// Marker component for the Barracks production action sub-panel
#[derive(Component)]
pub struct BarracksActionSection;

/// Marker component for the Unit tactical orders sub-panel (Stop, Hold, Attack-Move)
#[derive(Component)]
pub struct UnitTacticsSection;

/// Marker component for the Building construction sub-panel (Worker / Default)
#[derive(Component)]
pub struct BuildStructuresSection;

/// Marker component for the active placement cancellation sub-panel
#[derive(Component)]
pub struct PlacementCancelSection;

/// Marker component specifically for the Attack-Move button to track armed state visuals
#[derive(Component)]
pub struct AttackMoveButton;

/// Resource tracking whether Attack-Move command is currently armed
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct AttackMovePending(pub bool);

/// Marker component for the Command Card root container
#[derive(Component)]
pub struct CommandCardRoot;

/// Marker component for the Command Card header container
#[derive(Component)]
pub struct CommandCardHeader;

/// Spawns the entire Command Card UI hierarchy inside the bottom HUD right panel
pub fn spawn_command_card_ui(parent: &mut ChildBuilder) {
    parent
        .spawn((
            CommandCardRoot,
            Node {
                min_width: Val::Px(330.0),
                max_width: Val::Px(390.0),
                min_height: Val::Px(170.0),
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.5)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Stretch,
                ..default()
            },
            BorderRadius::all(Val::Px(4.0)),
            BackgroundColor(Color::srgba(0.06, 0.09, 0.14, 0.92)),
            BorderColor(Color::srgba(0.20, 0.38, 0.55, 0.90)),
            FocusPolicy::Pass,
        ))
        .with_children(|card| {
            // Header: Title & Context Status Subtitle
            card.spawn((
                CommandCardHeader,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    margin: UiRect::bottom(Val::Px(2.0)),
                    ..default()
                },
                FocusPolicy::Pass,
            ))
            .with_children(|header| {
                header.spawn((
                    Text::new("COMMAND CARD"),
                    TextFont {
                        font_size: 11.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.35, 0.85, 1.0)),
                    FocusPolicy::Pass,
                ));
                header.spawn((
                    Text::new("[B] Barracks (150) | [U] Turret (125) | [P] Depot (100) | [H] HQ (400)"),
                    TextFont {
                        font_size: 10.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.95, 0.85, 0.35)),
                    BuildMenuText,
                    FocusPolicy::Pass,
                ));
            });

            // Action Buttons Container
            card.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                FocusPolicy::Pass,
            ))
            .with_children(|body| {
                // 1. Placement Cancel Section (Active when placing a structure)
                body.spawn((
                    PlacementCancelSection,
                    Node {
                        display: Display::None,
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(6.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(4.0), Val::Px(8.0)),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ))
                .with_children(|sec| {
                    spawn_action_button(
                        sec,
                        CommandCardAction::CancelPlacement,
                        "Cancel Placement",
                        "[Esc]",
                        None,
                        true,
                    );
                });

                // 2. Base HQ Action Section (Train Worker)
                body.spawn((
                    HqActionSection,
                    Node {
                        display: Display::None,
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(4.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ))
                .with_children(|sec| {
                    spawn_action_button(
                        sec,
                        CommandCardAction::TrainWorker,
                        "Train Worker",
                        "[V]",
                        Some(50),
                        false,
                    );
                });

                // 3. Barracks Action Section (Train Ranged / Melee)
                body.spawn((
                    BarracksActionSection,
                    Node {
                        display: Display::None,
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(4.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ))
                .with_children(|sec| {
                    spawn_action_button(
                        sec,
                        CommandCardAction::TrainRangedFighter,
                        "Ranged Fighter",
                        "[R]",
                        Some(100),
                        false,
                    );
                    spawn_action_button(
                        sec,
                        CommandCardAction::TrainMeleeFighter,
                        "Melee Fighter",
                        "[F]",
                        Some(75),
                        false,
                    );
                });

                // 4. Unit Tactical Stances (Stop, Hold, Attack-Move)
                body.spawn((
                    UnitTacticsSection,
                    Node {
                        display: Display::None,
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(4.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ))
                .with_children(|sec| {
                    spawn_action_button(
                        sec,
                        CommandCardAction::Stop,
                        "Stop",
                        "[S]",
                        None,
                        false,
                    );
                    spawn_action_button(
                        sec,
                        CommandCardAction::HoldPosition,
                        "Hold",
                        "[H]",
                        None,
                        false,
                    );
                    spawn_action_button(
                        sec,
                        CommandCardAction::AttackMove,
                        "Attack",
                        "[A]",
                        None,
                        false,
                    );
                });

                // 5. Structure Construction Section (Worker selected or default build menu)
                body.spawn((
                    BuildStructuresSection,
                    Node {
                        display: Display::Flex,
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(4.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ))
                .with_children(|sec| {
                    spawn_action_button(
                        sec,
                        CommandCardAction::BuildBarracks,
                        "Barracks",
                        "[B]",
                        Some(150),
                        false,
                    );
                    spawn_action_button(
                        sec,
                        CommandCardAction::BuildTurret,
                        "Turret",
                        "[U]",
                        Some(125),
                        false,
                    );
                    spawn_action_button(
                        sec,
                        CommandCardAction::BuildSupplyDepot,
                        "Depot",
                        "[P]",
                        Some(100),
                        false,
                    );
                    spawn_action_button(
                        sec,
                        CommandCardAction::BuildHQ,
                        "Base HQ",
                        "[H]",
                        Some(400),
                        false,
                    );
                });
            });
        });
}

/// Helper function to spawn a styled interactive action button
fn spawn_action_button(
    parent: &mut ChildBuilder,
    action: CommandCardAction,
    title: &str,
    shortcut: &str,
    mineral_cost: Option<u32>,
    is_cancel: bool,
) {
    let bg_color = if is_cancel {
        Color::srgba(0.38, 0.12, 0.12, 0.95)
    } else {
        Color::srgba(0.12, 0.18, 0.28, 0.95)
    };
    let border_color = if is_cancel {
        Color::srgba(0.65, 0.22, 0.22, 0.85)
    } else {
        Color::srgba(0.25, 0.45, 0.65, 0.85)
    };

    let label_text = if let Some(cost) = mineral_cost {
        format!("{} ({}) {}", title, cost, shortcut)
    } else {
        format!("{} {}", title, shortcut)
    };

    let mut entity_cmds = parent.spawn((
        Button,
        action,
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
            border: UiRect::all(Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            margin: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderRadius::all(Val::Px(3.0)),
        BackgroundColor(bg_color),
        BorderColor(border_color),
        FocusPolicy::Block,
    ));

    if action == CommandCardAction::AttackMove {
        entity_cmds.insert(AttackMoveButton);
    }

    entity_cmds.with_children(|btn| {
        btn.spawn((
            Text::new(label_text),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(if is_cancel {
                Color::srgb(1.0, 0.85, 0.85)
            } else {
                Color::srgb(0.92, 0.96, 1.0)
            }),
            FocusPolicy::Pass,
        ));
    });
}

/// Updates the visibility of each Command Card action section depending on current selection and placement state
pub fn update_command_card_visibility_system(
    net_client: Res<NetClient>,
    placement_state: Res<PlacementState>,
    unit_query: Query<(
        &Faction,
        &Selectable,
        Option<&Worker>,
        Option<&Soldier>,
        Option<&MeleeFighter>,
    )>,
    building_query: Query<
        (
            &Faction,
            &Selectable,
            &Building,
            Option<&BaseHQ>,
            Option<&Barracks>,
        ),
    >,
    mut card_root_query: Query<
        &mut Node,
        (
            With<CommandCardRoot>,
            Without<HqActionSection>,
            Without<BarracksActionSection>,
            Without<UnitTacticsSection>,
            Without<BuildStructuresSection>,
            Without<PlacementCancelSection>,
        ),
    >,
    mut hq_section_query: Query<
        &mut Node,
        (
            With<HqActionSection>,
            Without<CommandCardRoot>,
            Without<BarracksActionSection>,
            Without<UnitTacticsSection>,
            Without<BuildStructuresSection>,
            Without<PlacementCancelSection>,
        ),
    >,
    mut barracks_section_query: Query<
        &mut Node,
        (
            With<BarracksActionSection>,
            Without<CommandCardRoot>,
            Without<HqActionSection>,
            Without<UnitTacticsSection>,
            Without<BuildStructuresSection>,
            Without<PlacementCancelSection>,
        ),
    >,
    mut tactics_section_query: Query<
        &mut Node,
        (
            With<UnitTacticsSection>,
            Without<CommandCardRoot>,
            Without<HqActionSection>,
            Without<BarracksActionSection>,
            Without<BuildStructuresSection>,
            Without<PlacementCancelSection>,
        ),
    >,
    mut build_section_query: Query<
        &mut Node,
        (
            With<BuildStructuresSection>,
            Without<CommandCardRoot>,
            Without<HqActionSection>,
            Without<BarracksActionSection>,
            Without<UnitTacticsSection>,
            Without<PlacementCancelSection>,
        ),
    >,
    mut cancel_section_query: Query<
        &mut Node,
        (
            With<PlacementCancelSection>,
            Without<CommandCardRoot>,
            Without<HqActionSection>,
            Without<BarracksActionSection>,
            Without<UnitTacticsSection>,
            Without<BuildStructuresSection>,
        ),
    >,
    control_scheme: Option<Res<crate::controls::ControlScheme>>,
    mobile_build_menu: Option<Res<crate::ui::mobile_hud::MobileBuildMenuOpen>>,
    window_query: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let win_mobile = window_query.get_single().map_or(false, |w| w.width() < 960.0 || w.height() < 550.0);
    let is_mobile = control_scheme.map_or(false, |s| *s == crate::controls::ControlScheme::MobileTouch)
        || net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_mobile;
    let build_menu_open = mobile_build_menu.map_or(false, |m| m.0);

    let my_faction = net_client.my_faction;
    let is_placing = placement_state.active_kind.is_some();

    // 1. Placement Cancel Section takes precedence when active
    for mut node in &mut cancel_section_query {
        node.display = if is_placing {
            Display::Flex
        } else {
            Display::None
        };
    }

    if is_placing {
        for mut node in &mut card_root_query {
            node.display = Display::Flex;
        }
        // Hide all other action sub-panels when in placement mode
        for mut node in &mut hq_section_query {
            node.display = Display::None;
        }
        for mut node in &mut barracks_section_query {
            node.display = Display::None;
        }
        for mut node in &mut tactics_section_query {
            node.display = Display::None;
        }
        for mut node in &mut build_section_query {
            node.display = Display::None;
        }
        return;
    }

    // Inspect player selections
    let mut has_selected_hq = false;
    let mut has_selected_barracks = false;
    for (faction, selectable, building, hq_opt, barracks_opt) in &building_query {
        if *faction == my_faction && selectable.is_selected && building.is_constructed {
            if hq_opt.is_some() {
                has_selected_hq = true;
            }
            if barracks_opt.is_some() {
                has_selected_barracks = true;
            }
        }
    }

    let mut has_selected_worker = false;
    let mut has_selected_combat = false;
    for (faction, selectable, worker_opt, soldier_opt, melee_opt) in &unit_query {
        if *faction == my_faction && selectable.is_selected {
            if worker_opt.is_some() {
                has_selected_worker = true;
            }
            if soldier_opt.is_some() || melee_opt.is_some() {
                has_selected_combat = true;
            }
        }
    }

    let has_any_selection = has_selected_hq
        || has_selected_barracks
        || has_selected_worker
        || has_selected_combat;

    // 2. Base HQ Section
    for mut node in &mut hq_section_query {
        node.display = if has_selected_hq {
            Display::Flex
        } else {
            Display::None
        };
    }

    // 3. Barracks Section
    for mut node in &mut barracks_section_query {
        node.display = if has_selected_barracks {
            Display::Flex
        } else {
            Display::None
        };
    }

    // 4. Unit Tactics Section (Stop, Hold, Attack)
    for mut node in &mut tactics_section_query {
        node.display = if has_selected_combat || has_selected_worker {
            Display::Flex
        } else {
            Display::None
        };
    }

    // 5. Structure Placement Section (Worker or Default/None selected on desktop, or mobile build menu open)
    let show_build = if is_mobile {
        has_selected_worker || (!has_any_selection && build_menu_open)
    } else {
        has_selected_worker || !has_any_selection
    };

    for mut node in &mut build_section_query {
        node.display = if show_build {
            Display::Flex
        } else {
            Display::None
        };
    }

    // Root Command Card visibility: On desktop always visible, on mobile only visible when selection or build menu active
    for mut node in &mut card_root_query {
        node.display = if is_mobile {
            if has_any_selection || build_menu_open {
                Display::Flex
            } else {
                Display::None
            }
        } else {
            Display::Flex
        };
        node.margin = if is_mobile && (has_selected_combat || has_selected_worker) {
            UiRect::right(Val::Px(56.0))
        } else {
            UiRect::ZERO
        };
    }
}

/// Handles click and hover interaction states for all Command Card buttons
pub fn handle_command_card_interactions_system(
    mut commands: Commands,
    mut interaction_query: Query<
        (
            &Interaction,
            &mut BackgroundColor,
            &mut BorderColor,
            &CommandCardAction,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    net_client: Res<NetClient>,
    outcome_opt: Option<Res<MatchOutcome>>,
    mut economy: ResMut<PlayerEconomy>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
    mut placement_state: ResMut<PlacementState>,
    mut attack_move_pending: ResMut<AttackMovePending>,
    mut mobile_build_menu: Option<ResMut<crate::ui::mobile_hud::MobileBuildMenuOpen>>,
    mut prod_query: Query<(
        &mut ProductionBuilding,
        &Building,
        &Faction,
        &Selectable,
        Option<&NetEntity>,
        Option<&BaseHQ>,
        Option<&Barracks>,
    )>,
    mut unit_query: Query<(
        Entity,
        &Faction,
        &Selectable,
        Option<&NetEntity>,
        Option<&mut MoveTarget>,
        Option<&mut TacticalStance>,
        Option<&mut Soldier>,
        Option<&mut MeleeFighter>,
    )>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory)
        || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat)
    {
        return;
    }

    let my_faction = net_client.my_faction;

    for (interaction, mut bg_color, mut border_color, action) in &mut interaction_query {
        let is_cancel = *action == CommandCardAction::CancelPlacement;
        match *interaction {
            Interaction::Pressed => {
                stats.record_action();
                bg_color.0 = if is_cancel {
                    Color::srgb(0.70, 0.15, 0.15)
                } else {
                    Color::srgb(0.20, 0.45, 0.70)
                };
                border_color.0 = Color::srgb(1.0, 1.0, 1.0);

                if let Some(ref mut m) = mobile_build_menu {
                    if is_cancel || matches!(action, CommandCardAction::BuildHQ | CommandCardAction::BuildBarracks | CommandCardAction::BuildSupplyDepot | CommandCardAction::BuildTurret) {
                        m.0 = false;
                    }
                }

                match action {
                    CommandCardAction::TrainWorker => {
                        for (mut prod, building, faction, selectable, net_entity_opt, base_hq, _) in
                            &mut prod_query
                        {
                            if *faction == my_faction
                                && selectable.is_selected
                                && building.is_constructed
                                && base_hq.is_some()
                                && prod.queue.len() < prod.max_queue_size
                            {
                                if !economy.has_minerals(*faction, 50) {
                                    info!("⚠️ [Economy] Not enough Gold for Worker (Requires 50 🪙)!");
                                    continue;
                                }

                                if !economy.has_supply(*faction, 1) {
                                    sound_events.send(SoundEffect::SupplyBlocked);
                                    info!("⚠️ [Economy] Not enough supply for Worker (Requires 1 ⚡) - Build a Supply Depot [P]!");
                                    continue;
                                }

                                economy.spend_minerals(*faction, 50);
                                economy.register_supply(*faction, 1);
                                if *faction == Faction::Player1 {
                                    stats.minerals_spent += 50;
                                    stats.units_trained += 1;
                                }
                                sound_events.send(SoundEffect::UnitTrained);

                                prod.queue.push(QueuedUnit {
                                    name: "Worker".to_string(),
                                    mineral_cost: 50,
                                    supply_cost: 1,
                                    build_duration: 3.0,
                                });

                                if let Some(net) = net_entity_opt {
                                    if net_client.status != NetStatus::Disconnected {
                                        net_client.send(&ClientMessage::RequestTrainUnit {
                                            building_net_id: net.net_id,
                                            unit_kind: UnitKind::Worker,
                                        });
                                    }
                                }

                                info!("⛏️ [CommandCard] Worker queued! Queue size: {}", prod.queue.len());
                            }
                        }
                    }
                    CommandCardAction::TrainRangedFighter => {
                        for (mut prod, building, faction, selectable, net_entity_opt, _, barracks) in
                            &mut prod_query
                        {
                            if *faction == my_faction
                                && selectable.is_selected
                                && building.is_constructed
                                && barracks.is_some()
                            {
                                if prod.queue.len() >= prod.max_queue_size {
                                    info!("⚠️ [CommandCard] Production queue is full!");
                                    continue;
                                }

                                if !economy.has_minerals(*faction, 100) {
                                    info!("⚠️ [Economy] Not enough Gold for Ranged Fighter (Requires 100 🪙)!");
                                    continue;
                                }

                                if !economy.has_supply(*faction, 2) {
                                    sound_events.send(SoundEffect::SupplyBlocked);
                                    info!("⚠️ [Economy] Not enough supply for Ranged Fighter (Requires 2 ⚡) - Build a Supply Depot [P]!");
                                    continue;
                                }

                                economy.spend_minerals(*faction, 100);
                                economy.register_supply(*faction, 2);
                                if *faction == Faction::Player1 {
                                    stats.minerals_spent += 100;
                                    stats.units_trained += 1;
                                }
                                sound_events.send(SoundEffect::UnitTrained);

                                prod.queue.push(QueuedUnit {
                                    name: "Ranged Fighter".to_string(),
                                    mineral_cost: 100,
                                    supply_cost: 2,
                                    build_duration: 4.0,
                                });

                                if let Some(net) = net_entity_opt {
                                    if net_client.status != NetStatus::Disconnected {
                                        net_client.send(&ClientMessage::RequestTrainUnit {
                                            building_net_id: net.net_id,
                                            unit_kind: UnitKind::RangedFighter,
                                        });
                                    }
                                }

                                info!("🏹 [CommandCard] Ranged Fighter queued! Queue size: {}", prod.queue.len());
                            }
                        }
                    }
                    CommandCardAction::TrainMeleeFighter => {
                        for (mut prod, building, faction, selectable, net_entity_opt, _, barracks) in
                            &mut prod_query
                        {
                            if *faction == my_faction
                                && selectable.is_selected
                                && building.is_constructed
                                && barracks.is_some()
                            {
                                if prod.queue.len() >= prod.max_queue_size {
                                    info!("⚠️ [CommandCard] Production queue is full!");
                                    continue;
                                }

                                if !economy.has_minerals(*faction, 75) {
                                    info!("⚠️ [Economy] Not enough Gold for Melee Fighter (Requires 75 🪙)!");
                                    continue;
                                }

                                if !economy.has_supply(*faction, 1) {
                                    sound_events.send(SoundEffect::SupplyBlocked);
                                    info!("⚠️ [Economy] Not enough supply for Melee Fighter (Requires 1 ⚡) - Build a Supply Depot [P]!");
                                    continue;
                                }

                                economy.spend_minerals(*faction, 75);
                                economy.register_supply(*faction, 1);
                                if *faction == Faction::Player1 {
                                    stats.minerals_spent += 75;
                                    stats.units_trained += 1;
                                }
                                sound_events.send(SoundEffect::UnitTrained);

                                prod.queue.push(QueuedUnit {
                                    name: "Melee Fighter".to_string(),
                                    mineral_cost: 75,
                                    supply_cost: 1,
                                    build_duration: 3.5,
                                });

                                if let Some(net) = net_entity_opt {
                                    if net_client.status != NetStatus::Disconnected {
                                        net_client.send(&ClientMessage::RequestTrainUnit {
                                            building_net_id: net.net_id,
                                            unit_kind: UnitKind::MeleeFighter,
                                        });
                                    }
                                }

                                info!("⚔️ [CommandCard] Melee Fighter queued! Queue size: {}", prod.queue.len());
                            }
                        }
                    }
                    CommandCardAction::BuildHQ => {
                        placement_state.active_kind = Some(BuildingKind::BaseHQ);
                        placement_state.mineral_cost = BuildingKind::BaseHQ.mineral_cost();
                        info!("🏗️ [CommandCard] Base HQ ($400) selected for placement");
                    }
                    CommandCardAction::BuildBarracks => {
                        placement_state.active_kind = Some(BuildingKind::Barracks);
                        placement_state.mineral_cost = BuildingKind::Barracks.mineral_cost();
                        info!("🏗️ [CommandCard] Barracks ($150) selected for placement");
                    }
                    CommandCardAction::BuildSupplyDepot => {
                        placement_state.active_kind = Some(BuildingKind::SupplyDepot);
                        placement_state.mineral_cost = BuildingKind::SupplyDepot.mineral_cost();
                        info!("🏗️ [CommandCard] Supply Depot ($100) selected for placement");
                    }
                    CommandCardAction::BuildTurret => {
                        placement_state.active_kind = Some(BuildingKind::Turret);
                        placement_state.mineral_cost = BuildingKind::Turret.mineral_cost();
                        info!("🏗️ [CommandCard] Gun Turret ($125) selected for placement");
                    }
                    CommandCardAction::CancelPlacement => {
                        placement_state.active_kind = None;
                        info!("❌ [CommandCard] Placement cancelled");
                    }
                    CommandCardAction::Stop => {
                        attack_move_pending.0 = false;
                        let mut net_ids = Vec::new();
                        for (
                            entity,
                            faction,
                            selectable,
                            net_opt,
                            _,
                            mut stance_opt,
                            mut soldier_opt,
                            mut melee_opt,
                        ) in &mut unit_query
                        {
                            if *faction == my_faction && selectable.is_selected {
                                commands.entity(entity).remove::<MoveTarget>();
                                if let Some(ref mut soldier) = soldier_opt {
                                    soldier.state = SoldierState::Idle;
                                    soldier.target = None;
                                }
                                if let Some(ref mut melee) = melee_opt {
                                    melee.state = SoldierState::Idle;
                                    melee.target = None;
                                }
                                if let Some(ref mut stance) = stance_opt {
                                    **stance = TacticalStance::Aggressive;
                                }
                                if let Some(net) = net_opt {
                                    net_ids.push(net.net_id);
                                }
                            }
                        }
                        if !net_ids.is_empty() && net_client.status != NetStatus::Disconnected {
                            net_client.send(&ClientMessage::RequestStop {
                                unit_net_ids: net_ids,
                            });
                        }
                        sound_events.send(SoundEffect::OrderIssued);
                        info!("🛑 [CommandCard] Stop command issued to selected units");
                    }
                    CommandCardAction::HoldPosition => {
                        attack_move_pending.0 = false;
                        let mut net_ids = Vec::new();
                        for (
                            entity,
                            faction,
                            selectable,
                            net_opt,
                            _,
                            mut stance_opt,
                            mut soldier_opt,
                            mut melee_opt,
                        ) in &mut unit_query
                        {
                            if *faction == my_faction && selectable.is_selected {
                                commands.entity(entity).remove::<MoveTarget>();
                                if let Some(ref mut soldier) = soldier_opt {
                                    soldier.state = SoldierState::HoldingPosition;
                                    soldier.target = None;
                                }
                                if let Some(ref mut melee) = melee_opt {
                                    melee.state = SoldierState::HoldingPosition;
                                    melee.target = None;
                                }
                                if let Some(ref mut stance) = stance_opt {
                                    **stance = TacticalStance::HoldPosition;
                                } else {
                                    commands.entity(entity).insert(TacticalStance::HoldPosition);
                                }
                                if let Some(net) = net_opt {
                                    net_ids.push(net.net_id);
                                }
                            }
                        }
                        if !net_ids.is_empty() && net_client.status != NetStatus::Disconnected {
                            net_client.send(&ClientMessage::RequestHoldPosition {
                                unit_net_ids: net_ids,
                            });
                        }
                        sound_events.send(SoundEffect::OrderIssued);
                        info!("🛡️ [CommandCard] Hold Position command issued to selected units");
                    }
                    CommandCardAction::AttackMove => {
                        attack_move_pending.0 = !attack_move_pending.0;
                        // If units already have an active MoveTarget, update it immediately to attack-move
                        for (
                            _,
                            faction,
                            selectable,
                            _,
                            mut move_target_opt,
                            _,
                            mut soldier_opt,
                            mut melee_opt,
                        ) in &mut unit_query
                        {
                            if *faction == my_faction && selectable.is_selected {
                                if let Some(ref mut mt) = move_target_opt {
                                    mt.is_attack_move = true;
                                }
                                if let Some(ref mut soldier) = soldier_opt {
                                    if soldier.state == SoldierState::MovingToGround {
                                        soldier.state = SoldierState::AttackMoving;
                                    }
                                }
                                if let Some(ref mut melee) = melee_opt {
                                    if melee.state == SoldierState::MovingToGround {
                                        melee.state = SoldierState::AttackMoving;
                                    }
                                }
                            }
                        }
                        sound_events.send(SoundEffect::OrderIssued);
                        info!(
                            "⚔️ [CommandCard] Attack-Move order armed: {} (Right-click ground to attack-move)",
                            attack_move_pending.0
                        );
                    }
                }
            }
            Interaction::Hovered => {
                bg_color.0 = if is_cancel {
                    Color::srgba(0.55, 0.18, 0.18, 0.95)
                } else {
                    Color::srgba(0.18, 0.35, 0.55, 0.95)
                };
                border_color.0 = if is_cancel {
                    Color::srgba(0.85, 0.35, 0.35, 1.0)
                } else {
                    Color::srgba(0.40, 0.70, 1.0, 1.0)
                };
            }
            Interaction::None => {
                if *action == CommandCardAction::AttackMove && attack_move_pending.0 {
                    bg_color.0 = Color::srgba(0.65, 0.35, 0.10, 0.95);
                    border_color.0 = Color::srgba(1.0, 0.65, 0.20, 0.95);
                } else {
                    bg_color.0 = if is_cancel {
                        Color::srgba(0.38, 0.12, 0.12, 0.95)
                    } else {
                        Color::srgba(0.12, 0.18, 0.28, 0.95)
                    };
                    border_color.0 = if is_cancel {
                        Color::srgba(0.65, 0.22, 0.22, 0.85)
                    } else {
                        Color::srgba(0.25, 0.45, 0.65, 0.85)
                    };
                }
            }
        }
    }
}

/// Updates the AttackMoveButton visual appearance when AttackMovePending state changes
pub fn update_attack_move_visuals_system(
    attack_move_pending: Res<AttackMovePending>,
    mut button_query: Query<
        (&mut BackgroundColor, &mut BorderColor, &Interaction),
        With<AttackMoveButton>,
    >,
) {
    if !attack_move_pending.is_changed() {
        return;
    }

    for (mut bg_color, mut border_color, interaction) in &mut button_query {
        if *interaction == Interaction::None {
            if attack_move_pending.0 {
                bg_color.0 = Color::srgba(0.65, 0.35, 0.10, 0.95);
                border_color.0 = Color::srgba(1.0, 0.65, 0.20, 0.95);
            } else {
                bg_color.0 = Color::srgba(0.12, 0.18, 0.28, 0.95);
                border_color.0 = Color::srgba(0.25, 0.45, 0.65, 0.85);
            }
        }
    }
}
