#![allow(clippy::type_complexity, clippy::too_many_arguments)]

mod game_session;
mod net_server;
mod sim_systems;

use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use net_server::ServerNetworkPlugin;
use sim_systems::ServerSimulationPlugin;
use std::time::Duration;

fn main() {
    println!("🚀 [Mini-RTS Server] Starting dedicated headless Bevy 0.15 server...");

    App::new()
        // Run deterministic 30 Hz loop for network replication
        .add_plugins(
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / 30.0,
            ))),
        )
        .add_plugins(bevy::remote::RemotePlugin::default())
        .add_plugins(ServerNetworkPlugin::default())
        .add_plugins(ServerSimulationPlugin)
        .add_systems(Startup, setup_server)
        .add_systems(Update, server_heartbeat)
        .run();
}

fn setup_server() {
    println!("✅ [Mini-RTS Server] Dedicated server started! Remote BRP active on port 15702.");
}

fn server_heartbeat(time: Res<Time>, mut timer: Local<f32>) {
    *timer += time.delta_secs();
    if *timer >= 10.0 {
        *timer = 0.0;
        println!("💓 [Mini-RTS Server] Heartbeat - Server active at 30 Hz.");
    }
}
