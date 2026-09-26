use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use shared::components::{BaseHQ, Building, Faction, MatchOutcome, NetEntity, ProductionBuilding, QueuedUnit};
use shared::economy::PlayerEconomy;
use shared::protocol::{ClientMessage, UnitKind};

use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::stats::MatchStats;

/// Marker component for the desktop quick-train container pinned below the minimap
#[derive(Component)]
pub struct DesktopQuickTrainContainer;

/// Interactive action button for quick-training workers without selecting the Base HQ
#[derive(Component)]
pub struct QuickTrainWorkerButton;

/// Spawns the desktop quick-train worker button pinned on the right edge below the minimap
pub fn spawn_desktop_quick_train_button(parent: &mut ChildBuilder) {
    parent
        .spawn((
            DesktopQuickTrainContainer,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(248.0),
                width: Val::Px(170.0),
                height: Val::Px(44.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::Flex, // Visible on desktop, hidden on mobile by responsive system
                ..default()
            },
            FocusPolicy::Pass,
        ))
        .with_children(|container| {
            container
                .spawn((
                    Button,
                    QuickTrainWorkerButton,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.5)),
                        ..default()
                    },
                    BorderRadius::all(Val::Px(6.0)),
                    BackgroundColor(Color::srgba(0.08, 0.12, 0.18, 0.94)),
                    BorderColor(Color::srgba(0.90, 0.75, 0.25, 0.85)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("⛏️ Worker [V]"),
                        TextFont {
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.95, 0.95, 0.95)),
                        FocusPolicy::Pass,
                    ));
                    btn.spawn((
                        Text::new("50 🪙"),
                        TextFont {
                            font_size: 11.5,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.85, 0.30)),
                        FocusPolicy::Pass,
                    ));
                });
        });
}

/// Spawns the compact 44x44 mobile quick-train worker button docked in thumb reach
pub fn spawn_quick_btn_train_worker(parent: &mut ChildBuilder) {
    parent
        .spawn((
            Button,
            QuickTrainWorkerButton,
            Node {
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                padding: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(10.0)),
            BackgroundColor(Color::srgba(0.22, 0.18, 0.08, 0.94)),
            BorderColor(Color::srgba(0.95, 0.80, 0.25, 0.85)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("⛏️"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                FocusPolicy::Pass,
            ));
            btn.spawn((
                Text::new("50🪙"),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.88, 0.35)),
                FocusPolicy::Pass,
            ));
        });
}

/// Core helper to train a worker at the least busy friendly constructed Base HQ.
/// Used by both UI buttons (desktop/mobile) and the global `V` hotkey.
pub fn execute_train_worker(
    economy: &mut PlayerEconomy,
    net_client: &NetClient,
    stats: &mut MatchStats,
    sound_events: &mut EventWriter<SoundEffect>,
    prod_query: &mut Query<(
        Entity,
        &mut ProductionBuilding,
        &Building,
        &Faction,
        Option<&NetEntity>,
        Option<&BaseHQ>,
    )>,
) -> bool {
    let my_faction = net_client.my_faction;

    // 1. Find friendly constructed Base HQs with room in their queue
    let mut best_hq: Option<(Entity, usize)> = None;

    for (entity, prod, building, faction, _, base_hq) in prod_query.iter() {
        if *faction == my_faction && base_hq.is_some() && building.is_constructed {
            if prod.queue.len() < prod.max_queue_size {
                match best_hq {
                    None => best_hq = Some((entity, prod.queue.len())),
                    Some((_, min_len)) => {
                        if prod.queue.len() < min_len {
                            best_hq = Some((entity, prod.queue.len()));
                        }
                    }
                }
            }
        }
    }

    let Some((target_entity, _)) = best_hq else {
        let has_any_hq = prod_query
            .iter()
            .any(|(_, _, b, f, _, hq)| *f == my_faction && hq.is_some() && b.is_constructed);
        if has_any_hq {
            info!("⚠️ [Production] Base HQ production queue is full!");
        } else {
            info!("⚠️ [Production] No constructed Base HQ available!");
        }
        return false;
    };

    // 2. Validate economy (50 Gold)
    if !economy.has_minerals(my_faction, 50) {
        info!("⚠️ [Economy] Not enough Gold for Worker (Requires 50 🪙)!");
        return false;
    }

    // 3. Validate supply (1 Supply)
    if !economy.has_supply(my_faction, 1) {
        sound_events.send(SoundEffect::SupplyBlocked);
        info!("⚠️ [Economy] Not enough supply for Worker (Requires 1 ⚡) - Build a Supply Depot!");
        return false;
    }

    // 4. Spend resources
    economy.spend_minerals(my_faction, 50);
    economy.register_supply(my_faction, 1);
    if my_faction == Faction::Player1 {
        stats.minerals_spent += 50;
        stats.units_trained += 1;
    }
    stats.record_action();
    sound_events.send(SoundEffect::UnitTrained);

    // 5. Push to selected Base HQ queue and dispatch network message
    for (entity, mut prod, _, _, net_entity_opt, _) in prod_query.iter_mut() {
        if entity == target_entity {
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
            info!(
                "⛏️ [Production] Worker queued in Base HQ! (Queue size: {})",
                prod.queue.len()
            );
            return true;
        }
    }

    false
}

/// Handles click / tap interactions on QuickTrainWorkerButton
pub fn handle_quick_train_button_interactions(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (Changed<Interaction>, With<QuickTrainWorkerButton>),
    >,
    outcome_opt: Option<Res<MatchOutcome>>,
    net_client: Res<NetClient>,
    mut economy: ResMut<PlayerEconomy>,
    mut stats: ResMut<MatchStats>,
    mut sound_events: EventWriter<SoundEffect>,
    mut prod_query: Query<(
        Entity,
        &mut ProductionBuilding,
        &Building,
        &Faction,
        Option<&NetEntity>,
        Option<&BaseHQ>,
    )>,
) {
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory)
        || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat)
    {
        return;
    }

    for (interaction, mut bg, mut border) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                bg.0 = Color::srgba(0.35, 0.28, 0.12, 0.98);
                border.0 = Color::srgb(1.0, 1.0, 1.0);
                execute_train_worker(
                    &mut economy,
                    &net_client,
                    &mut stats,
                    &mut sound_events,
                    &mut prod_query,
                );
            }
            Interaction::Hovered => {
                bg.0 = Color::srgba(0.28, 0.22, 0.10, 0.95);
                border.0 = Color::srgb(1.0, 0.90, 0.40);
            }
            Interaction::None => {
                bg.0 = Color::srgba(0.08, 0.12, 0.18, 0.94);
                border.0 = Color::srgba(0.90, 0.75, 0.25, 0.85);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn test_quick_train_button_trains_worker() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let mut net_client = NetClient::default();
        net_client.my_faction = Faction::Player1;
        app.insert_resource(net_client);

        let mut economy = PlayerEconomy::default();
        economy.set_minerals(Faction::Player1, 100);
        economy.set_supply(Faction::Player1, 0, 10);
        app.insert_resource(economy);
        app.init_resource::<MatchStats>();
        app.add_event::<SoundEffect>();

        // Spawn Base HQ (unselected!)
        let hq_entity = app
            .world_mut()
            .spawn((
                Faction::Player1,
                Building::new("Base HQ", Vec2::new(110.0, 110.0), 5.0, true),
                BaseHQ::default(),
                ProductionBuilding::default(),
            ))
            .id();

        // Spawn QuickTrainWorkerButton with Interaction::Pressed
        app.world_mut().spawn((
            Button,
            QuickTrainWorkerButton,
            Interaction::Pressed,
            BackgroundColor::default(),
            BorderColor::default(),
        ));

        app.world_mut()
            .run_system_once(handle_quick_train_button_interactions)
            .unwrap();

        let eco = app.world().resource::<PlayerEconomy>();
        assert_eq!(eco.get_minerals(Faction::Player1), 50, "50 gold spent");
        assert_eq!(eco.get(Faction::Player1).current_supply, 1, "1 supply used");

        let prod = app.world().get::<ProductionBuilding>(hq_entity).unwrap();
        assert_eq!(
            prod.queue.len(),
            1,
            "Worker should be queued in Base HQ even if unselected"
        );
        assert_eq!(prod.queue[0].name, "Worker");
    }

    #[test]
    fn test_quick_train_button_insufficient_gold() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let mut net_client = NetClient::default();
        net_client.my_faction = Faction::Player1;
        app.insert_resource(net_client);

        let mut economy = PlayerEconomy::default();
        economy.set_minerals(Faction::Player1, 30);
        economy.set_supply(Faction::Player1, 0, 10);
        app.insert_resource(economy);
        app.init_resource::<MatchStats>();
        app.add_event::<SoundEffect>();

        let hq_entity = app
            .world_mut()
            .spawn((
                Faction::Player1,
                Building::new("Base HQ", Vec2::new(110.0, 110.0), 5.0, true),
                BaseHQ::default(),
                ProductionBuilding::default(),
            ))
            .id();

        app.world_mut().spawn((
            Button,
            QuickTrainWorkerButton,
            Interaction::Pressed,
            BackgroundColor::default(),
            BorderColor::default(),
        ));

        app.world_mut()
            .run_system_once(handle_quick_train_button_interactions)
            .unwrap();

        let eco = app.world().resource::<PlayerEconomy>();
        assert_eq!(eco.get_minerals(Faction::Player1), 30);
        let prod = app.world().get::<ProductionBuilding>(hq_entity).unwrap();
        assert_eq!(prod.queue.len(), 0);
    }

    #[test]
    fn test_quick_train_button_supply_blocked() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let mut net_client = NetClient::default();
        net_client.my_faction = Faction::Player1;
        app.insert_resource(net_client);

        let mut economy = PlayerEconomy::default();
        economy.set_minerals(Faction::Player1, 100);
        economy.set_supply(Faction::Player1, 10, 10);
        app.insert_resource(economy);
        app.init_resource::<MatchStats>();
        app.add_event::<SoundEffect>();

        let hq_entity = app
            .world_mut()
            .spawn((
                Faction::Player1,
                Building::new("Base HQ", Vec2::new(110.0, 110.0), 5.0, true),
                BaseHQ::default(),
                ProductionBuilding::default(),
            ))
            .id();

        app.world_mut().spawn((
            Button,
            QuickTrainWorkerButton,
            Interaction::Pressed,
            BackgroundColor::default(),
            BorderColor::default(),
        ));

        app.world_mut()
            .run_system_once(handle_quick_train_button_interactions)
            .unwrap();

        let eco = app.world().resource::<PlayerEconomy>();
        assert_eq!(eco.get_minerals(Faction::Player1), 100);
        let prod = app.world().get::<ProductionBuilding>(hq_entity).unwrap();
        assert_eq!(prod.queue.len(), 0);
    }
}
