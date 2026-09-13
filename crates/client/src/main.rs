#![allow(clippy::type_complexity, clippy::too_many_arguments)]

mod audio_sfx;
mod camera;
mod chat;
mod combat;
mod command_marker;
mod fog_of_war;
mod mining;
mod minimap;
mod net;
mod particles;
mod pings;
mod placement;
mod controls;
mod production;
mod render_units;
mod selection;
mod stats;
mod ui;
mod unit_movement;
mod world_grid;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use audio_sfx::AudioSfxPlugin;
use camera::{RtsCamera, RtsCameraPlugin};
use chat::ChatPlugin;
use combat::CombatPlugin;
use command_marker::CommandMarkerPlugin;
use controls::ControlsPlugin;
use fog_of_war::FogOfWarPlugin;
use mining::MiningPlugin;
use minimap::MinimapPlugin;
use net::NetClientPlugin;
use particles::ParticlesPlugin;
use pings::TacticalPingPlugin;
use placement::PlacementPlugin;
use production::ProductionPlugin;
use render_units::RenderUnitsPlugin;
use selection::SelectionPlugin;
use shared::components::AppState;
use shared::economy::PlayerEconomy;
use stats::StatsPlugin;
use ui::RtsUiPlugin;
use unit_movement::UnitMovementPlugin;
use world_grid::WorldGridPlugin;

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let mut app = App::new();

    let economy = PlayerEconomy::new();

    #[cfg(target_arch = "wasm32")]
    let window_res = WindowResolution::new(1280.0, 720.0).with_scale_factor_override(1.0);
    #[cfg(not(target_arch = "wasm32"))]
    let window_res = WindowResolution::new(1280.0, 720.0);

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Mini-RTS (Bevy 0.15)".to_string(),
                    canvas: Some("#bevy-canvas".to_string()),
                    resolution: window_res,
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: false,
                    ..default()
                }),
                ..default()
            })
            .set(ImagePlugin::default_nearest()),
    )
    .init_state::<AppState>()
    .insert_resource(ClearColor(Color::srgb(0.04, 0.06, 0.08)))
    .insert_resource(economy)
    .add_plugins((
        WorldGridPlugin,
        RtsCameraPlugin,
        SelectionPlugin,
        CommandMarkerPlugin,
        UnitMovementPlugin,
        RenderUnitsPlugin,
        MiningPlugin,
        ProductionPlugin,
    ))
    .add_plugins((
        PlacementPlugin,
        CombatPlugin,
        AudioSfxPlugin,
        ParticlesPlugin,
        StatsPlugin,
        MinimapPlugin,
        FogOfWarPlugin,
        RtsUiPlugin,
        ChatPlugin,
        TacticalPingPlugin,
        NetClientPlugin,
        bot_ai::WaveAiPlugin,
        ControlsPlugin,
    ));


    // Enable Bevy Remote Protocol (BRP) for live MCP debugging on native desktop
    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(bevy::remote::RemotePlugin::default());

    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, sync_browser_canvas_resolution);

    app.add_systems(Startup, setup_demo_scene);

    app.run();
}

/// Spawns the RTS camera and initializes the viewport
fn setup_demo_scene(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        RtsCamera::default(),
        Transform::from_xyz(shared::map::P1_BASE_POS.x, shared::map::P1_BASE_POS.y, 0.0),
    ));
}

/// Synchronizes the Bevy window physical resolution and scale factor override with the browser window inner dimensions
#[cfg(target_arch = "wasm32")]
fn sync_browser_canvas_resolution(
    mut window_query: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    if let Ok(mut window) = window_query.get_single_mut() {
        if let Some(web_win) = web_sys::window() {
            if let (Ok(w), Ok(h)) = (web_win.inner_width(), web_win.inner_height()) {
                if let (Some(w_val), Some(h_val)) = (w.as_f64(), h.as_f64()) {
                    let w = w_val as f32;
                    let h = h_val as f32;
                    if (window.physical_width() as f32 - w).abs() > 1.0 || (window.physical_height() as f32 - h).abs() > 1.0 {
                        window.resolution.set_scale_factor_override(Some(1.0));
                        window.resolution.set_physical_resolution(w as u32, h as u32);
                    }
                }
            }
        }
    }
}
