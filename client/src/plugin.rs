use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::client::*;

use shared::channels::GameChannel;
use shared::components::player::{Class, Subclass};
use shared::messages::RequestSpawnMsg;
use shared::settings;

use crate::systems::input;
use crate::systems::keybindings::ActionBarBindings;
use crate::ui;
use crate::ui::hud::HudPlugin;
use crate::world::WorldPlugin;

/// The client ID assigned to this instance.  Used to skip rendering our own
/// server-authoritative player entity (we have a physics-driven local copy).
#[derive(Resource)]
pub struct LocalClientId(pub u64);

pub struct ClientGamePlugin;

impl Plugin for ClientGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((WorldPlugin, HudPlugin));
        app.init_resource::<ActionBarBindings>();
        app.add_systems(Startup, connect_to_server);

        // Gather local input every frame and send to server.
        app.add_systems(Update, input::gather_and_send_input);
        app.add_systems(Update, input::send_target_selection);

        // Send RequestSpawnMsg as soon as the Netcode handshake completes.
        app.add_observer(on_client_connected);

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

/// Sends `RequestSpawnMsg` to the server immediately after connecting.
/// Class defaults to Physical/Duelist; a character-select screen can replace this later.
fn on_client_connected(
    trigger: On<Add, Connected>,
    mut sender_query: Query<&mut MessageSender<RequestSpawnMsg>>,
    local_id: Res<LocalClientId>,
) {
    let entity = trigger.event_target();
    let Ok(mut sender) = sender_query.get_mut(entity) else { return };
    let name = format!("Player{}", local_id.0 % 1000);
    sender.send::<GameChannel>(RequestSpawnMsg {
        name: name.clone(),
        class: Class::Physical,
        subclass: Subclass::Duelist,
    });
    info!("[CLIENT] Connected — sent RequestSpawnMsg for '{name}'");
}

/// Spawns the client transport entity and initiates the connection to the server.
fn connect_to_server(mut commands: Commands) {
    // Use microsecond timestamp for a unique-enough client ID per run.
    let client_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);

    commands.insert_resource(LocalClientId(client_id));

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
    info!("[CLIENT] Spawned NetcodeClient {entity:?} (id={client_id}), triggering Connect");
}
