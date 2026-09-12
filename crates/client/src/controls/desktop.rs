use bevy::prelude::*;

/// Desktop-specific controls plugin
pub struct DesktopControlsPlugin;

impl Plugin for DesktopControlsPlugin {
    fn build(&self, _app: &mut App) {
        // Desktop mouse & keyboard handling is configured on primary input systems
        // with the is_desktop_control_scheme run condition.
    }
}
