#![allow(dead_code)]

pub mod desktop;
pub mod mobile;

use bevy::prelude::*;
use shared::components::{
    Faction, MeleeFighter, MoveTarget, NetEntity, Selectable,
    Soldier, SoldierState, TacticalStance, Worker, WorkerState,
};
use shared::grid::NavGrid;
use shared::protocol::{ClientMessage, ClientPlatform};

use crate::audio_sfx::SoundEffect;
use crate::command_marker::CommandMarker;
use crate::net::{NetClient, NetStatus};
use crate::stats::MatchStats;

/// Active control scheme driving input interpretation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource, Reflect)]
pub enum ControlScheme {
    DesktopMouseKeyboard,
    MobileTouch,
}

impl Default for ControlScheme {
    fn default() -> Self {
        Self::DesktopMouseKeyboard
    }
}

/// Resource tracking whether touch dragging should draw a marquee selection box instead of panning
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct BoxSelectMode(pub bool);

/// Resource tracking touch double-tap timing for selecting all units of same type
#[derive(Resource, Default, Debug)]
pub struct DoubleTapTracker {
    pub last_entity: Option<Entity>,
    pub last_tap_time: f32,
    pub last_tap_pos: Vec2,
}

/// Run condition checking if desktop controls are active
pub fn is_desktop_control_scheme(scheme: Res<ControlScheme>) -> bool {
    *scheme == ControlScheme::DesktopMouseKeyboard
}

/// Run condition checking if mobile touch controls are active
pub fn is_mobile_control_scheme(scheme: Res<ControlScheme>) -> bool {
    *scheme == ControlScheme::MobileTouch
}

/// Master plugin for controls management
pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ControlScheme>()
            .init_resource::<BoxSelectMode>()
            .init_resource::<DoubleTapTracker>()
            .add_plugins((desktop::DesktopControlsPlugin, mobile::MobileControlsPlugin))
            .add_systems(Update, auto_detect_control_scheme);
    }
}

/// Automatically synchronizes ControlScheme with NetClient.my_platform or touch presence
fn auto_detect_control_scheme(
    net_client: Res<NetClient>,
    touches: Res<Touches>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut scheme: ResMut<ControlScheme>,
) {
    if net_client.my_platform == ClientPlatform::Mobile && *scheme != ControlScheme::MobileTouch {
        *scheme = ControlScheme::MobileTouch;
        info!("📱 [Controls] Switched to MobileTouch control scheme based on platform detection");
    } else if touches.iter().next().is_some() && *scheme != ControlScheme::MobileTouch {
        *scheme = ControlScheme::MobileTouch;
        info!("📱 [Controls] Active touch detected; switched to MobileTouch control scheme");
    } else if (mouse_button.get_just_pressed().next().is_some() || keyboard.get_just_pressed().next().is_some())
        && *scheme != ControlScheme::DesktopMouseKeyboard
        && net_client.my_platform != ClientPlatform::Mobile
    {
        *scheme = ControlScheme::DesktopMouseKeyboard;
        info!("🖥️ [Controls] Mouse/Keyboard input detected; restored DesktopMouseKeyboard control scheme");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared Order & Selection Helpers (Used by both Desktop and Mobile systems)
// ─────────────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
/// Type alias for querying selectable units
pub type UnitQueryItem<'a> = (
    Entity,
    &'a Transform,
    &'a Faction,
    &'a mut Selectable,
    Option<&'a NetEntity>,
    Option<&'a mut MoveTarget>,
    Option<&'a mut TacticalStance>,
    Option<&'a mut Worker>,
    Option<&'a mut Soldier>,
    Option<&'a mut MeleeFighter>,
);

/// Issues a ground movement or attack-move order with formation spread and A* pathfinding
pub fn dispatch_move_order(
    commands: &mut Commands,
    target_world_pos: Vec2,
    is_attack_move: bool,
    net_client: &NetClient,
    stats: &mut MatchStats,
    nav_grid: &NavGrid,
    sound_events: &mut EventWriter<SoundEffect>,
    mut unit_query: Query<UnitQueryItem>,
) {
    let mut selected_units = Vec::new();
    let mut selected_net_ids = Vec::new();

    for (entity, tf, faction, selectable, net_opt, ..) in &unit_query {
        if *faction == net_client.my_faction && selectable.is_selected {
            selected_units.push((entity, tf.translation.truncate()));
            if let Some(net) = net_opt {
                selected_net_ids.push(net.net_id);
            }
        }
    }

    if selected_units.is_empty() {
        return;
    }

    sound_events.send(SoundEffect::OrderIssued);
    stats.record_action();

    // Send networked command if online
    if net_client.status != NetStatus::Disconnected && !selected_net_ids.is_empty() {
        net_client.send(&ClientMessage::RequestMove {
            unit_net_ids: selected_net_ids,
            target_position: target_world_pos,
            is_attack_move,
        });
    }

    let unit_count = selected_units.len();

    // Assign formation destinations and A* paths
    for (i, (entity, unit_pos)) in selected_units.iter().enumerate() {
        let formation_offset = if unit_count > 1 {
            let angle = (i as f32) * 2.39996;
            let dist = 24.0 * (i as f32).sqrt();
            Vec2::new(angle.cos(), angle.sin()) * dist
        } else {
            Vec2::ZERO
        };

        let destination = target_world_pos + formation_offset;
        let waypoints = nav_grid.find_path(*unit_pos, destination);

        if let Ok((_, _, _, _, _, move_target_opt, stance_opt, worker_opt, soldier_opt, melee_opt)) =
            unit_query.get_mut(*entity)
        {
            if let Some(mut worker) = worker_opt {
                worker.state = WorkerState::Idle;
                worker.target_node = None;
            }
            if let Some(mut soldier) = soldier_opt {
                soldier.target = None;
                soldier.state = if is_attack_move {
                    SoldierState::AttackMoving
                } else {
                    SoldierState::MovingToGround
                };
            }
            if let Some(mut melee) = melee_opt {
                melee.target = None;
                melee.state = if is_attack_move {
                    SoldierState::AttackMoving
                } else {
                    SoldierState::MovingToGround
                };
            }

            if let Some(mut stance) = stance_opt {
                *stance = TacticalStance::Aggressive;
            }

            if let Some(mut existing_target) = move_target_opt {
                existing_target.destination = destination;
                existing_target.is_attack_move = is_attack_move;
                existing_target.waypoints = waypoints;
                existing_target.current_waypoint_idx = 0;
            } else {
                commands.entity(*entity).insert(MoveTarget::with_waypoints(
                    destination,
                    is_attack_move,
                    waypoints,
                ));
            }
        }
    }

    // Visual tactical pulse marker
    let marker_color = if is_attack_move {
        Color::srgba(0.95, 0.45, 0.20, 0.95) // Orange/Red for Attack-Move
    } else {
        Color::srgba(0.25, 0.95, 0.45, 0.95) // Bright green for Move
    };

    commands.spawn((
        CommandMarker {
            lifetime: 0.0,
            max_lifetime: 0.45,
            initial_radius: 20.0,
            color: marker_color,
        },
        Transform::from_xyz(target_world_pos.x, target_world_pos.y, 1.0),
    ));
}

/// Issues a direct focus-fire attack order on an enemy target
pub fn dispatch_attack_target(
    commands: &mut Commands,
    target_entity: Entity,
    target_net_id: Option<u32>,
    target_pos: Vec2,
    net_client: &NetClient,
    stats: &mut MatchStats,
    sound_events: &mut EventWriter<SoundEffect>,
    mut unit_query: Query<UnitQueryItem>,
) {
    let mut selected_net_ids = Vec::new();
    let mut any_selected = false;

    for (entity, _, faction, selectable, net_opt, _, _, worker_opt, soldier_opt, melee_opt) in
        &mut unit_query
    {
        if *faction == net_client.my_faction && selectable.is_selected {
            any_selected = true;
            if let Some(net) = net_opt {
                selected_net_ids.push(net.net_id);
            }
            commands.entity(entity).remove::<MoveTarget>();
            if let Some(mut worker) = worker_opt {
                worker.state = WorkerState::Idle;
                worker.target_node = None;
            }
            if let Some(mut soldier) = soldier_opt {
                soldier.target = Some(target_entity);
                soldier.state = SoldierState::ChasingTarget;
            }
            if let Some(mut melee) = melee_opt {
                melee.target = Some(target_entity);
                melee.state = SoldierState::ChasingTarget;
            }
        }
    }

    if !any_selected {
        return;
    }

    stats.record_action();
    sound_events.send(SoundEffect::OrderIssued);

    // Spawn red attack pulse marker
    commands.spawn((
        CommandMarker {
            lifetime: 0.0,
            max_lifetime: 0.45,
            initial_radius: 22.0,
            color: Color::srgba(0.95, 0.25, 0.25, 0.95), // Red
        },
        Transform::from_xyz(target_pos.x, target_pos.y, 1.0),
    ));

    // Send networked attack order
    if net_client.status != NetStatus::Disconnected && !selected_net_ids.is_empty() {
        if let Some(t_net_id) = target_net_id {
            net_client.send(&ClientMessage::RequestAttackTarget {
                unit_net_ids: selected_net_ids,
                target_net_id: t_net_id,
            });
        }
    }
}

/// Issues a mineral harvest order to selected workers
pub fn dispatch_harvest_order(
    commands: &mut Commands,
    target_node_pos: Vec2,
    _net_client: &NetClient,
    stats: &mut MatchStats,
    sound_events: &mut EventWriter<SoundEffect>,
) {
    stats.record_action();
    sound_events.send(SoundEffect::OrderIssued);

    // Cyan harvest pulse marker
    commands.spawn((
        CommandMarker {
            lifetime: 0.0,
            max_lifetime: 0.45,
            initial_radius: 20.0,
            color: Color::srgba(0.20, 0.90, 1.0, 0.95), // Cyan
        },
        Transform::from_xyz(target_node_pos.x, target_node_pos.y, 1.0),
    ));
}

/// Issues a stop order to all selected units
pub fn dispatch_stop_order(
    commands: &mut Commands,
    net_client: &NetClient,
    stats: &mut MatchStats,
    sound_events: &mut EventWriter<SoundEffect>,
    mut unit_query: Query<(
        Entity,
        &Faction,
        &Selectable,
        Option<&NetEntity>,
        Option<&mut Soldier>,
        Option<&mut MeleeFighter>,
        Option<&mut TacticalStance>,
    )>,
) {
    let mut net_ids = Vec::new();
    let mut any_selected = false;

    for (entity, faction, selectable, net_opt, mut soldier_opt, mut melee_opt, mut stance_opt) in
        &mut unit_query
    {
        if *faction == net_client.my_faction && selectable.is_selected {
            any_selected = true;
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

    if any_selected {
        stats.record_action();
        if !net_ids.is_empty() && net_client.status != NetStatus::Disconnected {
            net_client.send(&ClientMessage::RequestStop { unit_net_ids: net_ids });
        }
        sound_events.send(SoundEffect::OrderIssued);
        info!("🛑 [Controls] Stop command issued to selected units");
    }
}

/// Issues a hold position order to all selected units
pub fn dispatch_hold_order(
    commands: &mut Commands,
    net_client: &NetClient,
    stats: &mut MatchStats,
    sound_events: &mut EventWriter<SoundEffect>,
    mut unit_query: Query<(
        Entity,
        &Faction,
        &Selectable,
        Option<&NetEntity>,
        Option<&mut Soldier>,
        Option<&mut MeleeFighter>,
        Option<&mut TacticalStance>,
    )>,
) {
    let mut net_ids = Vec::new();
    let mut any_selected = false;

    for (entity, faction, selectable, net_opt, mut soldier_opt, mut melee_opt, mut stance_opt) in
        &mut unit_query
    {
        if *faction == net_client.my_faction && selectable.is_selected {
            any_selected = true;
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

    if any_selected {
        stats.record_action();
        if !net_ids.is_empty() && net_client.status != NetStatus::Disconnected {
            net_client.send(&ClientMessage::RequestHoldPosition { unit_net_ids: net_ids });
        }
        sound_events.send(SoundEffect::OrderIssued);
        info!("🛡️ [Controls] Hold Position command issued to selected units");
    }
}

/// Selects all combat units (Soldiers and Melee Fighters) belonging to the player
pub fn select_all_combat_units(
    my_faction: Faction,
    stats: &mut MatchStats,
    sound_events: &mut EventWriter<SoundEffect>,
    mut query: Query<(&Faction, &mut Selectable, Option<&Soldier>, Option<&MeleeFighter>, Option<&Worker>)>,
) {
    let mut any_selected = false;

    for (faction, mut selectable, soldier_opt, melee_opt, worker_opt) in &mut query {
        if *faction == my_faction {
            if soldier_opt.is_some() || melee_opt.is_some() {
                selectable.is_selected = true;
                any_selected = true;
            } else if worker_opt.is_some() {
                selectable.is_selected = false;
            }
        }
    }

    if any_selected {
        stats.record_action();
        sound_events.send(SoundEffect::MarineSelect);
        info!("⚔️ [Controls] All combat units selected");
    }
}

/// Selects all workers belonging to the player
pub fn select_all_workers(
    my_faction: Faction,
    stats: &mut MatchStats,
    sound_events: &mut EventWriter<SoundEffect>,
    mut query: Query<(&Faction, &mut Selectable, Option<&Worker>)>,
) {
    let mut any_selected = false;

    for (faction, mut selectable, worker_opt) in &mut query {
        if *faction == my_faction {
            if worker_opt.is_some() {
                selectable.is_selected = true;
                any_selected = true;
            } else {
                selectable.is_selected = false;
            }
        }
    }

    if any_selected {
        stats.record_action();
        sound_events.send(SoundEffect::WorkerSelect);
        info!("⛏️ [Controls] All workers selected");
    }
}

/// Clears selection on all entities
pub fn clear_all_selection(mut query: Query<&mut Selectable>) {
    for mut selectable in &mut query {
        selectable.is_selected = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn test_control_scheme_defaults_and_run_conditions() {
        let scheme = ControlScheme::default();
        assert_eq!(scheme, ControlScheme::DesktopMouseKeyboard);

        let mut app = App::new();
        app.insert_resource(ControlScheme::DesktopMouseKeyboard);

        let desktop_cond = app.world_mut().run_system_once(is_desktop_control_scheme).unwrap();
        assert!(desktop_cond);

        let mobile_cond = app.world_mut().run_system_once(is_mobile_control_scheme).unwrap();
        assert!(!mobile_cond);

        // Switch to mobile touch
        *app.world_mut().resource_mut::<ControlScheme>() = ControlScheme::MobileTouch;

        let desktop_cond_after = app.world_mut().run_system_once(is_desktop_control_scheme).unwrap();
        assert!(!desktop_cond_after);

        let mobile_cond_after = app.world_mut().run_system_once(is_mobile_control_scheme).unwrap();
        assert!(mobile_cond_after);
    }

    #[test]
    fn test_quick_action_select_all_army_and_workers() {
        let mut app = App::new();
        app.add_event::<SoundEffect>();
        app.insert_resource(MatchStats::default());

        // Spawn friendly worker
        let w1 = app
            .world_mut()
            .spawn((
                Faction::Player1,
                Selectable { is_selected: false },
                Worker::default(),
            ))
            .id();

        // Spawn friendly soldier
        let s1 = app
            .world_mut()
            .spawn((
                Faction::Player1,
                Selectable { is_selected: false },
                Soldier::default(),
            ))
            .id();

        // Spawn enemy soldier
        let enemy_s = app
            .world_mut()
            .spawn((
                Faction::Player2,
                Selectable { is_selected: false },
                Soldier::default(),
            ))
            .id();

        // 1. Select all combat units
        app.world_mut().run_system_once(
            |mut stats: ResMut<MatchStats>,
             mut sound: EventWriter<SoundEffect>,
             query: Query<(&Faction, &mut Selectable, Option<&Soldier>, Option<&MeleeFighter>, Option<&Worker>)>| {
                select_all_combat_units(Faction::Player1, &mut stats, &mut sound, query);
            },
        ).unwrap();

        assert!(!app.world().get::<Selectable>(w1).unwrap().is_selected);
        assert!(app.world().get::<Selectable>(s1).unwrap().is_selected);
        assert!(!app.world().get::<Selectable>(enemy_s).unwrap().is_selected);

        // 2. Select all workers
        app.world_mut().run_system_once(
            |mut stats: ResMut<MatchStats>,
             mut sound: EventWriter<SoundEffect>,
             query: Query<(&Faction, &mut Selectable, Option<&Worker>)>| {
                select_all_workers(Faction::Player1, &mut stats, &mut sound, query);
            },
        ).unwrap();

        assert!(app.world().get::<Selectable>(w1).unwrap().is_selected);
        assert!(!app.world().get::<Selectable>(s1).unwrap().is_selected);
        assert!(!app.world().get::<Selectable>(enemy_s).unwrap().is_selected);

        // 3. Clear all selection
        app.world_mut().run_system_once(clear_all_selection).unwrap();
        assert!(!app.world().get::<Selectable>(w1).unwrap().is_selected);
        assert!(!app.world().get::<Selectable>(s1).unwrap().is_selected);
        assert!(!app.world().get::<Selectable>(enemy_s).unwrap().is_selected);
    }
}

