use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::client::ClientPlugins;

mod plugin;
mod systems;
mod ui;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
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
