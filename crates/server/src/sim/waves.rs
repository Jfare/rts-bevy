use bevy::prelude::*;
use shared::components::*;
use shared::grid::NavGrid;
use shared::protocol::{GameMode, ServerMessage, UnitKind};

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;

/// Spawns escalating squads of hostile marines for active SoloVsAi matches per room
pub fn server_solo_wave_spawner_system(
    mut commands: Commands,
    time: Res<Time>,
    net_channels: Res<ServerNetworkChannels>,
    mut matchmaker: ResMut<Matchmaker>,
    nav_grid: Res<NavGrid>,
    base_query: Query<(&Transform, &Faction, &RoomId), With<BaseHQ>>,
) {
    let dt = time.delta_secs();
    let mut waves_to_spawn = Vec::new();

    for (room_id, room) in matchmaker.rooms.iter_mut() {
        if !room.is_active || room.mode != GameMode::SoloVsAi {
            continue;
        }

        room.match_time += dt;

        // Check if Hostile AI Base HQ exists in this room
        let mut ai_base_pos = None;
        let mut p1_base_pos = shared::map::P1_BASE_POS;

        for (tf, faction, b_room) in &base_query {
            if b_room.0 == *room_id {
                if *faction == Faction::HostileAi {
                    ai_base_pos = Some(tf.translation.truncate());
                } else if *faction == Faction::Player1 {
                    p1_base_pos = tf.translation.truncate();
                }
            }
        }

        let Some(base_spawn) = ai_base_pos else {
            // AI HQ destroyed in this room -> no more waves
            continue;
        };

        room.time_until_next_wave -= dt;
        if room.time_until_next_wave <= 0.0 {
            room.current_wave += 1;
            room.time_until_next_wave = 45.0;

            let count = match room.current_wave {
                1 => 3,
                2 => 6,
                3 => 10,
                w => 14 + (w - 4) * 3,
            };

            waves_to_spawn.push((*room_id, room.current_wave, count, base_spawn, p1_base_pos));
        }
    }

    for (room_id, wave_num, count, base_spawn, target_pos) in waves_to_spawn {
        info!(
            "⚔️ [Server WaveAi] Room #{}: Wave {} Incoming! Spawning {} Hostile Marines",
            room_id, wave_num, count
        );

        let peers = matchmaker.get_room_peers(room_id);

        for i in 0..count {
            let angle = (i as f32) * 2.39996;
            let dist = 32.0 * (i as f32).sqrt();
            let offset = Vec2::new(angle.cos(), angle.sin()) * dist;
            let spawn_pos = base_spawn + offset;
            let net_id = matchmaker.alloc_net_id();
            let waypoints = nav_grid.find_path(spawn_pos, target_pos);

            commands.spawn((
                Unit {
                    name: "Hostile Marine".to_string(),
                    supply_cost: 2,
                },
                Soldier {
                    state: SoldierState::AttackMoving,
                    attack_range: 150.0,
                    aggro_radius: 240.0,
                    attack_damage: 14.0,
                    attack_cooldown: 0.9,
                    ..default()
                },
                Stimpack::default(),
                TacticalStance::default(),
                Health::new(120.0),
                Radius(16.0),
                MoveSpeed(175.0),
                Velocity::default(),
                Faction::HostileAi,
                RoomId(room_id),
                NetEntity {
                    net_id,
                    owner_peer_id: 2,
                },
                MoveTarget::with_waypoints(target_pos, true, waypoints),
                Transform::from_xyz(spawn_pos.x, spawn_pos.y, 2.0),
            ));

            if !peers.is_empty() {
                let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::BroadcastToPeers {
                    peer_ids: peers.clone(),
                    msg: ServerMessage::UnitSpawned {
                        net_id,
                        faction: Faction::HostileAi,
                        unit_kind: UnitKind::Soldier,
                        position: spawn_pos,
                        max_hp: 120.0,
                    },
                });
            }
        }
    }
}

