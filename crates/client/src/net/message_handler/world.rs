use bevy::prelude::*;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::BuildingKind;
use shared::protocol::{EntityKind, EntityState, GameMode, UnitKind};

use super::{CleanupQuery, EntityNetQuery};

pub fn handle_initial_world_state(
    commands: &mut Commands,
    economy: &mut PlayerEconomy,
    current_mode: GameMode,
    cleanup_query: &CleanupQuery,
    entities: Vec<EntityState>,
    p1_minerals: u32,
    p1_supply: u32,
    p1_max_supply: u32,
    p2_minerals: u32,
    p2_supply: u32,
    p2_max_supply: u32,
) {
    info!(
        "🌍 [NetClient] Initializing authoritative world state from server ({} entities).",
        entities.len()
    );

    // Clear previous match entities
    for ent in cleanup_query.iter() {
        commands.entity(ent).despawn_recursive();
    }

    // Sync starting minerals and supply authoritatively
    economy.set_minerals(Faction::Player1, p1_minerals);
    economy.set_supply(Faction::Player1, p1_supply, p1_max_supply);

    let p2_faction = if current_mode == GameMode::SoloVsAi {
        Faction::HostileAi
    } else {
        Faction::Player2
    };
    economy.set_minerals(p2_faction, p2_minerals);
    economy.set_supply(p2_faction, p2_supply, p2_max_supply);

    // Spawn authoritative entities
    let mut mineral_nodes: Vec<(Entity, Vec2)> = Vec::new();
    let mut pending_units: Vec<(EntityState, UnitKind)> = Vec::new();
    let mut pending_buildings: Vec<(EntityState, BuildingKind)> = Vec::new();

    for ent_state in entities {
        match ent_state.kind {
            EntityKind::ResourceNode => {
                let pos = ent_state.position;
                let node_e = commands
                    .spawn((
                        ResourceNode::new(2000),
                        Faction::Neutral,
                        Selectable::default(),
                        Radius(36.0),
                        NetEntity {
                            net_id: ent_state.net_id,
                            owner_peer_id: 0,
                        },
                        Transform::from_xyz(pos.x, pos.y, 0.5),
                    ))
                    .id();
                mineral_nodes.push((node_e, pos));
            }
            EntityKind::Building(kind) => {
                pending_buildings.push((ent_state, kind));
            }
            EntityKind::Unit(kind) => {
                pending_units.push((ent_state, kind));
            }
        }
    }

    for (ent_state, kind) in pending_buildings {
        let pos = ent_state.position;
        let mut b_cmds = commands.spawn((
            Building::new(kind.name(), kind.size(), kind.build_duration(), true),
            Health::new(ent_state.max_hp),
            ent_state.faction,
            Selectable::default(),
            Radius(kind.size().x.max(kind.size().y) * 0.5),
            NetEntity {
                net_id: ent_state.net_id,
                owner_peer_id: 0,
            },
            Transform::from_xyz(pos.x, pos.y, 1.0),
        ));

        match kind {
            BuildingKind::BaseHQ => {
                b_cmds.insert((
                    BaseHQ {
                        supply_provided: 10,
                        dropoff_radius: 70.0,
                    },
                    ProductionBuilding {
                        queue: Vec::new(),
                        current_timer: 0.0,
                        max_queue_size: 5,
                        rally_point: pos + Vec2::new(0.0, -100.0),
                    },
                ));
            }
            BuildingKind::Barracks => {
                b_cmds.insert((
                    Barracks,
                    ProductionBuilding {
                        queue: Vec::new(),
                        current_timer: 0.0,
                        max_queue_size: 5,
                        rally_point: pos + Vec2::new(0.0, -100.0),
                    },
                ));
            }
            BuildingKind::SupplyDepot => {
                b_cmds.insert(SupplyDepot {
                    supply_provided: 8,
                });
            }
            BuildingKind::Turret => {
                b_cmds.insert(GunTurret::default());
            }
        }
    }

    for (ent_state, kind) in pending_units {
        let pos = ent_state.position;
        let mut u_cmds = commands.spawn((
            Unit {
                name: kind.name().to_string(),
                supply_cost: kind.supply_cost(),
            },
            Health::new(ent_state.max_hp),
            ent_state.faction,
            Selectable::default(),
            NetEntity {
                net_id: ent_state.net_id,
                owner_peer_id: 0,
            },
            Transform::from_xyz(pos.x, pos.y, 2.0)
                .with_rotation(Quat::from_rotation_z(ent_state.rotation)),
        ));

        match kind {
            UnitKind::Worker => {
                let closest_node = mineral_nodes
                    .iter()
                    .filter(|(_, a)| pos.distance(*a) <= WORKER_AUTO_MINE_RANGE)
                    .min_by(|(_, a), (_, b)| {
                        let da = pos.distance(*a);
                        let db = pos.distance(*b);
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(e, _)| *e);

                let state = if closest_node.is_some() {
                    WorkerState::MovingToResource
                } else {
                    WorkerState::Idle
                };

                u_cmds.insert((
                    Worker {
                        state,
                        target_node: closest_node,
                        ..default()
                    },
                    Radius(14.0),
                    MoveSpeed(WORKER_MOVE_SPEED),
                    Velocity::default(),
                ));
            }
            UnitKind::RangedFighter => {
                u_cmds.insert((
                    Soldier {
                        state: SoldierState::Idle,
                        attack_range: 150.0,
                        aggro_radius: 240.0,
                        attack_damage: 15.0,
                        attack_cooldown: 0.85,
                        ..default()
                    },
                    Radius(16.0),
                    MoveSpeed(180.0),
                    Velocity::default(),
                ));
            }
            UnitKind::MeleeFighter => {
                u_cmds.insert((
                    MeleeFighter::default(),
                    Radius(16.0),
                    MoveSpeed(195.0),
                    Velocity::default(),
                ));
            }
        }
    }
}

pub fn handle_building_spawned(
    commands: &mut Commands,
    net_id: u32,
    faction: Faction,
    building_kind: BuildingKind,
    position: Vec2,
    max_hp: f32,
) {
    info!(
        "🏗️ [NetClient] Server spawned building #{}: {:?}",
        net_id, building_kind
    );
    let mut entity_cmds = commands.spawn((
        Building::new(
            building_kind.name(),
            building_kind.size(),
            building_kind.build_duration(),
            false,
        ),
        Health::new(max_hp),
        faction,
        Selectable::default(),
        Radius(building_kind.size().x.max(building_kind.size().y) * 0.5),
        NetEntity {
            net_id,
            owner_peer_id: 0,
        },
        Transform::from_xyz(position.x, position.y, 1.0),
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
                    rally_point: position + Vec2::new(0.0, -100.0),
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
                    rally_point: position + Vec2::new(0.0, -100.0),
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

pub fn handle_unit_spawned(
    commands: &mut Commands,
    net_id: u32,
    faction: Faction,
    unit_kind: UnitKind,
    position: Vec2,
    max_hp: f32,
) {
    info!("🎖️ [NetClient] Server spawned unit #{}: {:?}", net_id, unit_kind);
    let mut unit_cmds = commands.spawn((
        Unit {
            name: unit_kind.name().to_string(),
            supply_cost: unit_kind.supply_cost(),
        },
        Health::new(max_hp),
        faction,
        Selectable::default(),
        NetEntity {
            net_id,
            owner_peer_id: 0,
        },
        Transform::from_xyz(position.x, position.y, 2.0),
    ));

    match unit_kind {
        UnitKind::Worker => {
            unit_cmds.insert((
                Worker::default(),
                TacticalStance::default(),
                Radius(14.0),
                MoveSpeed(WORKER_MOVE_SPEED),
                Velocity::default(),
            ));
        }
        UnitKind::RangedFighter => {
            unit_cmds.insert((
                Soldier {
                    state: SoldierState::Idle,
                    attack_range: 150.0,
                    aggro_radius: 240.0,
                    attack_damage: 15.0,
                    attack_cooldown: 0.85,
                    ..default()
                },
                TacticalStance::default(),
                Radius(16.0),
                MoveSpeed(180.0),
                Velocity::default(),
            ));
        }
        UnitKind::MeleeFighter => {
            unit_cmds.insert((
                MeleeFighter::default(),
                TacticalStance::default(),
                Radius(16.0),
                MoveSpeed(195.0),
                Velocity::default(),
            ));
        }
    }
}

pub fn handle_queue_updated(
    entity_query: &mut EntityNetQuery,
    building_net_id: u32,
    queue_count: usize,
    current_progress: f32,
) {
    for (_e, net_entity, _fac, _tf, _hp, _worker, _soldier, _melee, _move, _stance, _rad, _turret, mut prod_opt) in
        entity_query.iter_mut()
    {
        if net_entity.net_id == building_net_id {
            if let Some(ref mut prod) = prod_opt {
                while prod.queue.len() > queue_count {
                    prod.queue.remove(0);
                }
                if !prod.queue.is_empty() {
                    let total_dur = prod.queue[0].build_duration;
                    prod.current_timer = current_progress * total_dur;
                } else {
                    prod.current_timer = 0.0;
                }
            }
        }
    }
}
