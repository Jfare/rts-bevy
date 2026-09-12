pub mod combat_fx;
pub mod orders;
pub mod session;
pub mod snapshots;
pub mod world;

use bevy::prelude::*;
use shared::components::*;
use shared::economy::PlayerEconomy;
use shared::grid::NavGrid;
use shared::protocol::ServerMessage;

use crate::audio_sfx::SoundEffect;
use crate::chat::ChatLog;
use crate::net::NetClient;
use crate::particles::ParticleEvent;
use crate::AppState;

pub type CleanupQuery<'w, 's> = Query<
    'w,
    's,
    Entity,
    Or<(With<NetEntity>, With<Unit>, With<Building>, With<ResourceNode>)>,
>;

pub type CameraQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Transform,
    (
        With<Camera2d>,
        Without<NetEntity>,
        Without<Unit>,
        Without<Building>,
        Without<ResourceNode>,
    ),
>;

pub type NodeQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static NetEntity, &'static Transform),
    (With<ResourceNode>, Without<Camera2d>, Without<Unit>, Without<Building>),
>;

pub type EntityNetItem = (
    Entity,
    &'static NetEntity,
    &'static Faction,
    &'static mut Transform,
    &'static mut Health,
    Option<&'static mut Worker>,
    Option<&'static mut Soldier>,
    Option<&'static mut MeleeFighter>,
    Option<&'static mut MoveTarget>,
    Option<&'static mut TacticalStance>,
    Option<&'static Radius>,
    Option<&'static mut GunTurret>,
    Option<&'static mut ProductionBuilding>,
);

pub type EntityNetFilter = (Without<Camera2d>, Without<ResourceNode>);

pub type EntityNetQuery<'w, 's> = Query<'w, 's, EntityNetItem, EntityNetFilter>;

/// Dispatches an authoritative ServerMessage to the appropriate modular domain handler.
#[allow(clippy::too_many_arguments)]
pub fn handle_server_message(
    commands: &mut Commands,
    nav_grid: &NavGrid,
    net_client: &mut ResMut<NetClient>,
    economy: &mut ResMut<PlayerEconomy>,
    outcome_opt: &mut Option<ResMut<MatchOutcome>>,
    chat_log_opt: &mut Option<ResMut<ChatLog>>,
    countdown_opt: &mut Option<ResMut<crate::ui::MatchCountdown>>,
    next_state: &mut ResMut<NextState<AppState>>,
    sound_events: &mut EventWriter<SoundEffect>,
    particle_events: &mut EventWriter<ParticleEvent>,
    cleanup_query: &CleanupQuery,
    camera_query: &mut CameraQuery,
    node_query: &NodeQuery,
    entity_query: &mut EntityNetQuery,
    now_ms: u64,
    msg: ServerMessage,
) {
    match msg {
        ServerMessage::LobbyJoined {
            player_id,
            assigned_faction,
            room_id,
            room_code,
            is_game_ready,
        } => session::handle_lobby_joined(
            net_client,
            player_id,
            assigned_faction,
            room_id,
            room_code,
            is_game_ready,
        ),

        ServerMessage::GameStarted {
            p1_pos,
            p2_pos,
            wave_initial_delay: _,
        } => session::handle_game_started(net_client, next_state, camera_query, p1_pos, p2_pos),

        ServerMessage::InitialWorldState {
            entities,
            p1_minerals,
            p1_supply,
            p1_max_supply,
            p2_minerals,
            p2_supply,
            p2_max_supply,
        } => world::handle_initial_world_state(
            commands,
            economy,
            net_client.current_mode,
            cleanup_query,
            entities,
            p1_minerals,
            p1_supply,
            p1_max_supply,
            p2_minerals,
            p2_supply,
            p2_max_supply,
        ),

        ServerMessage::TickSnapshotBatch {
            snapshots,
            p1_minerals,
            p1_supply,
            p1_max_supply,
            p2_minerals,
            p2_supply,
            p2_max_supply,
            ..
        } => snapshots::handle_tick_snapshot_batch(
            commands,
            economy,
            net_client.status,
            net_client.current_mode,
            net_client.my_faction,
            sound_events,
            entity_query,
            snapshots,
            p1_minerals,
            p1_supply,
            p1_max_supply,
            p2_minerals,
            p2_supply,
            p2_max_supply,
        ),

        ServerMessage::UnitsOrderedMove {
            unit_net_ids,
            destinations,
            is_attack_move,
        } => orders::handle_units_ordered_move(
            commands,
            nav_grid,
            entity_query,
            unit_net_ids,
            destinations,
            is_attack_move,
        ),

        ServerMessage::UnitsOrderedAttackTarget {
            unit_net_ids,
            target_net_id,
        } => orders::handle_units_ordered_attack_target(
            commands,
            entity_query,
            unit_net_ids,
            target_net_id,
        ),

        ServerMessage::WorkersOrderedHarvest {
            worker_net_ids,
            resource_net_id,
        } => orders::handle_workers_ordered_harvest(
            commands,
            node_query,
            entity_query,
            worker_net_ids,
            resource_net_id,
        ),

        ServerMessage::UnitsOrderedStop { unit_net_ids } => {
            orders::handle_units_ordered_stop(commands, entity_query, unit_net_ids)
        }

        ServerMessage::UnitsOrderedHoldPosition { unit_net_ids } => {
            orders::handle_units_ordered_hold_position(commands, entity_query, unit_net_ids)
        }

        ServerMessage::UnitsOrderedPatrol {
            unit_net_ids,
            destinations,
        } => orders::handle_units_ordered_patrol(
            commands,
            nav_grid,
            entity_query,
            unit_net_ids,
            destinations,
        ),

        ServerMessage::BuildingSpawned {
            net_id,
            faction,
            building_kind,
            position,
            max_hp,
        } => world::handle_building_spawned(commands, net_id, faction, building_kind, position, max_hp),

        ServerMessage::UnitSpawned {
            net_id,
            faction,
            unit_kind,
            position,
            max_hp,
        } => world::handle_unit_spawned(commands, net_id, faction, unit_kind, position, max_hp),

        ServerMessage::QueueUpdated {
            building_net_id,
            queue_count,
            current_progress,
        } => world::handle_queue_updated(entity_query, building_net_id, queue_count, current_progress),

        ServerMessage::ProjectileFired {
            attacker_net_id,
            target_net_id,
            origin,
            target_pos,
            damage,
        } => combat_fx::handle_projectile_fired(
            commands,
            sound_events,
            particle_events,
            entity_query,
            attacker_net_id,
            target_net_id,
            origin,
            target_pos,
            damage,
        ),

        ServerMessage::EntityDamaged {
            target_net_id,
            current_hp,
            max_hp,
        } => snapshots::handle_entity_damaged(entity_query, target_net_id, current_hp, max_hp),

        ServerMessage::EntityDied { net_id, .. } => {
            snapshots::handle_entity_died(commands, sound_events, entity_query, net_id)
        }

        ServerMessage::MatchEnded { winning_faction, .. } => {
            session::handle_match_ended(outcome_opt, sound_events, net_client.my_faction, winning_faction)
        }

        ServerMessage::Pong { client_timestamp, .. } => {
            session::handle_pong(net_client, now_ms, client_timestamp)
        }

        ServerMessage::LobbyStats {
            queue_1v1,
            active_1v1_matches,
            max_1v1_matches,
            active_solo_matches,
            max_solo_matches,
            total_online,
        } => session::handle_lobby_stats(
            commands,
            now_ms,
            queue_1v1,
            active_1v1_matches,
            max_1v1_matches,
            active_solo_matches,
            max_solo_matches,
            total_online,
        ),

        ServerMessage::ChatMessageReceived {
            sender_name,
            faction,
            color,
            text,
            is_system,
        } => session::handle_chat_message(chat_log_opt, now_ms, sender_name, faction, color, text, is_system),

        ServerMessage::TacticalPingReceived {
            sender_name,
            faction: _,
            color,
            position,
            ping_type,
        } => session::handle_tactical_ping(commands, sender_name, color, position, ping_type),

        ServerMessage::MatchFound {
            opponent_name,
            opponent_color,
            countdown_seconds,
        } => session::handle_match_found(
            countdown_opt,
            sound_events,
            opponent_name,
            opponent_color,
            countdown_seconds,
        ),

        ServerMessage::QueueCancelled => session::handle_queue_cancelled(net_client, next_state),

        ServerMessage::ErrorMessage { reason } => session::handle_error_message(net_client, reason),
    }
}
