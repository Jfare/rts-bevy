pub mod faction_a;
pub mod faction_b;

use bevy::prelude::*;
use shared::factions::PlayableFaction;

#[allow(dead_code)]
pub fn faction_theme_color(faction: PlayableFaction) -> Color {
    match faction {
        PlayableFaction::FactionA => faction_a::THEME_COLOR,
        PlayableFaction::FactionB => faction_b::THEME_COLOR,
    }
}

#[allow(dead_code)]
pub fn faction_hud_tag(faction: PlayableFaction) -> &'static str {
    match faction {
        PlayableFaction::FactionA => faction_a::HUD_TAG,
        PlayableFaction::FactionB => faction_b::HUD_TAG,
    }
}
