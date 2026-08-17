use bevy::prelude::*;
use bevy::render::camera::OrthographicProjection;
use bevy::window::PrimaryWindow;
use shared::components::*;
use shared::economy::PlayerEconomy;
use crate::audio_sfx::SoundEffect;
use crate::selection::screen_to_world_2d;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchOutcome>()
            .add_systems(
                Update,
                (
                    handle_attack_click_orders,
                    soldier_combat_system,
                    turret_combat_system,
                    siege_tank_combat_system,
                    projectile_movement_and_impact_system,
                    muzzle_flash_system,
                    death_and_elimination_system,
                    draw_combat_gizmos,
                ),
            );
    }
}

/// Contextual right-click handler for direct attack orders on enemy units/buildings
fn handle_attack_click_orders(
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &Transform, Option<&OrthographicProjection>), With<Camera>>,
    hostile_query: Query<(Entity, &Transform, &Radius, &Faction), (Without<Unit>, Without<Camera>)>,
    hostile_units: Query<(Entity, &Transform, &Radius, &Faction), (With<Unit>, Without<Camera>)>,
    mut soldier_query: Query<(&mut Soldier, &Faction, &Selectable)>,
    mut tank_query: Query<(&mut SiegeTank, &Faction, &Selectable)>,
) {
    if !mouse_button.just_pressed(MouseButton::Right) {
        return;
    }

    let Ok((_camera, cam_transform, ortho_opt)) = camera_query.get_single() else {
        return;
    };
    let Ok(window) = window_query.get_single() else {
        return;
    };
    let Some(cursor_screen) = window.cursor_position() else {
        return;
    };

    let win_size = Vec2::new(window.width(), window.height());
    let cam_pos = cam_transform.translation.truncate();
    let cam_scale = ortho_opt.map(|o| o.scale).unwrap_or(1.0);
    let click_pos = screen_to_world_2d(cursor_screen, win_size, cam_pos, cam_scale);

    // 1. Check if clicked an enemy unit
    let mut clicked_enemy = None;
    for (entity, transform, radius, faction) in &hostile_units {
        if *faction != Faction::Player1 {
            let pos = transform.translation.truncate();
            if pos.distance(click_pos) <= (radius.0 + 16.0) {
                clicked_enemy = Some(entity);
                break;
            }
        }
    }

    // 2. Check if clicked an enemy building
    if clicked_enemy.is_none() {
        for (entity, transform, radius, faction) in &hostile_query {
            if *faction != Faction::Player1 {
                let pos = transform.translation.truncate();
                if pos.distance(click_pos) <= (radius.0 + 20.0) {
                    clicked_enemy = Some(entity);
                    break;
                }
            }
        }
    }

    let Some(target_entity) = clicked_enemy else {
        return;
    };

    // Assign focus-fire attack order to all selected friendly combat units
    for (mut soldier, faction, selectable) in &mut soldier_query {
        if *faction == Faction::Player1 && selectable.is_selected {
            soldier.target = Some(target_entity);
            soldier.state = SoldierState::ChasingTarget;
        }
    }
    for (mut tank, faction, selectable) in &mut tank_query {
        if *faction == Faction::Player1 && selectable.is_selected {
            tank.target = Some(target_entity);
        }
    }
}

/// Target snapshot used for disjoint scanning and combat logic
struct TargetSnapshot {
    entity: Entity,
    pos: Vec2,
    radius: f32,
    faction: Faction,
    is_dead: bool,
}

/// Marine Combat State Machine using ParamSet
fn soldier_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    mut sound_events: EventWriter<SoundEffect>,
    mut queries: ParamSet<(
        Query<(Entity, &Transform, &Radius, &Faction, &Health)>,
        Query<(
            Entity,
            &mut Soldier,
            &mut Transform,
            &MoveSpeed,
            &Faction,
            &Radius,
            Option<&mut MoveTarget>,
        )>,
    )>,
) {
    let dt = time.delta_secs();

    // 1. Snapshot all potential targets in the world
    let mut targets = Vec::new();
    for (entity, transform, radius, faction, health) in &queries.p0() {
        targets.push(TargetSnapshot {
            entity,
            pos: transform.translation.truncate(),
            radius: radius.0,
            faction: *faction,
            is_dead: health.is_dead(),
        });
    }

    // 2. Update all soldiers using the snapshot
    for (soldier_entity, mut soldier, mut soldier_transform, move_speed, faction, radius, move_target_opt) in
        &mut queries.p1()
    {
        soldier.attack_timer += dt;
        soldier.scan_timer += dt;
        let soldier_pos = soldier_transform.translation.truncate();

        let current_target_snapshot = soldier.target.and_then(|t_ent| {
            targets
                .iter()
                .find(|t| t.entity == t_ent && !t.is_dead)
        });

        if current_target_snapshot.is_none() {
            soldier.target = None;
            if soldier.state == SoldierState::Attacking || soldier.state == SoldierState::ChasingTarget {
                soldier.state = if move_target_opt.as_ref().map(|m| m.is_attack_move).unwrap_or(false) {
                    SoldierState::AttackMoving
                } else {
                    SoldierState::Idle
                };
            }

            let mut nearest_enemy = None;
            let mut nearest_dist = soldier.aggro_radius;

            for t in &targets {
                if t.entity != soldier_entity && faction.is_hostile_to(&t.faction) && !t.is_dead {
                    let dist = soldier_pos.distance(t.pos);
                    if dist < nearest_dist {
                        nearest_dist = dist;
                        nearest_enemy = Some(t.entity);
                    }
                }
            }

            if let Some(enemy_ent) = nearest_enemy {
                soldier.target = Some(enemy_ent);
            }
        }

        if let Some(target_ent) = soldier.target {
            if let Some(target_snap) = targets.iter().find(|t| t.entity == target_ent && !t.is_dead) {
                let target_pos = target_snap.pos;
                let dist = soldier_pos.distance(target_pos);
                let effective_range = soldier.attack_range + target_snap.radius;

                let dir = (target_pos - soldier_pos).normalize_or_zero();
                if dir.length_squared() > 0.0 {
                    let angle = dir.y.atan2(dir.x);
                    soldier_transform.rotation = Quat::from_rotation_z(angle);
                }

                if dist <= effective_range {
                    soldier.state = SoldierState::Attacking;

                    if soldier.attack_timer >= soldier.attack_cooldown {
                        soldier.attack_timer = 0.0;
                        sound_events.send(SoundEffect::Gunshot);

                        let muzzle_offset = dir * (radius.0 + 8.0);
                        let projectile_start = soldier_pos + muzzle_offset;

                        commands.spawn((
                            Projectile {
                                origin: projectile_start,
                                target_entity: Some(target_ent),
                                target_pos,
                                speed: 780.0,
                                damage: soldier.attack_damage,
                                faction: *faction,
                                lifetime: 0.0,
                                max_lifetime: 0.7,
                            },
                            Transform::from_xyz(projectile_start.x, projectile_start.y, 3.0),
                        ));

                        commands.spawn((
                            MuzzleFlash {
                                lifetime: 0.0,
                                max_lifetime: 0.08,
                                color: Color::srgb(1.0, 0.9, 0.3),
                            },
                            Transform::from_xyz(projectile_start.x, projectile_start.y, 3.5),
                        ));
                    }
                } else {
                    soldier.state = SoldierState::ChasingTarget;
                    let step = dir * move_speed.0 * dt;
                    soldier_transform.translation.x += step.x;
                    soldier_transform.translation.y += step.y;
                }
            }
        }
    }
}

/// Defensive Gun Turret Combat System
fn turret_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    mut sound_events: EventWriter<SoundEffect>,
    mut turret_query: Query<(Entity, &mut GunTurret, &Transform, &Faction, &Building)>,
    target_query: Query<(Entity, &Transform, &Radius, &Faction, &Health)>,
) {
    let dt = time.delta_secs();

    for (turret_ent, mut turret, tf, faction, building) in &mut turret_query {
        if !building.is_constructed {
            continue;
        }
        turret.attack_timer += dt;
        let turret_pos = tf.translation.truncate();

        // Check current target
        let target_valid = turret.target.and_then(|t_ent| {
            if let Ok((ent, target_tf, radius, t_fac, hp)) = target_query.get(t_ent) {
                if !hp.is_dead() && faction.is_hostile_to(t_fac) && target_tf.translation.truncate().distance(turret_pos) <= (turret.attack_range + radius.0) {
                    return Some((ent, target_tf.translation.truncate()));
                }
            }
            None
        });

        let active_target = match target_valid {
            Some(t) => Some(t),
            None => {
                turret.target = None;
                // Acquire nearest enemy
                let mut best = None;
                let mut best_dist = turret.attack_range;
                for (ent, t_tf, radius, t_fac, hp) in &target_query {
                    if ent != turret_ent && faction.is_hostile_to(t_fac) && !hp.is_dead() {
                        let d = t_tf.translation.truncate().distance(turret_pos);
                        if d <= (turret.attack_range + radius.0) && d < best_dist {
                            best_dist = d;
                            best = Some((ent, t_tf.translation.truncate()));
                        }
                    }
                }
                if let Some((best_ent, _)) = best {
                    turret.target = Some(best_ent);
                }
                best
            }
        };

        if let Some((target_ent, target_pos)) = active_target {
            let dir = (target_pos - turret_pos).normalize_or_zero();
            turret.barrel_angle = dir.y.atan2(dir.x);

            if turret.attack_timer >= turret.attack_cooldown {
                turret.attack_timer = 0.0;
                sound_events.send(SoundEffect::Gunshot);

                let muzzle_start = turret_pos + dir * 28.0;
                commands.spawn((
                    Projectile {
                        origin: muzzle_start,
                        target_entity: Some(target_ent),
                        target_pos,
                        speed: 850.0,
                        damage: turret.attack_damage,
                        faction: *faction,
                        lifetime: 0.0,
                        max_lifetime: 0.6,
                    },
                    Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.0),
                ));

                commands.spawn((
                    MuzzleFlash {
                        lifetime: 0.0,
                        max_lifetime: 0.08,
                        color: Color::srgb(1.0, 0.95, 0.4),
                    },
                    Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.5),
                ));
            }
        }
    }
}

/// Siege Tank Combat System
fn siege_tank_combat_system(
    mut commands: Commands,
    time: Res<Time>,
    mut sound_events: EventWriter<SoundEffect>,
    mut tank_query: Query<(Entity, &mut SiegeTank, &mut Transform, &MoveSpeed, &Faction, &Radius, Option<&mut MoveTarget>)>,
    target_query: Query<(Entity, &Transform, &Radius, &Faction, &Health), Without<SiegeTank>>,
) {
    let dt = time.delta_secs();

    for (tank_ent, mut tank, mut tank_tf, move_speed, faction, radius, move_target_opt) in &mut tank_query {
        tank.attack_timer += dt;
        let tank_pos = tank_tf.translation.truncate();

        let current_target = tank.target.and_then(|t_ent| {
            if let Ok((ent, target_tf, t_rad, t_fac, hp)) = target_query.get(t_ent) {
                if !hp.is_dead() && faction.is_hostile_to(t_fac) {
                    return Some((ent, target_tf.translation.truncate(), t_rad.0));
                }
            }
            None
        });

        let active_target = match current_target {
            Some(t) => Some(t),
            None => {
                tank.target = None;
                let mut best = None;
                let mut best_dist = tank.attack_range + 100.0;
                for (ent, t_tf, t_rad, t_fac, hp) in &target_query {
                    if ent != tank_ent && faction.is_hostile_to(t_fac) && !hp.is_dead() {
                        let d = t_tf.translation.truncate().distance(tank_pos);
                        if d < best_dist {
                            best_dist = d;
                            best = Some((ent, t_tf.translation.truncate(), t_rad.0));
                        }
                    }
                }
                if let Some((best_ent, _, _)) = best {
                    tank.target = Some(best_ent);
                }
                best
            }
        };

        if let Some((target_ent, target_pos, target_rad)) = active_target {
            let dir = (target_pos - tank_pos).normalize_or_zero();
            tank.turret_angle = dir.y.atan2(dir.x);

            let dist = tank_pos.distance(target_pos);
            let effective_range = tank.attack_range + target_rad;

            if dist <= effective_range {
                if tank.attack_timer >= tank.attack_cooldown {
                    tank.attack_timer = 0.0;
                    sound_events.send(SoundEffect::Gunshot);

                    let muzzle_start = tank_pos + dir * (radius.0 * 1.5);
                    commands.spawn((
                        Projectile {
                            origin: muzzle_start,
                            target_entity: Some(target_ent),
                            target_pos,
                            speed: 680.0,
                            damage: tank.attack_damage,
                            faction: *faction,
                            lifetime: 0.0,
                            max_lifetime: 0.8,
                        },
                        Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.0),
                    ));

                    commands.spawn((
                        MuzzleFlash {
                            lifetime: 0.0,
                            max_lifetime: 0.12,
                            color: Color::srgb(1.0, 0.7, 0.2),
                        },
                        Transform::from_xyz(muzzle_start.x, muzzle_start.y, 3.5),
                    ));
                }
            } else if move_target_opt.is_none() {
                // Advance into firing range
                let step = dir * move_speed.0 * dt;
                tank_tf.translation.x += step.x;
                tank_tf.translation.y += step.y;
            }
        }
    }
}

/// Moves flying tracer projectiles and applies damage on impact
fn projectile_movement_and_impact_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Projectile, &mut Transform)>,
    mut health_query: Query<&mut Health, Without<Projectile>>,
) {
    let dt = time.delta_secs();

    for (proj_entity, mut projectile, mut transform) in &mut projectile_query {
        projectile.lifetime += dt;
        if projectile.lifetime >= projectile.max_lifetime {
            commands.entity(proj_entity).despawn();
            continue;
        }

        let current_pos = transform.translation.truncate();
        let target_pos = projectile.target_pos;
        let dist = current_pos.distance(target_pos);
        let step_dist = projectile.speed * dt;

        if dist <= step_dist || dist <= 14.0 {
            if let Some(target_ent) = projectile.target_entity {
                if let Ok(mut health) = health_query.get_mut(target_ent) {
                    health.take_damage(projectile.damage);
                }
            }
            commands.entity(proj_entity).despawn();
        } else {
            let dir = (target_pos - current_pos).normalize_or_zero();
            transform.translation.x += dir.x * step_dist;
            transform.translation.y += dir.y * step_dist;
        }
    }
}

/// Updates timers for muzzle flash effects and cleans them up
fn muzzle_flash_system(
    mut commands: Commands,
    time: Res<Time>,
    mut flash_query: Query<(Entity, &mut MuzzleFlash)>,
) {
    let dt = time.delta_secs();
    for (entity, mut flash) in &mut flash_query {
        flash.lifetime += dt;
        if flash.lifetime >= flash.max_lifetime {
            commands.entity(entity).despawn();
        }
    }
}

/// Eliminates entities when health hits 0 and evaluates Victory/Defeat
fn death_and_elimination_system(
    mut commands: Commands,
    mut outcome: ResMut<MatchOutcome>,
    mut economy: ResMut<PlayerEconomy>,
    mut sound_events: EventWriter<SoundEffect>,
    query: Query<(
        Entity,
        &Health,
        &Faction,
        Option<&Unit>,
        Option<&BaseHQ>,
    )>,
) {
    for (entity, health, faction, unit_opt, base_hq_opt) in &query {
        if health.is_dead() {
            if let Some(unit) = unit_opt {
                economy.unregister_supply(*faction, unit.supply_cost);
                info!("💀 [{:?}] {} destroyed!", faction, unit.name);
            }

            if base_hq_opt.is_some() {
                if *faction == Faction::HostileAi {
                    *outcome = MatchOutcome::Victory;
                    sound_events.send(SoundEffect::Victory);
                    info!("🏆 [MATCH RESULT] VICTORY! Hostile Base HQ destroyed!");
                } else if *faction == Faction::Player1 {
                    *outcome = MatchOutcome::Defeat;
                    sound_events.send(SoundEffect::Defeat);
                    info!("💥 [MATCH RESULT] DEFEAT! Player Base HQ destroyed!");
                }
            }

            commands.entity(entity).despawn_recursive();
        }
    }
}

/// Renders tracer bullets, muzzle flashes, and combat targeting indicators
fn draw_combat_gizmos(
    mut gizmos: Gizmos,
    projectile_query: Query<(&Transform, &Projectile), With<Projectile>>,
    flash_query: Query<(&Transform, &MuzzleFlash), With<MuzzleFlash>>,
    soldier_query: Query<(&Soldier, &Selectable), (With<Soldier>, Without<Projectile>, Without<MuzzleFlash>)>,
    transform_query: Query<&Transform, (Without<Projectile>, Without<MuzzleFlash>)>,
) {
    for (transform, projectile) in &projectile_query {
        let pos = transform.translation.truncate();
        let dir = (projectile.target_pos - projectile.origin).normalize_or_zero();
        let tracer_tail = pos - dir * 14.0;

        let bullet_col = Color::srgb(1.0, 0.85, 0.35);
        let glow_col = Color::srgba(1.0, 0.50, 0.15, 0.6);

        gizmos.line_2d(tracer_tail, pos, bullet_col);
        gizmos.circle_2d(pos, 3.0, bullet_col);
        gizmos.circle_2d(pos, 5.5, glow_col);
    }

    for (transform, flash) in &flash_query {
        let pos = transform.translation.truncate();
        gizmos.circle_2d(pos, 6.0, flash.color);
        gizmos.circle_2d(pos, 9.0, Color::srgba(1.0, 0.5, 0.1, 0.4));
    }

    for (soldier, selectable) in &soldier_query {
        if selectable.is_selected {
            if let Some(target_ent) = soldier.target {
                if let Ok(target_trans) = transform_query.get(target_ent) {
                    let t_pos = target_trans.translation.truncate();
                    let reticle_col = Color::srgba(1.0, 0.30, 0.20, 0.75);
                    gizmos.circle_2d(t_pos, 22.0, reticle_col);
                    gizmos.line_2d(t_pos + Vec2::new(-28.0, 0.0), t_pos + Vec2::new(28.0, 0.0), reticle_col);
                    gizmos.line_2d(t_pos + Vec2::new(0.0, -28.0), t_pos + Vec2::new(0.0, 28.0), reticle_col);
                }
            }
        }
    }
}
