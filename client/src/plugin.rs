use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::client::*;

use shared::settings;

use crate::systems::input;
use crate::ui;
use crate::ui::hud::HudPlugin;
use crate::world::WorldPlugin;

pub struct ClientGamePlugin;

impl Plugin for ClientGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((WorldPlugin, HudPlugin));
        app.add_systems(Startup, connect_to_server);

        // Gather local input every frame and send to server.
        app.add_systems(Update, input::gather_and_send_input);

        // Minigame UI rendering (reads replicated server state, draws overlay).
        app.add_systems(
            Update,
            (
                ui::arc::render_arc,
                ui::dag::render_dag,
                ui::bar_fill::render_bar_fill,
                ui::wave_interference::render_wave_interference,
                ui::value_lock::render_value_lock,
                ui::heartbeat::render_heartbeat,
            ),
        );
    }
}

/// Spawns the client transport entity and initiates the connection to the server.
fn connect_to_server(mut commands: Commands) {
    let client_id = 0u64; // TODO: replace with a proper unique client ID (e.g., from auth service)
    let auth = Authentication::Manual {
        server_addr: settings::client_connect_addr(),
        client_id,
        private_key: [0u8; 32],
        protocol_id: settings::PROTOCOL_ID,
    };

    let netcode_client = NetcodeClient::new(auth, NetcodeConfig::default())
        .expect("Failed to build netcode client");

    let entity = commands.spawn((
        Name::new("GameClient"),
        netcode_client,
        UdpIo::default(),
        LocalAddr("0.0.0.0:0".parse().unwrap()),
        ReplicationReceiver::default(),
    )).id();
    // Trigger the Lightyear client startup chain, same pattern as the server's Start trigger.
    commands.entity(entity).trigger(|e| Connect { entity: e });
    info!("[CLIENT] Spawned NetcodeClient {entity:?}, triggering Connect");
}
