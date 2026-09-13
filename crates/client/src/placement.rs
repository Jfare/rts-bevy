use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::BuildingKind;
use crate::audio_sfx::SoundEffect;
use crate::net::{NetClient, NetStatus};
use crate::selection::screen_to_world_2d;
use crate::stats::MatchStats;

/// Active placement state when the player is positioning a new building
#[derive(Debug, Resource, Default)]
pub struct PlacementState {
    pub active_kind: Option<BuildingKind>,
    pub ghost_pos: Vec2,
    pub is_valid: bool,
    pub mineral_cost: u32,
    /// When entering placement mode via a UI click/touch, we wait until that click/touch
    /// is completely released before accepting placement confirmations or snapping to the cursor.
    pub awaiting_initial_release: bool,
    /// True only when the player has actively indicated a placement location (e.g. mouse hover on desktop, or touch tap/drag on mobile).
    /// Prevents ghost artifacts from rendering over the Command Center / camera center before the user chooses a spot.
    pub has_preview_position: bool,
}

pub struct PlacementPlugin;

impl Plugin for PlacementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlacementState>()
            .add_systems(
                Update,
                (
                    handle_placement_input,
                    update_placement_validation,
                    draw_placement_ghost,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Bundled system parameters for placement input handling
#[derive(SystemParam)]
pub struct PlacementInputParams<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub keyboard: Res<'w, ButtonInput<KeyCode>>,
    pub mouse_button: Res<'w, ButtonInput<MouseButton>>,
    pub touches: Res<'w, Touches>,
    pub net_client: Res<'w, NetClient>,
    pub outcome_opt: Option<Res<'w, MatchOutcome>>,
    pub economy: ResMut<'w, PlayerEconomy>,
    pub stats: ResMut<'w, MatchStats>,
    pub sound_events: EventWriter<'w, SoundEffect>,
    pub state: ResMut<'w, PlacementState>,
    pub control_scheme: Option<Res<'w, crate::controls::ControlScheme>>,
    pub mobile_build_menu: Option<Res<'w, crate::ui::mobile_hud::MobileBuildMenuOpen>>,
    pub mobile_unit_menu: Option<Res<'w, crate::ui::mobile_hud::MobileUnitProductionOpen>>,
    pub minimap_opt: Option<Res<'w, crate::minimap::MinimapState>>,
}

/// Handles hotkeys and mouse clicks for building placement
fn handle_placement_input(
    mut p: PlacementInputParams,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>)>,
    building_query: Query<(&Transform, &Radius, &Building, &Faction)>,
    resource_query: Query<(&Transform, &Radius), With<ResourceNode>>,
    unit_query: Query<(&Transform, &Radius), With<Unit>>,
    selectable_query: Query<(&Faction, &Selectable)>,
) {
    if p.outcome_opt.as_deref() == Some(&MatchOutcome::Victory) || p.outcome_opt.as_deref() == Some(&MatchOutcome::Defeat) {
        return;
    }

    // 1. Hotkeys to enter placement mode
    if p.keyboard.just_pressed(KeyCode::KeyB) {
        p.stats.record_action();
        p.state.active_kind = Some(BuildingKind::Barracks);
        p.state.mineral_cost = BuildingKind::Barracks.mineral_cost();
        p.state.awaiting_initial_release = false;
        p.state.has_preview_position = false;
        info!("🏗️ [Build Mode] Barracks ($150) selected for placement");
    }
    if p.keyboard.just_pressed(KeyCode::KeyU) {
        p.stats.record_action();
        p.state.active_kind = Some(BuildingKind::Turret);
        p.state.mineral_cost = BuildingKind::Turret.mineral_cost();
        p.state.awaiting_initial_release = false;
        p.state.has_preview_position = false;
        info!("🏗️ [Build Mode] Gun Turret ($125) selected for placement");
    }
    if p.keyboard.just_pressed(KeyCode::KeyP) {
        p.stats.record_action();
        p.state.active_kind = Some(BuildingKind::SupplyDepot);
        p.state.mineral_cost = BuildingKind::SupplyDepot.mineral_cost();
        p.state.awaiting_initial_release = false;
        p.state.has_preview_position = false;
        info!("🏗️ [Build Mode] Supply Depot ($100) selected for placement");
    }
    if p.keyboard.just_pressed(KeyCode::KeyH) {
        p.stats.record_action();
        p.state.active_kind = Some(BuildingKind::BaseHQ);
        p.state.mineral_cost = BuildingKind::BaseHQ.mineral_cost();
        p.state.awaiting_initial_release = false;
        p.state.has_preview_position = false;
        info!("🏗️ [Build Mode] Base HQ ($400) selected for placement");
    }

    // 2. Cancel placement on Escape or Right-Click
    if (p.keyboard.just_pressed(KeyCode::Escape) || p.mouse_button.just_pressed(MouseButton::Right))
        && p.state.active_kind.is_some() {
            p.state.active_kind = None;
            p.state.awaiting_initial_release = false;
            p.state.has_preview_position = false;
            info!("❌ [Build Mode] Placement cancelled");
            return;
        }

    let Some(building_kind) = p.state.active_kind else {
        return;
    };

    let Ok(window) = window_query.get_single() else {
        return;
    };
    let Ok((_camera, cam_transform, ortho_opt)) = camera_query.get_single() else {
        return;
    };

    let win_size = Vec2::new(window.width(), window.height());
    let is_mobile = p.control_scheme.map_or(false, |s| *s == crate::controls::ControlScheme::MobileTouch)
        || p.net_client.my_platform == shared::protocol::ClientPlatform::Mobile
        || win_size.x < 960.0 || win_size.y < 550.0;

    let has_touches = p.touches.iter().count() > 0 || p.touches.any_just_released();
    let has_mouse_click = p.mouse_button.pressed(MouseButton::Left) || p.mouse_button.just_released(MouseButton::Left);

    // If we just entered placement mode from clicking a UI button, wait until the user
    // has completely released the click/touch that selected the building before accepting placement
    if p.state.awaiting_initial_release {
        if has_touches || has_mouse_click {
            return;
        }
        p.state.awaiting_initial_release = false;
    }

    // Determine the active screen coordinate for placement cursor:
    // When touches occur, touch takes full priority over mouse cursor.
    // On mobile without active touches or mouse click, avoid snapping to stale cursor.
    let cursor_screen = if has_touches {
        p.touches
            .first_pressed_position()
            .or_else(|| p.touches.iter_just_released().next().map(|t| t.position()))
    } else if !is_mobile || p.mouse_button.pressed(MouseButton::Left) || p.mouse_button.just_pressed(MouseButton::Left) {
        window.cursor_position()
    } else {
        None
    };

    let Some(cursor_screen) = cursor_screen else {
        return;
    };

    // Avoid updating ghost or confirming placement if clicking/tapping on HUD UI chrome
    let build_menu_open = p.mobile_build_menu.map_or(false, |m| m.0);
    let unit_menu_open = p.mobile_unit_menu.map_or(false, |m| m.0);
    let has_any_friendly_selection = selectable_query
        .iter()
        .any(|(fac, sel)| *fac == p.net_client.my_faction && sel.is_selected);

    let is_ui_touch = if is_mobile {
        crate::controls::mobile::is_mobile_ui_hit(
            cursor_screen,
            window,
            p.minimap_opt.as_deref(),
            has_any_friendly_selection,
            build_menu_open,
            unit_menu_open,
        )
    } else {
        cursor_screen.y <= 36.0 // top bar
            || (cursor_screen.x >= window.width() - 110.0 && cursor_screen.y <= 140.0) // top-right minimap
            || cursor_screen.y >= window.height() - 110.0 // bottom desktop command card
    };

    if is_ui_touch {
        return;
    }

    let cam_pos = cam_transform.translation.truncate();
    let cam_scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);
    let raw_world_pos = screen_to_world_2d(cursor_screen, win_size, cam_pos, cam_scale);

    // Snap to 16.0 pixel grid
    let snap_pos = (raw_world_pos / 16.0).round() * 16.0;
    p.state.ghost_pos = snap_pos;
    p.state.has_preview_position = true;

    let my_faction = p.net_client.my_faction;
    let is_valid = is_placement_valid(
        building_kind,
        snap_pos,
        p.state.mineral_cost,
        my_faction,
        &p.economy,
        &building_query,
        &resource_query,
        &unit_query,
    );
    p.state.is_valid = is_valid;

    // 3. Confirm placement on Left-Click or Touch Release
    let is_confirm = p.mouse_button.just_pressed(MouseButton::Left) || p.touches.any_just_released();
    if is_confirm && is_valid {
        if p.economy.spend_minerals(my_faction, p.state.mineral_cost) {
            p.stats.record_action();
            let spawn_pos = p.state.ghost_pos;
            p.sound_events.send(SoundEffect::BuildPlaced);
            if my_faction == Faction::Player1 {
                p.stats.minerals_spent += p.state.mineral_cost;
            }

            if p.net_client.status != NetStatus::Disconnected {
                p.net_client.send(&shared::protocol::ClientMessage::RequestBuild {
                    building_kind,
                    position: spawn_pos,
                });
            } else {
                let size = building_kind.size();
                let duration = building_kind.build_duration();
                let max_hp = building_kind.max_health();

                let radius = match building_kind {
                    BuildingKind::BaseHQ => 55.0,
                    BuildingKind::Barracks => 46.0,
                    BuildingKind::SupplyDepot => 30.0,
                    BuildingKind::Turret => 28.0,
                };

                let mut entity_cmds = p.commands.spawn((
                    Building::new(building_kind.name(), size, duration, false),
                    Health::new(max_hp),
                    my_faction,
                    Selectable::default(),
                    Radius(radius),
                    NetEntity {
                        net_id: 9000 + ((spawn_pos.x.abs() as u32 * 31 + spawn_pos.y.abs() as u32 * 17) % 1000),
                        owner_peer_id: 1,
                    },
                    Transform::from_xyz(spawn_pos.x, spawn_pos.y, 1.0),
                ));

                match building_kind {
                    BuildingKind::BaseHQ => {
                        entity_cmds.insert((
                            BaseHQ {
                                supply_provided: 10,
                                dropoff_radius: 70.0,
                            },
                            ProductionBuilding {
                                queue: Vec::new(),
                                current_timer: 0.0,
                                max_queue_size: 5,
                                rally_point: spawn_pos + Vec2::new(0.0, -100.0),
                            },
                        ));
                    }
                    BuildingKind::Barracks => {
                        entity_cmds.insert((
                            Barracks,
                            ProductionBuilding {
                                queue: Vec::new(),
                                current_timer: 0.0,
                                max_queue_size: 5,
                                rally_point: spawn_pos + Vec2::new(0.0, -90.0),
                            },
                        ));
                    }
                    BuildingKind::SupplyDepot => {
                        entity_cmds.insert(SupplyDepot {
                            supply_provided: 8,
                        });
                    }
                    BuildingKind::Turret => {
                        entity_cmds.insert(GunTurret::default());
                    }
                }
            }

            info!(
                "🏗️ [Build] Placed {} at {:?}! Construction started.",
                building_kind.name(),
                spawn_pos
            );

            // If Shift is NOT held, exit placement mode
            let shift_held = p.keyboard.pressed(KeyCode::ShiftLeft) || p.keyboard.pressed(KeyCode::ShiftRight);
            if !shift_held {
                p.state.active_kind = None;
                p.state.has_preview_position = false;
            }
        }
    }
}

/// Validates ghost placement against map boundaries, collisions, tech prerequisites, and mineral costs
pub fn is_placement_valid(
    building_kind: BuildingKind,
    ghost_pos: Vec2,
    mineral_cost: u32,
    my_faction: Faction,
    economy: &PlayerEconomy,
    building_query: &Query<(&Transform, &Radius, &Building, &Faction)>,
    resource_query: &Query<(&Transform, &Radius), With<ResourceNode>>,
    unit_query: &Query<(&Transform, &Radius), With<Unit>>,
) -> bool {
    // 1. Check Tech Tree Prerequisites
    if building_kind == BuildingKind::Turret {
        let has_barracks = building_query.iter().any(|(_, _, b, f)| {
            *f == my_faction && b.name.contains("Barracks") && b.is_constructed
        });
        if !has_barracks {
            return false;
        }
    }

    // 2. Check Mineral Funds
    if !economy.has_minerals(my_faction, mineral_cost) {
        return false;
    }

    let size = building_kind.size();
    let ghost_radius = match building_kind {
        BuildingKind::BaseHQ => 55.0,
        BuildingKind::Barracks => 46.0,
        BuildingKind::SupplyDepot => 30.0,
        BuildingKind::Turret => 28.0,
    };

    // 3. Check Map Boundaries (-1500 .. 1500)
    if ghost_pos.x - size.x * 0.5 < -1500.0
        || ghost_pos.x + size.x * 0.5 > 1500.0
        || ghost_pos.y - size.y * 0.5 < -1500.0
        || ghost_pos.y + size.y * 0.5 > 1500.0
    {
        return false;
    }

    // 4. Check Overlap with Existing Buildings
    for (transform, radius, _, _) in building_query {
        let b_pos = transform.translation.truncate();
        let min_dist = ghost_radius + radius.0 + 8.0;
        if ghost_pos.distance(b_pos) < min_dist {
            return false;
        }
    }

    // 5. Check Overlap with Resource Nodes
    for (transform, radius) in resource_query {
        let r_pos = transform.translation.truncate();
        let min_dist = ghost_radius + radius.0 + 16.0;
        if ghost_pos.distance(r_pos) < min_dist {
            return false;
        }
    }

    // 6. Check Overlap with Units
    for (transform, radius) in unit_query {
        let u_pos = transform.translation.truncate();
        let min_dist = ghost_radius + radius.0;
        if ghost_pos.distance(u_pos) < min_dist {
            return false;
        }
    }

    // 7. Check Overlap with Static Map Obstacles (Rocks, Cliff Bluffs)
    if shared::map::is_obstacle_blocked(ghost_pos, ghost_radius, 8.0) {
        return false;
    }

    true
}

/// Validates ghost placement against map boundaries, collisions, tech prerequisites, and mineral costs
fn update_placement_validation(
    economy: Res<PlayerEconomy>,
    net_client: Res<NetClient>,
    mut state: ResMut<PlacementState>,
    building_query: Query<(&Transform, &Radius, &Building, &Faction)>,
    resource_query: Query<(&Transform, &Radius), With<ResourceNode>>,
    unit_query: Query<(&Transform, &Radius), With<Unit>>,
) {
    let Some(building_kind) = state.active_kind else {
        state.is_valid = false;
        return;
    };

    if !state.has_preview_position {
        state.is_valid = false;
        return;
    }

    let my_faction = net_client.my_faction;
    state.is_valid = is_placement_valid(
        building_kind,
        state.ghost_pos,
        state.mineral_cost,
        my_faction,
        &economy,
        &building_query,
        &resource_query,
        &unit_query,
    );
}

/// Renders the ghost building preview and snapping grid indicators
fn draw_placement_ghost(mut gizmos: Gizmos, state: Res<PlacementState>) {
    let Some(building_kind) = state.active_kind else {
        return;
    };

    if !state.has_preview_position {
        return;
    }

    let pos = state.ghost_pos;
    let size = building_kind.size();

    let (border_col, fill_col) = if state.is_valid {
        (
            Color::srgba(0.20, 0.95, 0.45, 0.95), // Green
            Color::srgba(0.20, 0.95, 0.45, 0.35),
        )
    } else {
        (
            Color::srgba(0.95, 0.25, 0.25, 0.95), // Red
            Color::srgba(0.95, 0.25, 0.25, 0.35),
        )
    };

    // Ghost rectangle
    gizmos.rect_2d(pos, size, border_col);
    gizmos.rect_2d(pos, (size - Vec2::splat(4.0)).max(Vec2::ZERO), fill_col);

    // Diagonal construction cross
    let half = size * 0.5;
    gizmos.line_2d(pos - half, pos + half, border_col.with_alpha(0.6));
    gizmos.line_2d(
        pos + Vec2::new(-half.x, half.y),
        pos + Vec2::new(half.x, -half.y),
        border_col.with_alpha(0.6),
    );

    // 16px Grid alignment crosshair
    gizmos.line_2d(
        pos + Vec2::new(-size.x, 0.0),
        pos + Vec2::new(size.x, 0.0),
        Color::srgba(1.0, 1.0, 1.0, 0.2),
    );
    gizmos.line_2d(
        pos + Vec2::new(0.0, -size.y),
        pos + Vec2::new(0.0, size.y),
        Color::srgba(1.0, 1.0, 1.0, 0.2),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::touch::{TouchInput, TouchPhase};
    use bevy::window::WindowResolution;

    #[test]
    fn test_touch_placement_confirms_on_map() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::input::InputPlugin);
        app.init_resource::<MatchStats>();
        app.add_event::<SoundEffect>();

        let mut net_client = NetClient::default();
        net_client.my_faction = Faction::Player1;
        net_client.status = NetStatus::Disconnected;
        app.insert_resource(net_client);

        let mut economy = PlayerEconomy::default();
        economy.set_minerals(Faction::Player1, 500);
        app.insert_resource(economy);

        let mut placement = PlacementState::default();
        placement.active_kind = Some(BuildingKind::SupplyDepot);
        placement.mineral_cost = BuildingKind::SupplyDepot.mineral_cost();
        placement.is_valid = true;
        app.insert_resource(placement);

        let mut window = Window::default();
        window.resolution = WindowResolution::new(800.0, 400.0);
        let win_ent = app.world_mut().spawn((window, PrimaryWindow)).id();

        app.world_mut().spawn((
            Camera::default(),
            Camera2d,
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));

        app.add_systems(Update, handle_placement_input);

        // Simulate touch tap in center of screen (400.0, 200.0)
        let touch_loc = Vec2::new(400.0, 200.0);
        app.world_mut().send_event(TouchInput {
            phase: TouchPhase::Started,
            position: touch_loc,
            force: None,
            id: 1,
            window: win_ent,
        });
        app.world_mut().send_event(TouchInput {
            phase: TouchPhase::Ended,
            position: touch_loc,
            force: None,
            id: 1,
            window: win_ent,
        });

        // Run frame update
        app.update();

        // Building should be placed: active_kind becomes None and minerals are spent
        let p_state = app.world().resource::<PlacementState>();
        assert!(p_state.active_kind.is_none(), "Placement should be confirmed and active_kind reset to None");

        let econ = app.world().resource::<PlayerEconomy>();
        assert_eq!(econ.get_minerals(Faction::Player1), 400, "100 minerals should be spent for Supply Depot");

        // Building entity should exist in world
        let bldg_count = app.world_mut().query::<&Building>().iter(app.world()).count();
        assert_eq!(bldg_count, 1);
    }

    #[test]
    fn test_touch_placement_ignored_on_hud_chrome() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::input::InputPlugin);
        app.init_resource::<MatchStats>();
        app.add_event::<SoundEffect>();

        let mut net_client = NetClient::default();
        net_client.my_faction = Faction::Player1;
        app.insert_resource(net_client);

        let mut economy = PlayerEconomy::default();
        economy.set_minerals(Faction::Player1, 500);
        app.insert_resource(economy);

        let mut placement = PlacementState::default();
        placement.active_kind = Some(BuildingKind::SupplyDepot);
        placement.mineral_cost = BuildingKind::SupplyDepot.mineral_cost();
        placement.is_valid = true;
        app.insert_resource(placement);

        let mut window = Window::default();
        window.resolution = WindowResolution::new(800.0, 400.0);
        let win_ent = app.world_mut().spawn((window, PrimaryWindow)).id();

        app.world_mut().spawn((
            Camera::default(),
            Camera2d,
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));

        app.add_systems(Update, handle_placement_input);

        // Simulate touch tap in top bar area (y = 20.0)
        let touch_loc = Vec2::new(400.0, 20.0);
        app.world_mut().send_event(TouchInput {
            phase: TouchPhase::Started,
            position: touch_loc,
            force: None,
            id: 2,
            window: win_ent,
        });
        app.world_mut().send_event(TouchInput {
            phase: TouchPhase::Ended,
            position: touch_loc,
            force: None,
            id: 2,
            window: win_ent,
        });

        // Run frame update
        app.update();

        // Building should NOT be placed: active_kind stays Some and minerals not spent
        let p_state = app.world().resource::<PlacementState>();
        assert_eq!(p_state.active_kind, Some(BuildingKind::SupplyDepot));

        let econ = app.world().resource::<PlayerEconomy>();
        assert_eq!(econ.get_minerals(Faction::Player1), 500);
    }

    #[test]
    fn test_awaiting_initial_release_blocks_placement_until_released() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::input::InputPlugin);
        app.init_resource::<MatchStats>();
        app.add_event::<SoundEffect>();

        let mut net_client = NetClient::default();
        net_client.my_faction = Faction::Player1;
        net_client.status = NetStatus::Disconnected;
        app.insert_resource(net_client);

        let mut economy = PlayerEconomy::default();
        economy.set_minerals(Faction::Player1, 500);
        app.insert_resource(economy);

        let mut placement = PlacementState::default();
        placement.active_kind = Some(BuildingKind::SupplyDepot);
        placement.mineral_cost = BuildingKind::SupplyDepot.mineral_cost();
        placement.is_valid = true;
        placement.awaiting_initial_release = true; // Simulates entering placement mode from button press
        app.insert_resource(placement);

        let mut window = Window::default();
        window.resolution = WindowResolution::new(800.0, 400.0);
        let win_ent = app.world_mut().spawn((window, PrimaryWindow)).id();

        app.world_mut().spawn((
            Camera::default(),
            Camera2d,
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));

        app.add_systems(Update, handle_placement_input);

        // Frame 1: Touch is released from button click
        let button_loc = Vec2::new(740.0, 310.0);
        app.world_mut().send_event(TouchInput {
            phase: TouchPhase::Ended,
            position: button_loc,
            force: None,
            id: 3,
            window: win_ent,
        });

        app.update();

        // Building must NOT be placed on button release!
        let p_state = app.world().resource::<PlacementState>();
        assert_eq!(p_state.active_kind, Some(BuildingKind::SupplyDepot));
        let econ = app.world().resource::<PlayerEconomy>();
        assert_eq!(econ.get_minerals(Faction::Player1), 500);

        // Frame 2: Touch released, awaiting_initial_release transitions to false
        app.update();
        let p_state = app.world().resource::<PlacementState>();
        assert!(!p_state.awaiting_initial_release);
        assert_eq!(p_state.active_kind, Some(BuildingKind::SupplyDepot));

        // Frame 3: Player now taps on the map to place
        let map_loc = Vec2::new(400.0, 200.0);
        app.world_mut().send_event(TouchInput {
            phase: TouchPhase::Started,
            position: map_loc,
            force: None,
            id: 4,
            window: win_ent,
        });
        app.world_mut().send_event(TouchInput {
            phase: TouchPhase::Ended,
            position: map_loc,
            force: None,
            id: 4,
            window: win_ent,
        });

        app.update();

        // Now placement should be successfully confirmed on the map!
        let p_state = app.world().resource::<PlacementState>();
        assert!(p_state.active_kind.is_none());
        let econ = app.world().resource::<PlayerEconomy>();
        assert_eq!(econ.get_minerals(Faction::Player1), 400);
    }
}
