use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::server::*;

use shared::settings;

use crate::systems::{combat, connection, minigame};

pub struct ServerGamePlugin;

impl Plugin for ServerGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_listening);

        // Handle new link-of connections (add replication/messaging components).
        app.add_observer(connection::on_link_of_added);
        // Handle full client connections (spawn player entity).
        app.add_observer(connection::on_client_connected);
        // Handle disconnections (despawn player entity).
        app.add_observer(connection::on_client_disconnected);

        // Combat and minigame tick in FixedUpdate for deterministic simulation.
        app.add_systems(
            FixedUpdate,
            (
                combat::process_player_inputs,
                combat::tick_ability_cooldowns,
                minigame::tick_arc_states,
                minigame::tick_dag_states,
                minigame::tick_bar_fill_states,
                minigame::tick_wave_interference_states,
                minigame::tick_value_lock_states,
                minigame::tick_heartbeat_states,
            ),
        );
    }
}

/// Spawns the server transport entity and begins listening for UDP connections.
fn start_listening(mut commands: Commands) {
    let addr = settings::server_listen_addr();
    commands.spawn((
        Name::new("GameServer"),
        NetcodeServer::new(
            NetcodeConfig::default().with_protocol_id(settings::PROTOCOL_ID),
        ),
        ServerUdpIo::default(),
        LocalAddr(addr),
    ));
    info!("Server listening on {addr}");
}
