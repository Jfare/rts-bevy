use bevy::prelude::*;
use crate::audio_sfx::SoundEffect;
use super::{CountdownNumberText, CountdownOverlayContainer, CountdownSubText, MatchCountdown};

pub fn update_match_countdown_system(
    time: Res<Time>,
    mut countdown: ResMut<MatchCountdown>,
    mut sound_events: EventWriter<SoundEffect>,
    mut container_query: Query<&mut Node, With<CountdownOverlayContainer>>,
    mut text_query: Query<&mut Text, (With<CountdownNumberText>, Without<CountdownSubText>)>,
    mut subtext_query: Query<&mut Text, (With<CountdownSubText>, Without<CountdownNumberText>)>,
    mut color_query: Query<&mut TextColor, With<CountdownNumberText>>,
) {
    if !countdown.is_active {
        for mut node in &mut container_query {
            node.display = Display::None;
        }
        return;
    }

    countdown.remaining_seconds -= time.delta_secs();

    let sec = countdown.remaining_seconds.ceil() as i32;
    if sec > 0 && sec < countdown.last_announced_second {
        countdown.last_announced_second = sec;
        sound_events.send(SoundEffect::CountdownBeep);
    }

    if countdown.remaining_seconds <= 0.0 && !countdown.has_played_go_sound {
        countdown.has_played_go_sound = true;
        sound_events.send(SoundEffect::MatchStart);
    }

    if countdown.remaining_seconds <= -1.2 {
        countdown.is_active = false;
        for mut node in &mut container_query {
            node.display = Display::None;
        }
        return;
    }

    for mut node in &mut container_query {
        node.display = Display::Flex;
    }

    for mut text in &mut text_query {
        if countdown.remaining_seconds > 2.0 {
            text.0 = "3".to_string();
        } else if countdown.remaining_seconds > 1.0 {
            text.0 = "2".to_string();
        } else if countdown.remaining_seconds > 0.0 {
            text.0 = "1".to_string();
        } else {
            text.0 = "⚡ ENGAGE! ⚡".to_string();
        }
    }

    for mut color in &mut color_query {
        if countdown.remaining_seconds > 0.0 {
            color.0 = Color::srgb(0.22, 0.74, 0.97); // Cyan
        } else {
            color.0 = Color::srgb(0.29, 0.87, 0.50); // Emerald Green
        }
    }

    for mut subtext in &mut subtext_query {
        if countdown.remaining_seconds > 0.0 {
            if !countdown.opponent_name.is_empty() {
                subtext.0 = format!("VS {}", countdown.opponent_name);
            } else {
                subtext.0 = "PREPARE FOR BATTLE".to_string();
            }
        } else {
            subtext.0 = "COMMAND PROTOCOLS ENGAGED".to_string();
        }
    }
}

