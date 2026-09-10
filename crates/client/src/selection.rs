use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::{AppState, Building, Faction, MeleeFighter, Radius, Selectable, Soldier, Worker};
use shared::grid::WorldGridConfig;
use crate::audio_sfx::SoundEffect;
use crate::fog_of_war::{FogOfWarGrid, FogState};
use crate::minimap::{get_minimap_screen_rect, MinimapState};
use crate::net::NetClient;

/// Helper to convert screen cursor coordinates to 2D world coordinates accurately across all platforms
pub fn screen_to_world_2d(
    cursor_pos: Vec2,
    window_size: Vec2,
    camera_pos: Vec2,
    camera_scale: f32,
) -> Vec2 {
    let centered = Vec2::new(
        cursor_pos.x - window_size.x * 0.5,
        (window_size.y * 0.5) - cursor_pos.y, // Invert Y
    );
    camera_pos + centered * camera_scale
}

/// State of the active drag selection marquee box
#[derive(Debug, Resource, Default)]
pub struct SelectionState {
    pub drag_start_screen: Option<Vec2>,
    pub drag_start_world: Option<Vec2>,
    pub current_world_pos: Option<Vec2>,
    pub is_dragging: bool,
}

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectionState>()
            .add_systems(
                Update,
                (handle_selection_input, draw_selection_gizmos).run_if(in_state(AppState::InGame)),
            );
    }
}

/// Handles mouse input for single-click and drag-box entity selection
fn handle_selection_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    net_client: Res<NetClient>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    fog: Res<FogOfWarGrid>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>)>,
    mut selection_state: ResMut<SelectionState>,
    mut sound_events: EventWriter<SoundEffect>,
    mut selectable_query: Query<(
        Entity,
        &Transform,
        &Radius,
        &Faction,
        &mut Selectable,
        Option<&Soldier>,
        Option<&MeleeFighter>,
        Option<&Worker>,
        Option<&Building>,
    )>,
) {
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);

    let Ok((_camera, cam_transform, ortho_opt)) = camera_query.get_single() else {
        return;
    };
    let Ok(window) = window_query.get_single() else {
        return;
    };

    let Some(cursor_screen) = window.cursor_position() else {
        return;
    };

    let mm_rect = get_minimap_screen_rect(window, &MinimapState::default());
    let is_over_minimap = cursor_screen.x >= mm_rect.min.x
        && cursor_screen.x <= mm_rect.max.x
        && cursor_screen.y >= mm_rect.min.y
        && cursor_screen.y <= mm_rect.max.y;

    let win_size = Vec2::new(window.width(), window.height());
    let cam_pos = cam_transform.translation.truncate();
    let cam_scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);

    let cursor_world_pos = screen_to_world_2d(cursor_screen, win_size, cam_pos, cam_scale);

    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    // 1. Mouse Button Pressed (Start Drag)
    if mouse_button.just_pressed(MouseButton::Left) {
        if is_over_minimap {
            return;
        }
        selection_state.drag_start_screen = Some(cursor_screen);
        selection_state.drag_start_world = Some(cursor_world_pos);
        selection_state.current_world_pos = Some(cursor_world_pos);
        selection_state.is_dragging = false;
    }

    // 2. Mouse Button Held (Update Drag)
    if mouse_button.pressed(MouseButton::Left) {
        if let Some(start_screen) = selection_state.drag_start_screen {
            if start_screen.distance(cursor_screen) > 6.0 {
                selection_state.is_dragging = true;
                selection_state.current_world_pos = Some(cursor_world_pos);
            }
        }
    }

    // 3. Mouse Button Released (Commit Selection)
    if mouse_button.just_released(MouseButton::Left) {
        let start_world = selection_state.drag_start_world.unwrap_or(cursor_world_pos);
        let current_world = cursor_world_pos;

        if !shift_held {
            // Clear existing selection if not shift-adding
            for (_, _, _, _, mut sel, ..) in &mut selectable_query {
                sel.is_selected = false;
            }
        }

        if selection_state.is_dragging {
            // Drag Box Selection
            let min_x = start_world.x.min(current_world.x);
            let max_x = start_world.x.max(current_world.x);
            let min_y = start_world.y.min(current_world.y);
            let max_y = start_world.y.max(current_world.y);

            let mut friendly_selected = false;
            let mut sound_played = false;

            // Pass 1: Select friendly units inside the box (strictly excluding buildings)
            for (_, transform, _, faction, mut sel, soldier_opt, melee_opt, worker_opt, bldg_opt) in &mut selectable_query {
                if bldg_opt.is_some() {
                    continue;
                }
                let pos = transform.translation.truncate();
                if pos.x >= min_x && pos.x <= max_x && pos.y >= min_y && pos.y <= max_y
                    && *faction == net_client.my_faction {
                        sel.is_selected = true;
                        friendly_selected = true;

                        if !sound_played {
                            if melee_opt.is_some() {
                                sound_events.send(SoundEffect::MeleeSelect);
                            } else if soldier_opt.is_some() {
                                sound_events.send(SoundEffect::MarineSelect);
                            } else if worker_opt.is_some() {
                                sound_events.send(SoundEffect::WorkerSelect);
                            }
                            sound_played = true;
                        }
                    }
            }

            // Pass 2: If no friendly units were inside, select any visible units inside for inspection (strictly excluding buildings)
            if !friendly_selected {
                for (_, transform, _, faction, mut sel, _, _, _, bldg_opt) in &mut selectable_query {
                    if bldg_opt.is_some() {
                        continue;
                    }
                    let pos = transform.translation.truncate();
                    if pos.x >= min_x && pos.x <= max_x && pos.y >= min_y && pos.y <= max_y {
                        if *faction != net_client.my_faction && *faction != Faction::Neutral
                            && fog.get_state_at_world_pos(pos, config) != FogState::Visible {
                                continue;
                            }
                        sel.is_selected = true;
                    }
                }
            }
        } else {
            // Single Click Selection
            // Priority:
            // 1. Direct click on unit (dist <= radius)
            // 2. Direct click on building (dist <= radius)
            // 3. Proximity click near unit (dist <= radius + 24.0)
            // 4. Proximity click near building (dist <= radius + 24.0)
            let mut direct_unit = None;
            let mut direct_unit_dist = f32::MAX;

            let mut direct_bldg = None;
            let mut direct_bldg_dist = f32::MAX;

            let mut near_unit = None;
            let mut near_unit_dist = f32::MAX;

            let mut near_bldg = None;
            let mut near_bldg_dist = f32::MAX;

            for (entity, transform, radius, faction, _, _, _, _, bldg_opt) in &selectable_query {
                let pos = transform.translation.truncate();
                // Skip selecting shrouded enemies in unexplored or non-visible fog
                if *faction != net_client.my_faction && *faction != Faction::Neutral
                    && fog.get_state_at_world_pos(pos, config) != FogState::Visible {
                        continue;
                    }

                let dist = pos.distance(start_world);
                let is_bldg = bldg_opt.is_some();

                if dist <= radius.0 {
                    if is_bldg {
                        if dist < direct_bldg_dist {
                            direct_bldg_dist = dist;
                            direct_bldg = Some(entity);
                        }
                    } else if dist < direct_unit_dist {
                        direct_unit_dist = dist;
                        direct_unit = Some(entity);
                    }
                } else if dist <= (radius.0 + 24.0) {
                    if is_bldg {
                        if dist < near_bldg_dist {
                            near_bldg_dist = dist;
                            near_bldg = Some(entity);
                        }
                    } else if dist < near_unit_dist {
                        near_unit_dist = dist;
                        near_unit = Some(entity);
                    }
                }
            }

            let target_candidate = direct_unit
                .or(direct_bldg)
                .or(near_unit)
                .or(near_bldg);

            if let Some(target_entity) = target_candidate {
                if let Ok((_, _, _, faction, mut sel, soldier_opt, melee_opt, worker_opt, _)) = selectable_query.get_mut(target_entity) {
                    let new_state = if shift_held { !sel.is_selected } else { true };
                    sel.is_selected = new_state;

                    if new_state && *faction == net_client.my_faction {
                        if melee_opt.is_some() {
                            sound_events.send(SoundEffect::MeleeSelect);
                        } else if soldier_opt.is_some() {
                            sound_events.send(SoundEffect::MarineSelect);
                        } else if worker_opt.is_some() {
                            sound_events.send(SoundEffect::WorkerSelect);
                        }
                    }
                }
            }
        }

        // Reset drag state
        selection_state.drag_start_screen = None;
        selection_state.drag_start_world = None;
        selection_state.current_world_pos = None;
        selection_state.is_dragging = false;
    }
}

/// Renders selection rings around selected entities and the active marquee drag box
fn draw_selection_gizmos(
    mut gizmos: Gizmos,
    selection_state: Res<SelectionState>,
    query: Query<(&Transform, &Radius, &Faction, &Selectable)>,
) {
    let friendly_ring_col = Color::srgba(0.22, 0.90, 0.40, 0.95);
    let friendly_outer_col = Color::srgba(0.22, 0.90, 0.40, 0.40);
    let enemy_ring_col = Color::srgba(0.95, 0.30, 0.30, 0.95);
    let enemy_outer_col = Color::srgba(0.95, 0.30, 0.30, 0.40);

    // 1. Draw Selection Rings around selected units
    for (transform, radius, faction, selectable) in &query {
        if selectable.is_selected {
            let center = transform.translation.truncate();
            let r = radius.0 + 5.0;
            let (ring_col, outer_col) = if *faction == Faction::Player1 {
                (friendly_ring_col, friendly_outer_col)
            } else {
                (enemy_ring_col, enemy_outer_col)
            };

            gizmos.circle_2d(center, r, ring_col);
            gizmos.circle_2d(center, r + 2.5, outer_col);
        }
    }

    // 2. Draw Active Marquee Selection Box
    if selection_state.is_dragging {
        if let (Some(start), Some(current)) = (
            selection_state.drag_start_world,
            selection_state.current_world_pos,
        ) {
            let center = (start + current) * 0.5;
            let size = (current - start).abs();
            let box_border_col = Color::srgba(0.25, 0.95, 0.45, 0.90);
            let box_inner_col = Color::srgba(0.25, 0.95, 0.45, 0.35);

            gizmos.rect_2d(center, size, box_border_col);
            gizmos.rect_2d(center, (size - Vec2::splat(2.0)).max(Vec2::ZERO), box_inner_col);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::mouse::MouseButton;
    use bevy::input::ButtonInput;
    use shared::components::Building;

    fn setup_test_app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<NetClient>()
            .init_resource::<FogOfWarGrid>()
            .init_resource::<SelectionState>()
            .add_event::<SoundEffect>();

        let mut window = Window::default();
        window.resolution.set(800.0, 600.0);
        window.set_cursor_position(Some(Vec2::new(400.0, 300.0))); // Centers to world (0, 0)
        app.world_mut().spawn((window, PrimaryWindow));

        app.world_mut().spawn((
            Camera::default(),
            Transform::from_xyz(0.0, 0.0, 0.0),
            OrthographicProjection::default_2d(),
        ));

        app
    }

    #[test]
    fn test_drag_selection_strictly_excludes_buildings() {
        let mut app = setup_test_app();

        let unit_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(-10.0, -10.0, 0.0),
                Radius(14.0),
                Faction::Player1,
                Selectable { is_selected: false },
                Soldier::default(),
            ))
            .id();

        let building_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(-20.0, -20.0, 0.0),
                Radius(50.0),
                Faction::Player1,
                Selectable { is_selected: false },
                Building::new("Base HQ", Vec2::new(96.0, 96.0), 0.0, true),
            ))
            .id();

        app.add_systems(Update, handle_selection_input);

        // Configure active drag marquee box covering (-100, -100) to (0, 0)
        {
            let mut state = app.world_mut().resource_mut::<SelectionState>();
            state.is_dragging = true;
            state.drag_start_world = Some(Vec2::new(-100.0, -100.0));
        }

        // Simulate release of Left mouse button
        {
            let mut mouse_btn = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            mouse_btn.press(MouseButton::Left);
            mouse_btn.clear_just_pressed(MouseButton::Left);
            mouse_btn.release(MouseButton::Left);
        }
        app.update();

        let unit_sel = app.world().get::<Selectable>(unit_entity).unwrap();
        let bldg_sel = app.world().get::<Selectable>(building_entity).unwrap();

        assert!(unit_sel.is_selected, "Friendly unit inside drag box must be selected");
        assert!(!bldg_sel.is_selected, "Building inside drag box must NOT be selected");
    }

    #[test]
    fn test_single_click_selects_building() {
        let mut app = setup_test_app();

        let building_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.0, 0.0),
                Radius(50.0),
                Faction::Player1,
                Selectable { is_selected: false },
                Building::new("Base HQ", Vec2::new(96.0, 96.0), 0.0, true),
            ))
            .id();

        // Not dragging, click directly on the building at (0, 0)
        {
            let mut state = app.world_mut().resource_mut::<SelectionState>();
            state.is_dragging = false;
            state.drag_start_world = Some(Vec2::new(0.0, 0.0));
        }

        {
            let mut mouse_btn = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            mouse_btn.press(MouseButton::Left);
            mouse_btn.release(MouseButton::Left);
        }

        app.add_systems(Update, handle_selection_input);
        app.update();

        let bldg_sel = app.world().get::<Selectable>(building_entity).unwrap();
        assert!(bldg_sel.is_selected, "Direct single click on a building must select it");
    }

    #[test]
    fn test_single_click_prioritizes_unit_over_building_when_both_under_cursor() {
        let mut app = setup_test_app();

        let unit_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.0, 0.0),
                Radius(14.0),
                Faction::Player1,
                Selectable { is_selected: false },
                Soldier::default(),
            ))
            .id();

        let building_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.0, 0.0),
                Radius(50.0),
                Faction::Player1,
                Selectable { is_selected: false },
                Building::new("Base HQ", Vec2::new(96.0, 96.0), 0.0, true),
            ))
            .id();

        {
            let mut state = app.world_mut().resource_mut::<SelectionState>();
            state.is_dragging = false;
            state.drag_start_world = Some(Vec2::new(0.0, 0.0));
        }

        {
            let mut mouse_btn = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            mouse_btn.press(MouseButton::Left);
            mouse_btn.release(MouseButton::Left);
        }

        app.add_systems(Update, handle_selection_input);
        app.update();

        let unit_sel = app.world().get::<Selectable>(unit_entity).unwrap();
        let bldg_sel = app.world().get::<Selectable>(building_entity).unwrap();

        assert!(unit_sel.is_selected, "Direct click on unit should select the unit");
        assert!(!bldg_sel.is_selected, "Unit should take precedence over building when overlapping");
    }
}

