use std::f32::consts::TAU;
use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::server::*;

use shared::components::combat::{AbilityCooldowns, CombatState, Health};
use shared::components::minigame::{
    arc::ArcState, bar_fill::BarFillState, dag::DagState, heartbeat::HeartbeatState,
    value_lock::ValueLockState, wave_interference::WaveInterferenceState,
};
use shared::components::player::{
    Class, GroupId, PlayerClass, PlayerId, PlayerName, PlayerPosition, PlayerSubclass, PlayerVelocity,
};
use shared::messages::RequestSpawnMsg;
use shared::terrain;

/// Links a client's network link entity to its spawned player entity.
/// Added to the link entity by `process_spawn_requests` after the player is created.
#[derive(Component)]
pub struct PlayerEntityLink(pub Entity);

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
pub fn on_client_connected(trigger: On<Add, Connected>, _commands: Commands) {
    let link_entity = trigger.event_target();
    info!("[SERVER] Client link {link_entity:?} is now fully Connected — waiting for RequestSpawnMsg");
}

/// Fires when a client's connection transitions to Disconnected.
/// Despawns the associated player entity.
pub fn on_client_disconnected(
    trigger: On<Add, Disconnected>,
    links: Query<&PlayerEntityLink>,
    mut commands: Commands,
) {
    let link_entity = trigger.event_target();
    info!("[SERVER] Client link {link_entity:?} disconnected");
    if let Ok(link) = links.get(link_entity) {
        commands.entity(link.0).despawn();
        info!("[SERVER] Despawned player entity {:?}", link.0);
    }
}

/// Reads `RequestSpawnMsg` from connected clients that don't yet have a player entity.
/// Spawns the player and records the link→player mapping in `PlayerEntityLink`.
pub fn process_spawn_requests(
    mut commands: Commands,
    mut link_query: Query<
        (Entity, &RemoteId, &mut MessageReceiver<RequestSpawnMsg>),
        (With<Connected>, Without<PlayerEntityLink>),
    >,
) {
    for (link_entity, remote_id, mut receiver) in link_query.iter_mut() {
        for req in receiver.receive() {
            let PeerId::Netcode(client_id) = remote_id.0 else { continue };

            // Spread spawn positions in a circle so players don't overlap.
            let angle = (client_id % 8) as f32 * TAU / 8.0;
            let x = angle.cos() * 3.0;
            let z = angle.sin() * 3.0;
            let y = terrain::height_at(x, z) + 1.1;

            let player_entity = commands.spawn((
                Name::new(format!("Player_{}", req.name)),
                PlayerId(client_id),
                GroupId(0), // All clients share group 0 for now
                PlayerName(req.name.clone()),
                PlayerClass(req.class.clone()),
                PlayerSubclass(req.subclass.clone()),
                PlayerPosition { x, y, z },
                PlayerVelocity::default(),
                Health::default(),
                CombatState::default(),
                AbilityCooldowns::default(),
                Replicate::to_clients(NetworkTarget::All),
            )).id();

            // Add class-specific minigame state
            match &req.class {
                Class::Physical => {
                    commands.entity(player_entity).insert((ArcState::default(), DagState::default()));
                }
                Class::Arcane => {
                    commands.entity(player_entity).insert((BarFillState::default(), WaveInterferenceState::default()));
                }
                Class::Nature => {
                    commands.entity(player_entity).insert((ValueLockState::default(), HeartbeatState::default()));
                }
            }

            commands.entity(link_entity).insert(PlayerEntityLink(player_entity));
            info!("[SERVER] Spawned player '{}' (client_id={client_id}) as {player_entity:?} at ({x:.1},{y:.1},{z:.1})", req.name);
        }
    }
}
