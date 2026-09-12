use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::{
    AppState, Building, Faction, MatchOutcome, MeleeFighter, MoveTarget, NetEntity,
    Radius, ResourceNode, Selectable, Soldier, SoldierState, TacticalStance, Worker, WorkerState,
};
use shared::grid::{NavGrid, WorldGridConfig};
use shared::protocol::ClientMessage;

use crate::audio_sfx::SoundEffect;
use crate::camera::RtsCamera;
use crate::command_marker::CommandMarker;
use crate::fog_of_war::{FogOfWarGrid, FogState};
use crate::minimap::{get_minimap_screen_rect, MinimapState};
use crate::net::{NetClient, NetStatus};
use crate::selection::{screen_to_world_2d, SelectionState};
use crate::stats::MatchStats;
use crate::ui::AttackMovePending;
use super::{
    dispatch_harvest_order, is_mobile_control_scheme, BoxSelectMode, DoubleTapTracker,
};

/// Tracks active touch gestures for tap vs drag discrimination
#[derive(Resource, Default, Debug)]
pub struct TouchGestureState {
    pub touch_start_pos: Option<Vec2>,
    pub touch_start_time: f32,
    pub max_displacement: f32,
    pub is_multi_touch: bool,
}

pub struct MobileControlsPlugin;

impl Plugin for MobileControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchGestureState>()
            .add_systems(
                Update,
                (
                    mobile_camera_pan_system,
                    mobile_pinch_zoom_system,
                    mobile_touch_interaction_system,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_mobile_control_scheme),
            );
    }
}

/// 1-finger touch pan: Smoothly drags camera when touching empty battlefield
fn mobile_camera_pan_system(
    touches: Res<Touches>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    grid_config: Option<Res<WorldGridConfig>>,
    minimap_opt: Option<Res<MinimapState>>,
    box_select: Res<BoxSelectMode>,
    mut camera_query: Query<(&mut Transform, Option<&OrthographicProjection>), With<Camera2d>>,
) {
    // If box selection mode is active or multi-touch (pinch), do not pan camera
    if box_select.0 || touches.iter().count() != 1 {
        return;
    }

    let Ok(window) = window_query.get_single() else {
        return;
    };
    let Ok((mut transform, ortho_opt)) = camera_query.get_single_mut() else {
        return;
    };

    let Some(touch) = touches.iter().next() else {
        return;
    };

    let pos = touch.position();

    // Ignore touches over the minimap
    if let Some(ref mm) = minimap_opt {
        let mm_rect = get_minimap_screen_rect(window, mm);
        if pos.x >= mm_rect.min.x && pos.x <= mm_rect.max.x && pos.y >= mm_rect.min.y && pos.y <= mm_rect.max.y {
            return;
        }
    }

    // Ignore touches at the bottom HUD area or top resource bar
    if pos.y >= window.height() - 120.0 || pos.y <= 50.0 {
        return;
    }

    // Ignore touches at the left thumb quick action bar
    if pos.x <= 160.0 && pos.y >= window.height() - 360.0 {
        return;
    }

    let delta = touch.delta();
    if delta.length_squared() > 0.0 {
        let scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);
        // Dragging finger right moves world right (camera moves left)
        transform.translation.x -= delta.x * scale;
        // Screen Y is Down, World Y is Up -> dragging down moves camera down
        transform.translation.y += delta.y * scale;

        let default_config = WorldGridConfig::default();
        let config = grid_config.as_deref().unwrap_or(&default_config);
        let padding = 100.0;
        transform.translation.x = transform
            .translation
            .x
            .clamp(config.min_bounds.x + padding, config.max_bounds.x - padding);
        transform.translation.y = transform
            .translation
            .y
            .clamp(config.min_bounds.y + padding, config.max_bounds.y - padding);
    }
}

/// 2-finger touch pinch zoom: Smoothly adjusts orthographic camera target zoom
fn mobile_pinch_zoom_system(
    touches: Res<Touches>,
    mut camera_query: Query<(&mut RtsCamera, Option<&mut OrthographicProjection>)>,
) {
    if touches.iter().count() != 2 {
        return;
    }

    let mut iter = touches.iter();
    let Some(t0) = iter.next() else { return; };
    let Some(t1) = iter.next() else { return; };

    let current_dist = t0.position().distance(t1.position());
    let prev_dist = t0.previous_position().distance(t1.previous_position());
    let delta_dist = current_dist - prev_dist;

    if delta_dist.abs() > 0.3 {
        let Ok((mut rts_cam, _)) = camera_query.get_single_mut() else {
            return;
        };
        // Pinch outward (delta > 0) zooms in (decreases ortho scale)
        let zoom_change = -delta_dist * 0.005;
        rts_cam.target_zoom = (rts_cam.target_zoom + zoom_change).clamp(rts_cam.min_zoom, rts_cam.max_zoom);
    }
}

/// Bundled ECS parameters for mobile touch interaction to satisfy system parameter limits
#[derive(SystemParam)]
pub struct MobileTouchParams<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub time: Res<'w, Time>,
    pub touches: Res<'w, Touches>,
    pub grid_cfg: Option<Res<'w, WorldGridConfig>>,
    pub fog: Res<'w, FogOfWarGrid>,
    pub net_client: Res<'w, NetClient>,
    pub outcome_opt: Option<Res<'w, MatchOutcome>>,
    pub stats: ResMut<'w, MatchStats>,
    pub attack_move_pending: ResMut<'w, AttackMovePending>,
    pub gesture_state: ResMut<'w, TouchGestureState>,
    pub double_tap: ResMut<'w, DoubleTapTracker>,
    pub box_select: ResMut<'w, BoxSelectMode>,
    pub selection_state: ResMut<'w, SelectionState>,
    pub sound_events: EventWriter<'w, SoundEffect>,
    pub nav_grid: Res<'w, NavGrid>,
    pub minimap_opt: Option<Res<'w, MinimapState>>,
}

/// Touch tap interaction: Selects entities or issues contextual Move/Attack/Harvest orders
fn mobile_touch_interaction_system(
    p: MobileTouchParams,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>), With<Camera2d>>,
    node_query: Query<(Entity, &Transform, &Radius, &ResourceNode, Option<&NetEntity>), With<ResourceNode>>,
    hostile_query: Query<(Entity, &Transform, &Radius, &Faction, Option<&NetEntity>), (Without<Camera>, Without<ResourceNode>)>,
    mut selectable_query: Query<(
        Entity,
        &Transform,
        &Radius,
        &Faction,
        &mut Selectable,
        Option<&NetEntity>,
        Option<&mut MoveTarget>,
        Option<&mut TacticalStance>,
        Option<&mut Worker>,
        Option<&mut Soldier>,
        Option<&mut MeleeFighter>,
        Option<&Building>,
    )>,
) {
    let MobileTouchParams {
        mut commands,
        time,
        touches,
        grid_cfg,
        fog,
        net_client,
        outcome_opt,
        mut stats,
        mut attack_move_pending,
        mut gesture_state,
        mut double_tap,
        mut box_select,
        mut selection_state,
        mut sound_events,
        nav_grid,
        minimap_opt,
    } = p;
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory)
        || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat)
    {
        return;
    }

    let Ok(window) = window_query.get_single() else { return; };
    let Ok((_, cam_tf, ortho_opt)) = camera_query.get_single() else { return; };

    let win_size = Vec2::new(window.width(), window.height());
    let cam_pos = cam_tf.translation.truncate();
    let cam_scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);

    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);

    // Multi-touch tracking: if 2+ fingers touch, cancel tap tracking
    if touches.iter().count() > 1 {
        gesture_state.is_multi_touch = true;
        if box_select.0 {
            selection_state.is_dragging = false;
        }
        return;
    }

    // 1. Touch Pressed
    if touches.any_just_pressed() {
        if let Some(touch) = touches.iter().next() {
            let pos = touch.position();

            // Check if touch is in minimap or UI bar
            let is_minimap = minimap_opt.as_ref().map_or(false, |mm| {
                let rect = get_minimap_screen_rect(window, mm);
                pos.x >= rect.min.x && pos.x <= rect.max.x && pos.y >= rect.min.y && pos.y <= rect.max.y
            });
            let is_hud = pos.y >= window.height() - 110.0 || pos.y <= 50.0;
            let is_quick_bar = pos.x <= 160.0 && pos.y >= window.height() - 360.0;

            if !is_minimap && !is_hud && !is_quick_bar {
                gesture_state.touch_start_pos = Some(pos);
                gesture_state.touch_start_time = time.elapsed_secs();
                gesture_state.max_displacement = 0.0;
                gesture_state.is_multi_touch = false;

                if box_select.0 {
                    let world_pos = screen_to_world_2d(pos, win_size, cam_pos, cam_scale);
                    selection_state.drag_start_screen = Some(pos);
                    selection_state.drag_start_world = Some(world_pos);
                    selection_state.current_world_pos = Some(world_pos);
                    selection_state.is_dragging = false;
                }
            }
        }
    }

    // 2. Touch Active / Moved
    if let Some(start_pos) = gesture_state.touch_start_pos {
        if let Some(touch) = touches.iter().next() {
            let current_pos = touch.position();
            let dist = start_pos.distance(current_pos);
            if dist > gesture_state.max_displacement {
                gesture_state.max_displacement = dist;
            }

            if box_select.0 && dist > 10.0 {
                let world_pos = screen_to_world_2d(current_pos, win_size, cam_pos, cam_scale);
                selection_state.is_dragging = true;
                selection_state.current_world_pos = Some(world_pos);
            }
        }
    }

    // 3. Touch Released
    if touches.any_just_released() {
        let Some(start_pos) = gesture_state.touch_start_pos.take() else {
            return;
        };

        // If Box Select was dragging, commit the marquee selection box
        if box_select.0 && selection_state.is_dragging {
            if let Some(start_world) = selection_state.drag_start_world.take() {
                let current_world = selection_state.current_world_pos.unwrap_or(start_world);
                let min_x = start_world.x.min(current_world.x);
                let max_x = start_world.x.max(current_world.x);
                let min_y = start_world.y.min(current_world.y);
                let max_y = start_world.y.max(current_world.y);

                let mut friendly_selected = false;
                for (_, tf, _, faction, mut sel, .., bldg_opt) in &mut selectable_query {
                    if bldg_opt.is_some() {
                        continue;
                    }
                    let p = tf.translation.truncate();
                    if p.x >= min_x && p.x <= max_x && p.y >= min_y && p.y <= max_y && *faction == net_client.my_faction {
                        sel.is_selected = true;
                        friendly_selected = true;
                    }
                }

                if friendly_selected {
                    stats.record_action();
                    sound_events.send(SoundEffect::MarineSelect);
                }
            }
            selection_state.is_dragging = false;
            box_select.0 = false; // Turn off box select after release
            return;
        }

        // Check if this was a stationary tap (< 14px movement and not multi-touch)
        let is_tap = gesture_state.max_displacement < 14.0 && !gesture_state.is_multi_touch;
        if !is_tap {
            return;
        }

        let tap_world_pos = screen_to_world_2d(start_pos, win_size, cam_pos, cam_scale);

        // Check if any friendly units are currently selected
        let mut currently_selected_friendly_count = 0;
        let mut has_selected_workers = false;
        for (_, _, _, faction, sel, _, _, _, worker_opt, _, _, bldg_opt) in &selectable_query {
            if *faction == net_client.my_faction && sel.is_selected {
                if bldg_opt.is_none() {
                    currently_selected_friendly_count += 1;
                    if worker_opt.is_some() {
                        has_selected_workers = true;
                    }
                }
            }
        }

        // ── STEP A: Check if tapped a friendly unit/building ──
        let mut tapped_friendly_unit = None;
        let mut tapped_friendly_dist = f32::MAX;
        let mut tapped_friendly_bldg = None;

        for (entity, tf, radius, faction, _, _, _, _, worker_opt, soldier_opt, _melee_opt, bldg_opt) in &selectable_query {
            if *faction == net_client.my_faction {
                let pos = tf.translation.truncate();
                let dist = pos.distance(tap_world_pos);
                // Generous 20.0px touch tolerance for fingers
                let touch_radius = radius.0 + 20.0;
                if dist <= touch_radius {
                    if bldg_opt.is_some() {
                        if tapped_friendly_bldg.is_none() {
                            tapped_friendly_bldg = Some(entity);
                        }
                    } else if dist < tapped_friendly_dist {
                        tapped_friendly_dist = dist;
                        let kind = if worker_opt.is_some() {
                            1
                        } else if soldier_opt.is_some() {
                            2
                        } else {
                            3
                        };
                        tapped_friendly_unit = Some((entity, kind));
                    }
                }
            }
        }

        if let Some((target_entity, kind)) = tapped_friendly_unit {
            // Check for Double-Tap: select all of the same kind
            let now = time.elapsed_secs();
            let is_double_tap = double_tap.last_entity == Some(target_entity)
                && (now - double_tap.last_tap_time) < 0.35
                && double_tap.last_tap_pos.distance(start_pos) < 24.0;

            double_tap.last_entity = Some(target_entity);
            double_tap.last_tap_time = now;
            double_tap.last_tap_pos = start_pos;

            if is_double_tap {
                // Double tap: select all of same kind on screen
                for (_, _tf, _, faction, mut sel, _, _, _, worker_opt, soldier_opt, melee_opt, bldg_opt) in &mut selectable_query {
                    if *faction == net_client.my_faction && bldg_opt.is_none() {
                        let matches = match kind {
                            1 => worker_opt.is_some(),
                            2 => soldier_opt.is_some(),
                            _ => melee_opt.is_some(),
                        };
                        if matches {
                            sel.is_selected = true;
                        }
                    }
                }
                stats.record_action();
                sound_events.send(SoundEffect::MarineSelect);
                info!("📱 [Touch] Double-tap: Selected all units of type {}", kind);
                return;
            }

            // Single tap: select this unit
            for (entity, _, _, _, mut sel, ..) in &mut selectable_query {
                sel.is_selected = entity == target_entity;
            }
            stats.record_action();
            if kind == 1 {
                sound_events.send(SoundEffect::WorkerSelect);
            } else if kind == 2 {
                sound_events.send(SoundEffect::MarineSelect);
            } else {
                sound_events.send(SoundEffect::MeleeSelect);
            }
            return;
        }

        if let Some(bldg_entity) = tapped_friendly_bldg {
            for (entity, _, _, _, mut sel, ..) in &mut selectable_query {
                sel.is_selected = entity == bldg_entity;
            }
            stats.record_action();
            sound_events.send(SoundEffect::MarineSelect);
            return;
        }

        // ── STEP B: If units are already selected, check context orders ──
        if currently_selected_friendly_count > 0 {
            // 1. Check if tapped a visible hostile unit/building -> Attack Order
            let mut clicked_hostile = None;
            for (h_ent, h_tf, h_radius, h_fac, h_net_opt) in &hostile_query {
                if h_fac.is_hostile_to(&net_client.my_faction) {
                    let pos = h_tf.translation.truncate();
                    let is_visible = fog.get_state_at_world_pos(pos, config) == FogState::Visible;
                    if is_visible && pos.distance(tap_world_pos) <= (h_radius.0 + 22.0) {
                        clicked_hostile = Some((h_ent, h_net_opt.map(|n| n.net_id), pos));
                        break;
                    }
                }
            }

            if let Some((target_ent, target_net, target_pos)) = clicked_hostile {
                let mut unit_items = Vec::new();
                let mut selected_net_ids = Vec::new();
                for (ent, _, _, fac, sel, net_opt, .., bldg_opt) in &mut selectable_query {
                    if *fac == net_client.my_faction && sel.is_selected && bldg_opt.is_none() {
                        if let Some(net) = net_opt {
                            selected_net_ids.push(net.net_id);
                        }
                        unit_items.push(ent);
                    }
                }

                if !unit_items.is_empty() {
                    for ent in unit_items {
                        commands.entity(ent).remove::<MoveTarget>();
                        if let Ok((_, _, _, _, _, _, _, _, mut worker_opt, mut soldier_opt, mut melee_opt, _)) = selectable_query.get_mut(ent) {
                            if let Some(ref mut worker) = worker_opt {
                                worker.state = WorkerState::Idle;
                                worker.target_node = None;
                            }
                            if let Some(ref mut soldier) = soldier_opt {
                                soldier.target = Some(target_ent);
                                soldier.state = SoldierState::ChasingTarget;
                            }
                            if let Some(ref mut melee) = melee_opt {
                                melee.target = Some(target_ent);
                                melee.state = SoldierState::ChasingTarget;
                            }
                        }
                    }

                    stats.record_action();
                    sound_events.send(SoundEffect::OrderIssued);

                    commands.spawn((
                        CommandMarker {
                            lifetime: 0.0,
                            max_lifetime: 0.45,
                            initial_radius: 22.0,
                            color: Color::srgba(0.95, 0.25, 0.25, 0.95), // Red
                        },
                        Transform::from_xyz(target_pos.x, target_pos.y, 1.0),
                    ));

                    if net_client.status != NetStatus::Disconnected && !selected_net_ids.is_empty() {
                        if let Some(t_net) = target_net {
                            net_client.send(&ClientMessage::RequestAttackTarget {
                                unit_net_ids: selected_net_ids,
                                target_net_id: t_net,
                            });
                        }
                    }
                    return;
                }
            }

            // 2. Check if tapped an active mineral node with worker(s) selected -> Harvest Order
            if has_selected_workers {
                let mut tapped_node = None;
                for (node_ent, node_tf, node_rad, node_res, node_net_opt) in &node_query {
                    if node_res.remaining_minerals > 0 {
                        let pos = node_tf.translation.truncate();
                        if pos.distance(tap_world_pos) <= (node_rad.0 + 24.0) {
                            tapped_node = Some((node_ent, pos, node_net_opt.map(|n| n.net_id)));
                            break;
                        }
                    }
                }

                if let Some((node_ent, node_pos, node_net_id_opt)) = tapped_node {
                    dispatch_harvest_order(&mut commands, node_pos, &net_client, &mut stats, &mut sound_events);

                    let mut worker_net_ids = Vec::new();
                    for (ent, _, _, fac, sel, net_opt, _, _, ref mut worker_opt, ..) in &mut selectable_query {
                        if *fac == net_client.my_faction && sel.is_selected {
                            if let Some(ref mut worker) = worker_opt {
                                if let Some(net) = net_opt {
                                    worker_net_ids.push(net.net_id);
                                }
                                commands.entity(ent).remove::<MoveTarget>();
                                worker.target_node = Some(node_ent);
                                worker.state = WorkerState::MovingToResource;
                            }
                        }
                    }

                    if !worker_net_ids.is_empty() && net_client.status != NetStatus::Disconnected {
                        if let Some(res_net_id) = node_net_id_opt {
                            net_client.send(&ClientMessage::RequestHarvest {
                                worker_net_ids,
                                resource_net_id: res_net_id,
                            });
                        }
                    }
                    return;
                }
            }

            // 3. Tapped empty battlefield ground -> Move / Attack-Move Order
            let is_attack_move = attack_move_pending.0;
            attack_move_pending.0 = false;

            let mut selected_units = Vec::new();
            let mut selected_net_ids = Vec::new();

            for (ent, tf, _, fac, sel, net_opt, _, _, _, _, _, bldg_opt) in &selectable_query {
                if *fac == net_client.my_faction && sel.is_selected && bldg_opt.is_none() {
                    selected_units.push((ent, tf.translation.truncate()));
                    if let Some(net) = net_opt {
                        selected_net_ids.push(net.net_id);
                    }
                }
            }

            if !selected_units.is_empty() {
                sound_events.send(SoundEffect::OrderIssued);
                stats.record_action();

                if net_client.status != NetStatus::Disconnected && !selected_net_ids.is_empty() {
                    net_client.send(&ClientMessage::RequestMove {
                        unit_net_ids: selected_net_ids,
                        target_position: tap_world_pos,
                        is_attack_move,
                    });
                }

                let unit_count = selected_units.len();
                for (i, (entity, unit_pos)) in selected_units.iter().enumerate() {
                    let formation_offset = if unit_count > 1 {
                        let angle = (i as f32) * 2.39996;
                        let dist = 24.0 * (i as f32).sqrt();
                        Vec2::new(angle.cos(), angle.sin()) * dist
                    } else {
                        Vec2::ZERO
                    };

                    let destination = tap_world_pos + formation_offset;
                    let waypoints = nav_grid.find_path(*unit_pos, destination);

                    if let Ok((_, _, _, _, _, _, move_target_opt, stance_opt, worker_opt, soldier_opt, melee_opt, _)) =
                        selectable_query.get_mut(*entity)
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
                        if let Some(mut existing) = move_target_opt {
                            existing.destination = destination;
                            existing.is_attack_move = is_attack_move;
                            existing.waypoints = waypoints;
                            existing.current_waypoint_idx = 0;
                        } else {
                            commands.entity(*entity).insert(MoveTarget::with_waypoints(
                                destination,
                                is_attack_move,
                                waypoints,
                            ));
                        }
                    }
                }

                let marker_color = if is_attack_move {
                    Color::srgba(0.95, 0.45, 0.20, 0.95)
                } else {
                    Color::srgba(0.25, 0.95, 0.45, 0.95)
                };

                commands.spawn((
                    CommandMarker {
                        lifetime: 0.0,
                        max_lifetime: 0.45,
                        initial_radius: 20.0,
                        color: marker_color,
                    },
                    Transform::from_xyz(tap_world_pos.x, tap_world_pos.y, 1.0),
                ));
                return;
            }
        }

        // ── STEP C: Tapped empty ground with nothing selected -> clear selection ──
        for (_, _, _, _, mut sel, ..) in &mut selectable_query {
            sel.is_selected = false;
        }
    }
}
