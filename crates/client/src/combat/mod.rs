pub mod melee_fighters;
pub mod projectiles;
pub mod ranged_fighters;
pub mod turrets;

use bevy::prelude::*;
use shared::components::{AppState, Faction, MatchOutcome};

pub use melee_fighters::melee_fighter_combat_system;
pub use projectiles::{
    death_and_elimination_system, draw_combat_gizmos, muzzle_flash_system,
    projectile_movement_and_impact_system,
};
pub use ranged_fighters::ranged_fighter_combat_system;
pub use turrets::turret_combat_system;

/// Target snapshot used for disjoint scanning and combat logic
pub(crate) struct TargetSnapshot {
    pub(crate) entity: Entity,
    pub(crate) pos: Vec2,
    pub(crate) radius: f32,
    pub(crate) faction: Faction,
    pub(crate) is_dead: bool,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchOutcome>()
            .add_systems(
                Update,
                (
                    ranged_fighter_combat_system,
                    turret_combat_system,
                    melee_fighter_combat_system,
                    projectile_movement_and_impact_system,
                    muzzle_flash_system,
                    death_and_elimination_system,
                    draw_combat_gizmos,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

