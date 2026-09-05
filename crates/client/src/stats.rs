use bevy::prelude::*;
use shared::components::AppState;

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
        }
    }
}

impl MatchStats {
    /// Computes real-time Actions Per Minute (APM) over the last 60 seconds
    pub fn current_apm(&self) -> u32 {
        self.recent_command_timestamps.len() as u32
    }

    /// Computes overall Kill/Death ratio
    pub fn kd_ratio(&self) -> f32 {
        if self.units_lost == 0 {
            self.enemy_units_killed as f32
        } else {
            self.enemy_units_killed as f32 / self.units_lost as f32
        }
    }

    pub fn record_action(&mut self) {
        self.total_commands += 1;
        self.recent_command_timestamps.push(self.elapsed_seconds);
    }
}

pub struct StatsPlugin;

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchStats>()
            .add_systems(
                Update,
                (
                    update_match_stats_system,
                    track_player_input_apm_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Updates timer and trims rolling APM timestamp window
fn update_match_stats_system(
    time: Res<Time>,
    mut stats: ResMut<MatchStats>,
) {
    let dt = time.delta_secs();
    stats.elapsed_seconds += dt;

    let cutoff = stats.elapsed_seconds - 60.0;
    stats.recent_command_timestamps.retain(|&t| t >= cutoff);
}

/// Detects player tactical inputs (mouse clicks & hotkeys) to track APM
fn track_player_input_apm_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut stats: ResMut<MatchStats>,
) {
    let mouse_clicked = mouse.just_pressed(MouseButton::Left)
        || mouse.just_pressed(MouseButton::Right)
        || mouse.just_pressed(MouseButton::Middle);

    let key_pressed = keys.get_just_pressed().next().is_some();

    if mouse_clicked || key_pressed {
        stats.record_action();
    }
}
