#![allow(unused_variables)]

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use shared::components::player::PlayerId;

/// Fires when a new link is established server-side.
/// Adds the replication sender to the link entity so it can push state to clients.
pub fn on_link_of_added(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(trigger.event_target()).insert((
        ReplicationSender::new(
            Duration::from_millis(100),
            SendUpdatesMode::SinceLastAck,
            false,
        ),
        Name::new("ClientLink"),
    ));
}

/// Fires when a client completes the netcode handshake and is fully connected.
/// The actual player entity is spawned after the client sends `RequestSpawnMsg`.
pub fn on_client_connected(trigger: On<Add, Connected>, commands: Commands) {
    let link_entity = trigger.event_target();
    info!("Client link {link_entity:?} is now connected");
    // TODO: wait for RequestSpawnMsg, then call spawn_player(commands, link_entity, ...)
}

/// Fires when a client's connection transitions to Disconnected.
/// Despawns the associated player entity.
pub fn on_client_disconnected(
    trigger: On<Add, Disconnected>,
    players: Query<(Entity, &PlayerId)>,
    commands: Commands,
) {
    let link_entity = trigger.event_target();
    info!("Client link {link_entity:?} disconnected");
    // TODO: map link_entity → PlayerId → despawn player entity
}
