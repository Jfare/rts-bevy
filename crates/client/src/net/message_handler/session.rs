use bevy::prelude::*;
use shared::components::{Faction, MatchOutcome};
use shared::protocol::{FactionColor, PingType};

use crate::audio_sfx::SoundEffect;
use crate::chat::{ChatEntry, ChatLog};
use crate::net::{NetClient, NetStatus, ServerTelemetry};
use crate::pings::TacticalPingVisual;
use crate::AppState;
use super::CameraQuery;

pub fn handle_lobby_joined(
    net_client: &mut NetClient,
    player_id: u64,
    assigned_faction: Faction,
    room_id: u32,
    room_code: Option<String>,
    is_game_ready: bool,
) {
    net_client.my_peer_id = player_id;
    net_client.my_faction = assigned_faction;
    net_client.current_room_code = room_code.clone();
    net_client.status = if is_game_ready {
        NetStatus::InGame
    } else {
        NetStatus::InLobby
    };
    info!(
        "🚪 [NetClient] Assigned Faction: {:?}, Room #{}, Code: {:?}, Status: {:?}",
        assigned_faction, room_id, room_code, net_client.status
    );
}

pub fn handle_game_started(
    net_client: &mut NetClient,
    next_state: &mut NextState<AppState>,
    camera_query: &mut CameraQuery,
    p1_pos: Vec2,
    p2_pos: Vec2,
) {
    info!("⚔️ [NetClient] Match started! Initializing battlefield cameras.");
    net_client.status = NetStatus::InGame;
    next_state.set(AppState::InGame);

    // Center camera on player spawn base
    let spawn_pos = if net_client.my_faction == Faction::Player1 {
        p1_pos
    } else {
        p2_pos
    };

    for mut cam_tf in camera_query.iter_mut() {
        cam_tf.translation.x = spawn_pos.x;
        cam_tf.translation.y = spawn_pos.y;
    }
}

pub fn handle_match_found(
    countdown_opt: &mut Option<ResMut<crate::ui::MatchCountdown>>,
    sound_events: &mut EventWriter<SoundEffect>,
    opponent_name: String,
    opponent_color: FactionColor,
    countdown_seconds: f32,
) {
    info!(
        "⚔️ [NetClient] Match Found vs [{}] ({:?})! Countdown: {:.1}s",
        opponent_name, opponent_color, countdown_seconds
    );
    sound_events.send(SoundEffect::CountdownBeep);

    #[cfg(target_arch = "wasm32")]
    {
        let js_call = format!(
            "if (window.__rts_on_match_found) {{ window.__rts_on_match_found('{}', '{}', {:.1}); }}",
            opponent_name.replace('\'', "\\'"),
            opponent_color.name(),
            countdown_seconds
        );
        let _ = js_sys::eval(&js_call);
    }

    if let Some(ref mut countdown) = countdown_opt {
        countdown.is_active = true;
        countdown.remaining_seconds = countdown_seconds;
        countdown.opponent_name = opponent_name;
        countdown.opponent_color = opponent_color;
        countdown.last_announced_second = (countdown_seconds.ceil() as i32) + 1;
        countdown.has_played_go_sound = false;
    }
}

pub fn handle_match_ended(
    outcome_opt: &mut Option<ResMut<MatchOutcome>>,
    sound_events: &mut EventWriter<SoundEffect>,
    my_faction: Faction,
    winning_faction: Faction,
) {
    if let Some(ref mut outcome) = outcome_opt {
        if winning_faction == my_faction {
            **outcome = MatchOutcome::Victory;
            sound_events.send(SoundEffect::Victory);
        } else {
            **outcome = MatchOutcome::Defeat;
            sound_events.send(SoundEffect::Defeat);
        }
    }
}

pub fn handle_queue_cancelled(
    net_client: &mut NetClient,
    next_state: &mut NextState<AppState>,
) {
    info!("🚪 [NetClient] Queue Cancelled acknowledged by server.");
    net_client.status = NetStatus::Connected;
    next_state.set(AppState::Lobby);
    #[cfg(target_arch = "wasm32")]
    {
        let _ = js_sys::eval("if (window.__rts_on_queue_cancelled) { window.__rts_on_queue_cancelled(); }");
    }
}

pub fn handle_pong(net_client: &mut NetClient, now_ms: u64, client_timestamp: u64) {
    if now_ms >= client_timestamp {
        net_client.rtt_ms = (now_ms - client_timestamp) as u32;
    }
}

pub fn handle_lobby_stats(
    commands: &mut Commands,
    now_ms: u64,
    queue_1v1: u32,
    active_1v1_matches: u32,
    max_1v1_matches: u32,
    active_solo_matches: u32,
    max_solo_matches: u32,
    total_online: u32,
) {
    commands.insert_resource(ServerTelemetry {
        queue_1v1,
        active_1v1_matches,
        max_1v1_matches,
        active_solo_matches,
        max_solo_matches,
        total_online,
        last_updated_ms: now_ms,
    });
}

pub fn handle_chat_message(
    chat_log_opt: &mut Option<ResMut<ChatLog>>,
    now_ms: u64,
    sender_name: String,
    faction: Faction,
    color: FactionColor,
    text: String,
    is_system: bool,
) {
    info!("💬 [Chat] {}: {}", sender_name, text);
    if let Some(ref mut chat_log) = chat_log_opt {
        chat_log.entries.push(ChatEntry {
            sender_name,
            faction,
            color,
            text,
            is_system,
            timestamp_ms: now_ms,
        });
        if chat_log.entries.len() > 100 {
            chat_log.entries.remove(0);
        }
    }
}

pub fn handle_tactical_ping(
    commands: &mut Commands,
    sender_name: String,
    color: FactionColor,
    position: Vec2,
    ping_type: PingType,
) {
    info!("📍 [Ping] {} pinged {:?} at {:?}", sender_name, ping_type, position);
    commands.spawn((
        TacticalPingVisual {
            position,
            ping_type,
            color,
            lifetime: 0.0,
            max_lifetime: 3.5,
        },
        Transform::from_xyz(position.x, position.y, 4.0),
    ));
}

pub fn handle_error_message(net_client: &mut NetClient, reason: String) {
    warn!("🛑 [NetClient] Server message: {}", reason);
    net_client.last_error_message = Some(reason);
}
