use bevy::prelude::*;
use shared::components::{MoveTarget, SoldierState, TacticalStance, WorkerState};
use shared::grid::NavGrid;

use super::{EntityNetQuery, NodeQuery};

pub fn handle_units_ordered_move(
    commands: &mut Commands,
    nav_grid: &NavGrid,
    entity_query: &mut EntityNetQuery,
    unit_net_ids: Vec<u32>,
    destinations: Vec<Vec2>,
    is_attack_move: bool,
) {
    for (net_id, dest) in unit_net_ids.into_iter().zip(destinations) {
        for (entity, net_entity, _fac, tf, _hp, mut worker_opt, soldier_opt, melee_opt, move_target_opt, stance_opt, ..) in
            entity_query.iter_mut()
        {
            if net_entity.net_id == net_id {
                if let Some(ref mut worker) = worker_opt {
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

                let unit_pos = tf.translation.truncate();
                let waypoints = nav_grid.find_path(unit_pos, dest);

                if let Some(mut mt) = move_target_opt {
                    mt.destination = dest;
                    mt.is_attack_move = is_attack_move;
                    mt.waypoints = waypoints;
                    mt.current_waypoint_idx = 0;
                } else {
                    commands
                        .entity(entity)
                        .insert(MoveTarget::with_waypoints(dest, is_attack_move, waypoints));
                }
                break;
            }
        }
    }
}

pub fn handle_units_ordered_attack_target(
    commands: &mut Commands,
    entity_query: &mut EntityNetQuery,
    unit_net_ids: Vec<u32>,
    target_net_id: u32,
) {
    let target_entity = entity_query
        .iter()
        .find(|(_, net_entity, ..)| net_entity.net_id == target_net_id)
        .map(|(e, ..)| e);

    if let Some(target_e) = target_entity {
        for (entity, net_entity, _fac, _tf, _hp, _worker, soldier_opt, melee_opt, ..) in
            entity_query.iter_mut()
        {
            if unit_net_ids.contains(&net_entity.net_id) {
                commands.entity(entity).remove::<MoveTarget>();
                if let Some(mut soldier) = soldier_opt {
                    soldier.target = Some(target_e);
                    soldier.state = SoldierState::ChasingTarget;
                }
                if let Some(mut melee) = melee_opt {
                    melee.target = Some(target_e);
                    melee.state = SoldierState::ChasingTarget;
                }
            }
        }
    }
}

pub fn handle_workers_ordered_harvest(
    commands: &mut Commands,
    node_query: &NodeQuery,
    entity_query: &mut EntityNetQuery,
    worker_net_ids: Vec<u32>,
    resource_net_id: u32,
) {
    let target_node = node_query
        .iter()
        .find(|(_, net_entity, _)| net_entity.net_id == resource_net_id)
        .map(|(e, _, _)| e);

    if let Some(node_e) = target_node {
        for (entity, net_entity, _fac, _tf, _hp, worker_opt, ..) in entity_query.iter_mut() {
            if worker_net_ids.contains(&net_entity.net_id) {
                commands.entity(entity).remove::<MoveTarget>();
                if let Some(mut worker) = worker_opt {
                    worker.target_node = Some(node_e);
                    worker.state = WorkerState::MovingToResource;
                    worker.harvest_timer = 0.0;
                }
            }
        }
    }
}

pub fn handle_units_ordered_stop(
    commands: &mut Commands,
    entity_query: &mut EntityNetQuery,
    unit_net_ids: Vec<u32>,
) {
    for (entity, net_entity, _fac, _tf, _hp, worker_opt, soldier_opt, mut melee_opt, _, stance_opt, ..) in
        entity_query.iter_mut()
    {
        if unit_net_ids.contains(&net_entity.net_id) {
            commands.entity(entity).remove::<MoveTarget>();
            if let Some(mut soldier) = soldier_opt {
                soldier.target = None;
                soldier.state = SoldierState::Idle;
            }
            if let Some(ref mut melee) = melee_opt {
                melee.target = None;
                melee.state = SoldierState::Idle;
            }
            if let Some(mut worker) = worker_opt {
                worker.state = WorkerState::Idle;
            }
            if let Some(mut stance) = stance_opt {
                *stance = TacticalStance::Aggressive;
            }
        }
    }
}

pub fn handle_units_ordered_hold_position(
    commands: &mut Commands,
    entity_query: &mut EntityNetQuery,
    unit_net_ids: Vec<u32>,
) {
    for (entity, net_entity, _fac, _tf, _hp, _worker, soldier_opt, mut melee_opt, _, stance_opt, ..) in
        entity_query.iter_mut()
    {
        if unit_net_ids.contains(&net_entity.net_id) {
            commands.entity(entity).remove::<MoveTarget>();
            if let Some(mut soldier) = soldier_opt {
                soldier.target = None;
                soldier.state = SoldierState::HoldingPosition;
            }
            if let Some(ref mut melee) = melee_opt {
                melee.target = None;
                melee.state = SoldierState::HoldingPosition;
            }
            if let Some(mut stance) = stance_opt {
                *stance = TacticalStance::HoldPosition;
            } else {
                commands.entity(entity).insert(TacticalStance::HoldPosition);
            }
        }
    }
}

pub fn handle_units_ordered_patrol(
    commands: &mut Commands,
    nav_grid: &NavGrid,
    entity_query: &mut EntityNetQuery,
    unit_net_ids: Vec<u32>,
    destinations: Vec<Vec2>,
) {
    for (net_id, dest) in unit_net_ids.into_iter().zip(destinations) {
        for (entity, net_entity, _fac, tf, _hp, _worker, soldier_opt, _melee, move_target_opt, stance_opt, ..) in
            entity_query.iter_mut()
        {
            if net_entity.net_id == net_id {
                let unit_pos = tf.translation.truncate();
                let waypoints = nav_grid.find_path(unit_pos, dest);

                if let Some(mut stance) = stance_opt {
                    *stance = TacticalStance::Patrol {
                        origin: unit_pos,
                        target: dest,
                        heading_to_target: true,
                    };
                } else {
                    commands.entity(entity).insert(TacticalStance::Patrol {
                        origin: unit_pos,
                        target: dest,
                        heading_to_target: true,
                    });
                }

                if let Some(mut soldier) = soldier_opt {
                    soldier.target = None;
                    soldier.state = SoldierState::AttackMoving;
                }

                if let Some(mut mt) = move_target_opt {
                    mt.destination = dest;
                    mt.is_attack_move = true;
                    mt.waypoints = waypoints;
                    mt.current_waypoint_idx = 0;
                } else {
                    commands
                        .entity(entity)
                        .insert(MoveTarget::with_waypoints(dest, true, waypoints));
                }
                break;
            }
        }
    }
}
