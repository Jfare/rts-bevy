use bevy::prelude::*;
use shared::components::{AppState, MatchOutcome};

/// Tracks real-time gameplay metrics and actions per minute (APM)
#[derive(Resource, Debug)]
pub struct MatchStats {
    pub elapsed_seconds: f32,
    pub minerals_mined: u32,
    pub minerals_spent: u32,
    pub units_trained: u32,
    pub units_lost: u32,
    pub enemy_units_killed: u32,
    pub enemy_buildings_destroyed: u32,
    pub damage_dealt: f32,
    pub total_commands: u32,
    pub recent_command_timestamps: Vec<f32>,
    pub final_apm: Option<u32>,
}

impl Default for MatchStats {
    fn default() -> Self {
        Self {
            elapsed_seconds: 0.0,
            minerals_mined: 0,
            minerals_spent: 0,
            units_trained: 0,
            units_lost: 0,
            enemy_units_killed: 0,
            enemy_buildings_destroyed: 0,
            damage_dealt: 0.0,
            total_commands: 0,
            recent_command_timestamps: Vec::new(),
            final_apm: None,
        }
    }
}

impl MatchStats {
    /// Computes real-time Actions Per Minute (APM) over a 60-second rolling window.
    /// Before 60 seconds have elapsed, scales by the elapsed time to accurately reflect rate.
    pub fn current_apm(&self) -> u32 {
        if let Some(final_apm) = self.final_apm {
            return final_apm;
        }

        if self.recent_command_timestamps.is_empty() {
            return 0;
        }

        let window_duration = self.elapsed_seconds.min(60.0).max(3.0);
        let count = self.recent_command_timestamps.len() as f32;
        ((count / window_duration) * 60.0).round() as u32
    }

    /// Computes overall match average APM across the entire match duration
    pub fn average_apm(&self) -> u32 {
        if self.total_commands == 0 || self.elapsed_seconds < 1.0 {
            return 0;
        }
        ((self.total_commands as f32 / self.elapsed_seconds) * 60.0).round() as u32
    }

    /// Computes overall Kill/Death ratio
    pub fn kd_ratio(&self) -> f32 {
        if self.units_lost == 0 {
            self.enemy_units_killed as f32
        } else {
            self.enemy_units_killed as f32 / self.units_lost as f32
        }
    }

    /// Records a genuine tactical player action (orders, unit training, building, selection, pings)
    pub fn record_action(&mut self) {
        if self.final_apm.is_some() {
            return;
        }
        self.total_commands += 1;
        self.recent_command_timestamps.push(self.elapsed_seconds);
    }
}

pub struct StatsPlugin;

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchStats>()
            .add_systems(OnEnter(AppState::InGame), reset_match_stats_system)
            .add_systems(
                Update,
                update_match_stats_system.run_if(in_state(AppState::InGame)),
            );
    }
}

/// Clears previous match metrics on entering a new match
fn reset_match_stats_system(mut stats: ResMut<MatchStats>) {
    *stats = MatchStats::default();
}

/// Updates timer and trims rolling APM timestamp window; freezes stats when match concludes
fn update_match_stats_system(
    time: Res<Time>,
    outcome_opt: Option<Res<MatchOutcome>>,
    mut stats: ResMut<MatchStats>,
) {
    // When the match concludes, freeze elapsed time and lock the final APM
    if outcome_opt.as_deref() == Some(&MatchOutcome::Victory)
        || outcome_opt.as_deref() == Some(&MatchOutcome::Defeat)
    {
        if stats.final_apm.is_none() {
            stats.final_apm = Some(stats.average_apm());
        }
        return;
    }

    let dt = time.delta_secs();
    stats.elapsed_seconds += dt;

    let cutoff = stats.elapsed_seconds - 60.0;
    stats.recent_command_timestamps.retain(|&t| t >= cutoff);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apm_rate_extrapolation_early_game() {
        let mut stats = MatchStats::default();
        stats.elapsed_seconds = 10.0;
        for _ in 0..10 {
            stats.record_action();
        }
        // 10 actions in 10 seconds = 60 APM
        assert_eq!(stats.current_apm(), 60);
    }

    #[test]
    fn test_apm_full_window_matches_count() {
        let mut stats = MatchStats::default();
        stats.elapsed_seconds = 90.0;
        for i in 0..75 {
            stats.recent_command_timestamps.push(35.0 + i as f32 * 0.7);
        }
        // In a full 60s window, 75 actions = 75 APM
        assert_eq!(stats.current_apm(), 75);
    }

    #[test]
    fn test_final_apm_locks_and_prevents_tracking() {
        let mut stats = MatchStats::default();
        stats.elapsed_seconds = 120.0;
        stats.total_commands = 240;
        stats.final_apm = Some(120);

        // Recording actions after game ends should be completely ignored
        stats.record_action();
        assert_eq!(stats.total_commands, 240);
        assert_eq!(stats.current_apm(), 120);
    }

    #[test]
    fn test_update_match_stats_system_locks_on_victory() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);
        app.init_resource::<MatchStats>();
        app.insert_resource(MatchOutcome::Victory);
        {
            let mut stats = app.world_mut().resource_mut::<MatchStats>();
            stats.elapsed_seconds = 60.0;
            stats.total_commands = 90;
        }
        app.add_systems(Update, update_match_stats_system);
        app.update();

        let stats = app.world().resource::<MatchStats>();
        assert_eq!(stats.final_apm, Some(90));
        assert_eq!(stats.current_apm(), 90);
        assert_eq!(stats.elapsed_seconds, 60.0);
    }
}
