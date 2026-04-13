#![allow(unused_variables)]

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use shared::components::player::PlayerId;

/// Fires when a new link is established server-side.
/// Insert ReplicationSender here so it is already on the entity when Connected
/// arrives — that causes Bevy to fire On<Add, (Connected, ReplicationSender)>
/// when Connected is added, triggering Lightyear's handle_connection.
pub fn on_link_of_added(trigger: On<Add, LinkOf>, mut commands: Commands) {
    let link = trigger.event_target();
    info!("[SERVER] New client link {link:?} — inserting ReplicationSender");
    commands.entity(link).insert((
        ReplicationSender::new(Duration::from_millis(100), SendUpdatesMode::SinceLastAck, false),
        Name::new("ClientLink"),
    ));
}

/// Fires when the (Connected, ReplicationSender) bundle is complete on a link.
/// This is the SAME trigger as Lightyear's handle_connection — if this fires,
/// handle_connection should also fire and enqueue existing entities for the sender.
pub fn debug_connected_sender(trigger: On<Add, (Connected, ReplicationSender)>) {
    let entity = trigger.event_target();
    info!("[SERVER] On<Add,(Connected,ReplicationSender)> fired for {entity:?} — handle_connection should run now");
}

/// Fires when a client completes the netcode handshake and is fully connected.
pub fn on_client_connected(trigger: On<Add, Connected>, mut commands: Commands) {
    let link_entity = trigger.event_target();
    info!("[SERVER] Client link {link_entity:?} is now fully Connected");
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
