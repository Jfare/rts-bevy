use bevy::prelude::*;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::protocol::ServerMessage;

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;

/// Target snapshot used for disjoint query access in server combat
pub(crate) struct ServerTargetSnapshot {
    pub entity: Entity,
    pub net_id: u32,
    pub pos: Vec2,
    pub radius: f32,
    pub faction: Faction,
    pub room_id: u32,
    pub is_dead: bool,
    pub supply_cost: u32,
}

/// Military combat, aggro, weapon cooldowns, and damage deduction
pub fn server_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    matchmaker: Res<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    mut queries: ParamSet<(
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &Transform,
            &Radius,
            &Health,
            Option<&Unit>,
        )>,
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &mut Transform,
            &MoveSpeed,
            &mut Soldier,
            Option<&Stimpack>,
            Option<&mut MoveTarget>,
        )>,
        Query<(Entity, &mut Health)>,
    )>,
) {
    let dt = time.delta_secs();

    // 1. Snapshot all targets
    let targets: Vec<ServerTargetSnapshot> = queries
        .p0()
        .iter()
        .map(|(e, net, fac, room, tf, rad, hp, unit_opt)| ServerTargetSnapshot {
            entity: e,
            net_id: net.net_id,
            pos: tf.translation.truncate(),
            radius: rad.0,
            faction: *fac,
            room_id: room.0,
            is_dead: hp.is_dead(),
            supply_cost: unit_opt.map(|u| u.supply_cost).unwrap_or(0),
        })
        .collect();

    // 2. Iterate through soldiers and execute aggro, chasing, and weapon firing
    let mut damages_to_apply: Vec<(Entity, u32, f32, Faction, u32, u32, u32)> = Vec::new();

    for (s_entity, attacker_net, attacker_faction, attacker_room, mut attacker_tf, move_speed, mut soldier, stim_opt, move_target_opt) in
        &mut queries.p1()
    {
        let is_room_active = matchmaker.rooms.get(&attacker_room.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active {
            continue;
        }

        soldier.attack_timer += dt;
        soldier.scan_timer += dt;
        let attacker_pos = attacker_tf.translation.truncate();

        let is_attack_move = move_target_opt.as_ref().map(|m| m.is_attack_move).unwrap_or(false);

        // If unit has an active pure ground move order, do NOT auto-acquire enemies or stop to attack:
        if move_target_opt.is_some() && !is_attack_move {
            soldier.target = None;
            soldier.state = SoldierState::MovingToGround;
            continue;
        }

        let target_valid = soldier.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead && t.room_id == attacker_room.0 && attacker_faction.is_hostile_to(&t.faction))
        });

        if let Some(target_snap) = target_valid {
            let target_pos = target_snap.pos;
            let dist = target_pos.distance(attacker_pos);
            let effective_range = soldier.attack_range + target_snap.radius;
            let dir = (target_pos - attacker_pos).normalize_or_zero();

            if dir.length_squared() > 0.001 {
                let angle = dir.y.atan2(dir.x);
                attacker_tf.rotation = Quat::from_rotation_z(angle);
            }

            if dist <= effective_range {
                soldier.state = SoldierState::Attacking;

                if soldier.attack_timer >= soldier.attack_cooldown {
                    soldier.attack_timer = 0.0;
                    damages_to_apply.push((
                        target_snap.entity,
                        target_snap.net_id,
                        soldier.attack_damage,
                        *attacker_faction,
                        attacker_room.0,
                        attacker_net.net_id,
                        target_snap.supply_cost,
                    ));

                    let peers = matchmaker.get_room_peers(attacker_room.0);
                    if !peers.is_empty() {
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                            peer_ids: peers,
                            msg: ServerMessage::ProjectileFired {
                                attacker_net_id: attacker_net.net_id,
                                target_net_id: target_snap.net_id,
                                origin: attacker_pos + dir * 18.0,
                                target_pos,
                                damage: soldier.attack_damage,
                            },
                        });
                    }
                }
            } else if soldier.state != SoldierState::HoldingPosition {
                soldier.state = SoldierState::ChasingTarget;
                let speed_mult = stim_opt
                    .map(|s| if s.is_active { 1.5 } else { 1.0 })
                    .unwrap_or(1.0);
                let stop_dist = (effective_range * 0.90).max(10.0);
                let travel_needed = (dist - stop_dist).max(0.0);
                let step = dir * (move_speed.0 * speed_mult * dt).min(travel_needed);
                attacker_tf.translation.x += step.x;
                attacker_tf.translation.y += step.y;
            }
        } else {
            // Target dead or none: scan for enemies in room
            soldier.target = None;

            let max_scan_range = if is_attack_move {
                soldier.aggro_radius
            } else {
                soldier.attack_range
            };

            let mut closest = None;
            let mut min_d = max_scan_range;
            for t in &targets {
                if t.entity != s_entity && t.room_id == attacker_room.0 && attacker_faction.is_hostile_to(&t.faction) && !t.is_dead {
                    let d = t.pos.distance(attacker_pos);
                    let effective_range = max_scan_range + t.radius;
                    if d <= effective_range && d < min_d {
                        min_d = d;
                        closest = Some(t);
                    }
                }
            }
            if let Some(t) = closest {
                soldier.target = Some(t.entity);
                let dist = attacker_pos.distance(t.pos);
                let effective_range = soldier.attack_range + t.radius;
                let dir = (t.pos - attacker_pos).normalize_or_zero();

                if dir.length_squared() > 0.001 {
                    let angle = dir.y.atan2(dir.x);
                    attacker_tf.rotation = Quat::from_rotation_z(angle);
                }

                if dist <= effective_range {
                    soldier.state = SoldierState::Attacking;
                    if soldier.attack_timer >= soldier.attack_cooldown {
                        soldier.attack_timer = 0.0;
                        damages_to_apply.push((
                            t.entity,
                            t.net_id,
                            soldier.attack_damage,
                            *attacker_faction,
                            attacker_room.0,
                            attacker_net.net_id,
                            t.supply_cost,
                        ));

                        let peers = matchmaker.get_room_peers(attacker_room.0);
                        if !peers.is_empty() {
                            let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                                peer_ids: peers,
                                msg: ServerMessage::ProjectileFired {
                                    attacker_net_id: attacker_net.net_id,
                                    target_net_id: t.net_id,
                                    origin: attacker_pos + dir * 18.0,
                                    target_pos: t.pos,
                                    damage: soldier.attack_damage,
                                },
                            });
                        }
                    }
                } else if soldier.state != SoldierState::HoldingPosition {
                    soldier.state = SoldierState::ChasingTarget;
                    let speed_mult = stim_opt
                        .map(|s| if s.is_active { 1.5 } else { 1.0 })
                        .unwrap_or(1.0);
                    let stop_dist = (effective_range * 0.90).max(10.0);
                    let travel_needed = (dist - stop_dist).max(0.0);
                    let step = dir * (move_speed.0 * speed_mult * dt).min(travel_needed);
                    attacker_tf.translation.x += step.x;
                    attacker_tf.translation.y += step.y;
                }
            } else if is_attack_move {
                soldier.state = SoldierState::AttackMoving;
            } else if soldier.state != SoldierState::HoldingPosition {
                soldier.state = SoldierState::Idle;
            }
        }
    }

    // 3. Apply damages and handle deaths
    let mut health_query = queries.p2();
    for (target_e, target_net, dmg, attacker_fac, attacker_room_id, _attacker_net, supply_cost) in damages_to_apply {
        if let Ok((_, mut hp)) = health_query.get_mut(target_e) {
            hp.take_damage(dmg);

            let peers = matchmaker.get_room_peers(attacker_room_id);
            if !peers.is_empty() {
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                    peer_ids: peers.clone(),
                    msg: ServerMessage::EntityDamaged {
                        target_net_id: target_net,
                        current_hp: hp.current,
                        max_hp: hp.max,
                    },
                });

                if hp.is_dead() {
                    if supply_cost > 0 {
                        let victim_faction = if attacker_fac == Faction::Player1 {
                            Faction::Player2
                        } else {
                            Faction::Player1
                        };
                        economy.unregister_supply(victim_faction, supply_cost);
                    }

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                        peer_ids: peers,
                        msg: ServerMessage::EntityDied {
                            net_id: target_net,
                            faction: attacker_fac,
                        },
                    });

                    commands.entity(target_e).despawn_recursive();
                }
            }
        }
    }
}

/// Dedicated Server Defensive Gun Turret Combat System
pub fn server_turret_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    matchmaker: Res<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    mut queries: ParamSet<(
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &Transform,
            &Radius,
            &Health,
            Option<&Unit>,
        )>,
        Query<(Entity, &NetEntity, &Faction, &RoomId, &Transform, &Building, &mut GunTurret)>,
        Query<(Entity, &mut Health)>,
    )>,
) {
    let dt = time.delta_secs();

    let targets: Vec<ServerTargetSnapshot> = queries
        .p0()
        .iter()
        .map(|(e, net, fac, room, tf, rad, hp, unit_opt)| ServerTargetSnapshot {
            entity: e,
            net_id: net.net_id,
            pos: tf.translation.truncate(),
            radius: rad.0,
            faction: *fac,
            room_id: room.0,
            is_dead: hp.is_dead(),
            supply_cost: unit_opt.map(|u| u.supply_cost).unwrap_or(0),
        })
        .collect();

    let mut damages_to_apply: Vec<(Entity, u32, f32, Faction, u32, u32, u32)> = Vec::new();

    for (_t_entity, turret_net, turret_faction, turret_room, turret_tf, building, mut turret) in
        &mut queries.p1()
    {
        let is_room_active = matchmaker.rooms.get(&turret_room.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active || !building.is_constructed {
            continue;
        }

        turret.attack_timer += dt;
        let turret_pos = turret_tf.translation.truncate();

        let target_valid = turret.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead && t.room_id == turret_room.0 && turret_faction.is_hostile_to(&t.faction) && t.pos.distance(turret_pos) <= (turret.attack_range + t.radius))
        });

        let active_target = match target_valid {
            Some(t) => Some(t),
            None => {
                turret.target = None;
                let mut closest = None;
                let mut min_d = turret.attack_range;
                for t in &targets {
                    if t.room_id == turret_room.0 && turret_faction.is_hostile_to(&t.faction) && !t.is_dead {
                        let d = t.pos.distance(turret_pos);
                        let effective_range = turret.attack_range + t.radius;
                        if d <= effective_range && d < min_d {
                            min_d = d;
                            closest = Some(t);
                        }
                    }
                }
                if let Some(t) = closest {
                    turret.target = Some(t.entity);
                }
                closest
            }
        };

        if let Some(target_snap) = active_target {
            let target_pos = target_snap.pos;
            let dir = (target_pos - turret_pos).normalize_or_zero();
            turret.barrel_angle = dir.y.atan2(dir.x);

            if turret.attack_timer >= turret.attack_cooldown {
                turret.attack_timer = 0.0;
                damages_to_apply.push((
                    target_snap.entity,
                    target_snap.net_id,
                    turret.attack_damage,
                    *turret_faction,
                    turret_room.0,
                    turret_net.net_id,
                    target_snap.supply_cost,
                ));

                let peers = matchmaker.get_room_peers(turret_room.0);
                if !peers.is_empty() {
                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                        peer_ids: peers,
                        msg: ServerMessage::ProjectileFired {
                            attacker_net_id: turret_net.net_id,
                            target_net_id: target_snap.net_id,
                            origin: turret_pos + dir * 26.0,
                            target_pos,
                            damage: turret.attack_damage,
                        },
                    });
                }
            }
        }
    }

    let mut health_query = queries.p2();
    for (target_e, target_net, dmg, attacker_fac, attacker_room_id, _attacker_net, supply_cost) in damages_to_apply {
        if let Ok((_, mut hp)) = health_query.get_mut(target_e) {
            hp.take_damage(dmg);

            let peers = matchmaker.get_room_peers(attacker_room_id);
            if !peers.is_empty() {
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                    peer_ids: peers.clone(),
                    msg: ServerMessage::EntityDamaged {
                        target_net_id: target_net,
                        current_hp: hp.current,
                        max_hp: hp.max,
                    },
                });

                if hp.is_dead() {
                    if supply_cost > 0 {
                        let victim_faction = if attacker_fac == Faction::Player1 {
                            Faction::Player2
                        } else {
                            Faction::Player1
                        };
                        economy.unregister_supply(victim_faction, supply_cost);
                    }

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                        peer_ids: peers,
                        msg: ServerMessage::EntityDied {
                            net_id: target_net,
                            faction: attacker_fac,
                        },
                    });

                    commands.entity(target_e).despawn_recursive();
                }
            }
        }
    }
}

/// Dedicated Server Siege Tank Combat & Artillery System
pub fn server_siege_tank_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    matchmaker: Res<Matchmaker>,
    mut economy: ResMut<PlayerEconomy>,
    mut queries: ParamSet<(
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &Transform,
            &Radius,
            &Health,
            Option<&Unit>,
        )>,
        Query<(
            Entity,
            &NetEntity,
            &Faction,
            &RoomId,
            &mut Transform,
            &MoveSpeed,
            &mut SiegeTank,
            Option<&mut MoveTarget>,
        )>,
        Query<(Entity, &mut Health)>,
    )>,
) {
    let dt = time.delta_secs();

    let targets: Vec<ServerTargetSnapshot> = queries
        .p0()
        .iter()
        .map(|(e, net, fac, room, tf, rad, hp, unit_opt)| ServerTargetSnapshot {
            entity: e,
            net_id: net.net_id,
            pos: tf.translation.truncate(),
            radius: rad.0,
            faction: *fac,
            room_id: room.0,
            is_dead: hp.is_dead(),
            supply_cost: unit_opt.map(|u| u.supply_cost).unwrap_or(0),
        })
        .collect();

    let mut damages_to_apply = Vec::new();

    for (tank_ent, tank_net, tank_faction, tank_room, mut tank_tf, move_speed, mut tank, move_target_opt) in
        &mut queries.p1()
    {
        let is_room_active = matchmaker.rooms.get(&tank_room.0).map(|r| r.is_active && r.countdown_timer <= 0.0).unwrap_or(true);
        if !is_room_active {
            continue;
        }

        tank.attack_timer += dt;
        let tank_pos = tank_tf.translation.truncate();
        let is_siege = tank.mode == TankMode::Siege;
        let is_attack_move = move_target_opt.as_ref().map(|m| m.is_attack_move).unwrap_or(false);

        // If tank is in mobile mode and has a pure ground move order, ignore combat and move!
        if move_target_opt.is_some() && !is_attack_move && tank.mode == TankMode::Tank {
            tank.target = None;
            continue;
        }

        let target_valid = tank.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead && t.room_id == tank_room.0 && tank_faction.is_hostile_to(&t.faction))
        });

        if let Some(target_snap) = target_valid {
            let target_pos = target_snap.pos;
            let dist = target_pos.distance(tank_pos);
            let effective_range = tank.attack_range + target_snap.radius;
            let dir = (target_pos - tank_pos).normalize_or_zero();
            tank.turret_angle = dir.y.atan2(dir.x);

            if dist <= effective_range {
                if tank.attack_timer >= tank.attack_cooldown {
                    tank.attack_timer = 0.0;
                    damages_to_apply.push((
                        target_snap.entity,
                        target_snap.net_id,
                        tank.attack_damage,
                        *tank_faction,
                        tank_room.0,
                        tank_net.net_id,
                        target_snap.supply_cost,
                    ));

                    let peers = matchmaker.get_room_peers(tank_room.0);
                    if !peers.is_empty() {
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                            peer_ids: peers,
                            msg: ServerMessage::ProjectileFired {
                                attacker_net_id: tank_net.net_id,
                                target_net_id: target_snap.net_id,
                                origin: tank_pos + dir * (if is_siege { 36.0 } else { 26.0 }),
                                target_pos,
                                damage: tank.attack_damage,
                            },
                        });
                    }
                }
            } else if tank.mode == TankMode::Tank {
                let stop_dist = (effective_range * 0.90).max(20.0);
                let travel_needed = (dist - stop_dist).max(0.0);
                let step = dir * (move_speed.0 * dt).min(travel_needed);
                tank_tf.translation.x += step.x;
                tank_tf.translation.y += step.y;
                let angle = dir.y.atan2(dir.x);
                tank_tf.rotation = Quat::from_rotation_z(angle);
            } else if is_siege {
                tank.target = None;
            }
        } else {
            // Target dead or none: scan for enemies in room
            tank.target = None;

            let max_scan_range = if is_attack_move {
                (tank.attack_range * 1.25).max(300.0)
            } else {
                tank.attack_range
            };

            let mut closest = None;
            let mut min_d = max_scan_range;
            for t in &targets {
                if t.entity != tank_ent && t.room_id == tank_room.0 && tank_faction.is_hostile_to(&t.faction) && !t.is_dead {
                    let d = t.pos.distance(tank_pos);
                    let effective_range = max_scan_range + t.radius;
                    if d <= effective_range && d < min_d {
                        min_d = d;
                        closest = Some(t);
                    }
                }
            }
            if let Some(t) = closest {
                tank.target = Some(t.entity);
                let dir = (t.pos - tank_pos).normalize_or_zero();
                tank.turret_angle = dir.y.atan2(dir.x);

                if tank.attack_timer >= tank.attack_cooldown {
                    tank.attack_timer = 0.0;
                    damages_to_apply.push((
                        t.entity,
                        t.net_id,
                        tank.attack_damage,
                        *tank_faction,
                        tank_room.0,
                        tank_net.net_id,
                        t.supply_cost,
                    ));

                    let peers = matchmaker.get_room_peers(tank_room.0);
                    if !peers.is_empty() {
                        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                            peer_ids: peers,
                            msg: ServerMessage::ProjectileFired {
                                attacker_net_id: tank_net.net_id,
                                target_net_id: t.net_id,
                                origin: tank_pos + dir * (if is_siege { 36.0 } else { 26.0 }),
                                target_pos: t.pos,
                                damage: tank.attack_damage,
                            },
                        });
                    }
                }
            }
        }
    }

    let mut health_query = queries.p2();
    for (target_e, target_net, dmg, attacker_fac, attacker_room_id, _attacker_net, supply_cost) in damages_to_apply {
        if let Ok((_, mut hp)) = health_query.get_mut(target_e) {
            hp.take_damage(dmg);

            let peers = matchmaker.get_room_peers(attacker_room_id);
            if !peers.is_empty() {
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                    peer_ids: peers.clone(),
                    msg: ServerMessage::EntityDamaged {
                        target_net_id: target_net,
                        current_hp: hp.current,
                        max_hp: hp.max,
                    },
                });

                if hp.is_dead() {
                    if supply_cost > 0 {
                        let victim_faction = if attacker_fac == Faction::Player1 {
                            Faction::Player2
                        } else {
                            Faction::Player1
                        };
                        economy.unregister_supply(victim_faction, supply_cost);
                    }

                    let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                        peer_ids: peers,
                        msg: ServerMessage::EntityDied {
                            net_id: target_net,
                            faction: attacker_fac,
                        },
                    });

                    commands.entity(target_e).despawn_recursive();
                }
            }
        }
    }
}

