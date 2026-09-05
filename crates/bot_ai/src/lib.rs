use bevy::prelude::*;
use shared::components::*;

/// Configuration and state for solo-skirmish hostile AI attack waves
#[derive(Debug, Clone, Resource, Reflect)]
pub struct WaveAiState {
    pub current_wave: u32,
    pub time_until_next_wave: f32,
    pub initial_delay: f32,
    pub wave_interval: f32,
    pub is_active: bool,
    pub ai_spawn_pos: Vec2,
    pub target_player_pos: Vec2,
}

impl Default for WaveAiState {
    fn default() -> Self {
        Self {
            current_wave: 0,
            time_until_next_wave: 40.0,
            initial_delay: 40.0,
            wave_interval: 45.0,
            is_active: false,
            ai_spawn_pos: shared::map::P2_BASE_POS,
            target_player_pos: shared::map::P1_BASE_POS,
        }
    }
}

pub struct WaveAiPlugin;

impl Plugin for WaveAiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaveAiState>()
            .add_systems(Update, wave_spawner_system);
            .add_systems(Update, wave_spawner_system.run_if(in_state(AppState::InGame)));
    }
}

/// Spawns escalating squads of hostile marines when the wave countdown expires
fn wave_spawner_system(
    mut commands: Commands,
    time: Res<Time>,
    outcome_opt: Option<Res<MatchOutcome>>,
    mut ai_state: ResMut<WaveAiState>,
    base_query: Query<(&Transform, &Faction, &BaseHQ)>,
) {
    // 1. Check if match has ended (Victory or Defeat)
    if let Some(outcome) = outcome_opt {
        if *outcome != MatchOutcome::InProgress {
            ai_state.is_active = false;
            return;
        }
    }

    if !ai_state.is_active {
        return;
    }

    // 2. Check if Hostile AI Base HQ still exists on the battlefield
    let mut has_ai_base = false;
    for (transform, faction, _) in &base_query {
        if *faction == Faction::Player1 {
            ai_state.target_player_pos = transform.translation.truncate();
        } else if *faction == Faction::HostileAi {
            ai_state.ai_spawn_pos = transform.translation.truncate();
            has_ai_base = true;
        }
    }

    // If hostile base is completely destroyed, stop incoming waves!
    if !has_ai_base {
        ai_state.is_active = false;
        info!("🛑 [WaveAi] Hostile Base HQ is gone! Ceasing all assault waves.");
        return;
    }

    ai_state.time_until_next_wave -= time.delta_secs();

    if ai_state.time_until_next_wave <= 0.0 {
        ai_state.current_wave += 1;
        ai_state.time_until_next_wave = ai_state.wave_interval;

        // Squad size escalates: Wave 1 = 3, Wave 2 = 6, Wave 3 = 10, Wave 4+ = 14 + (wave-4)*3
        let count = match ai_state.current_wave {
            1 => 3,
            2 => 6,
            3 => 10,
            w => 14 + (w - 4) * 3,
        };

        info!(
            "⚔️ [WaveAi] ⚠️ Wave {} Incoming! Spawning {} Hostile Marines attacking player base!",
            ai_state.current_wave, count
        );

        let base_spawn = ai_state.ai_spawn_pos;
        let target_pos = ai_state.target_player_pos;

        for i in 0..count {
            let angle = (i as f32) * 2.39996;
            let dist = 32.0 * (i as f32).sqrt();
            let offset = Vec2::new(angle.cos(), angle.sin()) * dist;
            let spawn_pos = base_spawn + offset;

            let net_id = 9000 + ai_state.current_wave * 100 + i;

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
                Selectable::default(),
                NetEntity {
                    net_id,
                    owner_peer_id: 2,
                },
                MoveTarget::new(target_pos, true),
                Transform::from_xyz(spawn_pos.x, spawn_pos.y, 2.0),
            ));
        }
    }
}
