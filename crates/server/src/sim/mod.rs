use bevy::prelude::*;
use shared::grid::NavGrid;

use crate::net_server::{OutgoingNetEvent, ServerNetworkChannels};
use crate::session::Matchmaker;

pub mod combat;
pub mod commands;
pub mod lobby;
pub mod mining;
pub mod movement;
pub mod net_events;
pub mod outcome;
pub mod production;
pub mod snapshots;
pub mod waves;
#[cfg(test)]
mod tests;

pub use combat::{server_combat_system, server_melee_fighter_combat_system, server_turret_combat_system};
pub use mining::server_mining_system;
pub use movement::{
    server_abilities_and_stances_system, server_movement_system,
    server_unit_separation_and_collision_system, update_server_nav_grid_system,
};
pub use net_events::handle_incoming_network_events;
pub use outcome::server_match_outcome_system;
pub use production::server_production_system;
pub use snapshots::server_tick_snapshot_system;
pub use waves::server_solo_wave_spawner_system;

pub struct ServerSimulationPlugin;

impl Plugin for ServerSimulationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Matchmaker::new())
            .init_resource::<NavGrid>()
            .insert_resource(ServerTickTimer(Timer::from_seconds(
                1.0 / 30.0,
                TimerMode::Repeating,
            )))
            .insert_resource(ServerStatsTimer(Timer::from_seconds(
                2.0,
                TimerMode::Repeating,
            )))
            .add_systems(
                Update,
                (
                    server_room_tick_system,
                    handle_incoming_network_events,
                    update_server_nav_grid_system,
                    server_combat_system,
                    server_turret_combat_system,
                    server_melee_fighter_combat_system,
                    server_movement_system,
                    server_abilities_and_stances_system,
                    server_unit_separation_and_collision_system,
                    server_mining_system,
                    server_production_system,
                    server_solo_wave_spawner_system,
                    server_match_outcome_system,
                    server_tick_snapshot_system,
                    server_lobby_stats_broadcast_system,
                ).chain(),
            );
    }
}

#[derive(Resource)]
pub struct ServerTickTimer(pub Timer);

#[derive(Resource)]
pub struct ServerStatsTimer(pub Timer);

pub fn server_room_tick_system(
    time: Res<Time>,
    mut matchmaker: ResMut<Matchmaker>,
) {
    let dt = time.delta_secs();
    for room in matchmaker.rooms.values_mut() {
        if room.is_active {
            if room.countdown_timer > 0.0 {
                room.countdown_timer = (room.countdown_timer - dt).max(0.0);
            } else {
                room.match_time += dt;
            }
        }
    }
}

pub fn server_lobby_stats_broadcast_system(
    time: Res<Time>,
    mut timer: ResMut<ServerStatsTimer>,
    matchmaker: Res<Matchmaker>,
    net_channels: Res<ServerNetworkChannels>,
) {
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        let (q, a1, m1, aso, mso, tot) = matchmaker.get_telemetry();
        crate::net_server::update_global_telemetry(q, a1, aso, tot);

        let _ = net_channels.tx_outgoing.send(OutgoingNetEvent::Broadcast {
            msg: shared::protocol::ServerMessage::LobbyStats {
                queue_1v1: q,
                active_1v1_matches: a1,
                max_1v1_matches: m1,
                active_solo_matches: aso,
                max_solo_matches: mso,
                total_online: tot,
            },
        });
    }
}

