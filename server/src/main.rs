use std::time::Duration;

use bevy::log::LogPlugin;
use bevy::prelude::*;
use lightyear::prelude::server::ServerPlugins;

fn main() {
    App::new()
        .add_plugins((
            // Headless Bevy — no window or renderer on the server.
            MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(
                Duration::from_secs_f64(1.0 / shared::settings::FIXED_TIMESTEP_HZ),
            )),
            LogPlugin {
                filter: "warn,server=info,lightyear_netcode=info".into(),
                ..default()
            },
            // Lightyear server networking stack.
            ServerPlugins {
                tick_duration: Duration::from_secs_f64(
                    1.0 / shared::settings::FIXED_TIMESTEP_HZ,
                ),
            },
            // Shared protocol: component/message/channel registration.
            shared::SharedPlugin,
            // Game-logic systems.
            server::plugin::ServerGamePlugin,
        ))
        .run();
}
