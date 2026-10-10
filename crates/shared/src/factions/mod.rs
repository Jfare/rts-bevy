pub mod faction_a;
pub mod faction_b;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Playable faction archetypes.
/// Orthogonal to player team slots (Faction::Player1, Faction::Player2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Component, Reflect, Default)]
pub enum PlayableFaction {
    #[default]
    FactionA,
    FactionB,
}

impl PlayableFaction {
    pub fn name(&self) -> &'static str {
        match self {
            PlayableFaction::FactionA => faction_a::NAME,
            PlayableFaction::FactionB => faction_b::NAME,
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            PlayableFaction::FactionA => faction_a::BADGE,
            PlayableFaction::FactionB => faction_b::BADGE,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            PlayableFaction::FactionA => faction_a::DESCRIPTION,
            PlayableFaction::FactionB => faction_b::DESCRIPTION,
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            PlayableFaction::FactionA => "FactionA",
            PlayableFaction::FactionB => "FactionB",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id.trim().to_ascii_lowercase().as_str() {
            "b" | "factionb" | "faction_b" => PlayableFaction::FactionB,
            _ => PlayableFaction::FactionA,
        }
    }

    pub fn opposite(&self) -> Self {
        match self {
            PlayableFaction::FactionA => PlayableFaction::FactionB,
            PlayableFaction::FactionB => PlayableFaction::FactionA,
        }
    }

    /// Randomly picks Faction A or Faction B (50/50 probability)
    pub fn random() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1337);
        let c = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        #[cfg(target_arch = "wasm32")]
        let seed = {
            let math_rand = (js_sys::Math::random() * 4294967296.0) as u64;
            math_rand ^ c
        };

        #[cfg(not(target_arch = "wasm32"))]
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
            ^ c;

        // Simple xorshift hash to disperse bits
        let mut x = seed ^ (seed >> 12);
        x ^= x << 25;
        x ^= x >> 27;

        if x % 2 == 0 {
            PlayableFaction::FactionA
        } else {
            PlayableFaction::FactionB
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playable_faction_names_and_badges() {
        assert_eq!(PlayableFaction::FactionA.name(), "Faction A");
        assert_eq!(PlayableFaction::FactionB.name(), "Faction B");
        assert_eq!(PlayableFaction::FactionA.badge(), "🅰️");
        assert_eq!(PlayableFaction::FactionB.badge(), "🅱️");
        assert_eq!(PlayableFaction::FactionA.id(), "FactionA");
        assert_eq!(PlayableFaction::FactionB.id(), "FactionB");
        assert_eq!(PlayableFaction::from_id("FactionA"), PlayableFaction::FactionA);
        assert_eq!(PlayableFaction::from_id("factionb"), PlayableFaction::FactionB);
        assert_eq!(PlayableFaction::from_id("b"), PlayableFaction::FactionB);
        assert_eq!(PlayableFaction::FactionA.opposite(), PlayableFaction::FactionB);
        assert_eq!(PlayableFaction::FactionB.opposite(), PlayableFaction::FactionA);
    }

    #[test]
    fn test_playable_faction_random() {
        let mut seen_a = false;
        let mut seen_b = false;
        for _ in 0..200 {
            match PlayableFaction::random() {
                PlayableFaction::FactionA => seen_a = true,
                PlayableFaction::FactionB => seen_b = true,
            }
        }
        assert!(seen_a, "Expected to see FactionA in 200 random rolls");
        assert!(seen_b, "Expected to see FactionB in 200 random rolls");
    }
}
