use bevy::prelude::*;
use std::collections::HashMap;
use crate::components::Faction;

#[derive(Debug, Clone, Reflect)]
pub struct FactionEconomy {
    pub minerals: u32,
    pub current_supply: u32,
    pub max_supply: u32,
}

impl Default for FactionEconomy {
    fn default() -> Self {
        Self {
            minerals: 200,
            current_supply: 0,
            max_supply: 10,
        }
    }
}

/// Global resource tracking per-faction economy (minerals and supply limits)
#[derive(Debug, Clone, Resource, Default, Reflect)]
pub struct PlayerEconomy {
    pub economies: HashMap<Faction, FactionEconomy>,
}

impl PlayerEconomy {
    pub fn new() -> Self {
        let mut economies = HashMap::new();
        economies.insert(
            Faction::Player1,
            FactionEconomy {
                minerals: 200,
                current_supply: 0,
                max_supply: 10,
            },
        );
        economies.insert(
            Faction::Player2,
            FactionEconomy {
                minerals: 200,
                current_supply: 0,
                max_supply: 10,
            },
        );
        economies.insert(
            Faction::HostileAi,
            FactionEconomy {
                minerals: 200,
                current_supply: 0,
                max_supply: 10,
            },
        );
        Self { economies }
    }

    pub fn get(&self, faction: Faction) -> FactionEconomy {
        self.economies
            .get(&faction)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_minerals(&self, faction: Faction) -> u32 {
        self.economies
            .get(&faction)
            .map(|e| e.minerals)
            .unwrap_or(0)
    }

    pub fn add_minerals(&mut self, faction: Faction, amount: u32) {
        let entry = self.economies.entry(faction).or_default();
        entry.minerals = entry.minerals.saturating_add(amount);
    }

    pub fn has_minerals(&self, faction: Faction, amount: u32) -> bool {
        self.get_minerals(faction) >= amount
    }

    pub fn spend_minerals(&mut self, faction: Faction, amount: u32) -> bool {
        let entry = self.economies.entry(faction).or_default();
        if entry.minerals >= amount {
            entry.minerals -= amount;
            true
        } else {
            false
        }
    }

    pub fn get_supply(&self, faction: Faction) -> (u32, u32) {
        if let Some(entry) = self.economies.get(&faction) {
            (entry.current_supply, entry.max_supply)
        } else {
            (0, 10)
        }
    }

    pub fn has_supply(&self, faction: Faction, cost: u32) -> bool {
        let (current, max) = self.get_supply(faction);
        current + cost <= max
    }

    pub fn register_supply(&mut self, faction: Faction, cost: u32) {
        let entry = self.economies.entry(faction).or_default();
        entry.current_supply = entry.current_supply.saturating_add(cost);
    }

    pub fn unregister_supply(&mut self, faction: Faction, cost: u32) {
        let entry = self.economies.entry(faction).or_default();
        entry.current_supply = entry.current_supply.saturating_sub(cost);
    }

    pub fn add_max_supply(&mut self, faction: Faction, amount: u32) {
        let entry = self.economies.entry(faction).or_default();
        entry.max_supply = entry.max_supply.saturating_add(amount);
    }
}
