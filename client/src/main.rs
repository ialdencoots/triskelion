use std::time::Duration;

use bevy::log::LogPlugin;
use bevy::prelude::*;
use lightyear::prelude::client::ClientPlugins;

mod plugin;
mod systems;
mod ui;
mod world;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(LogPlugin {
                filter: "warn,client=info,lightyear_netcode=info".into(),
                ..default()
            }),
            // Lightyear client networking stack.
            ClientPlugins {
                tick_duration: Duration::from_secs_f64(
                    1.0 / shared::settings::FIXED_TIMESTEP_HZ,
                ),
            },
            // Shared protocol: component/message/channel registration.
            shared::SharedPlugin,
            // Game-logic and UI systems.
            plugin::ClientGamePlugin,
        ))
        .run();
}
