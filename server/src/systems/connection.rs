use std::f32::consts::TAU;
use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::components::combat::{AbilityCooldowns, CombatState, Health};
use shared::components::instance::InstanceId;
use shared::components::minigame::{
    arc::ArcState, bar_fill::BarFillState, dag::DagState, heartbeat::HeartbeatState,
    value_lock::ValueLockState, wave_interference::WaveInterferenceState,
};
use shared::components::player::{
    Class, GroupId, PlayerClass, PlayerId, PlayerName, PlayerPosition, PlayerSubclass, PlayerVelocity,
};
use shared::instances::{find_def, sample_height, InstanceKind};
use shared::messages::{InstanceEnteredMsg, RequestSpawnMsg};

use super::instances::{create_instance, populate_instance, remove_player_from_instance, InstanceRegistry};

/// Links a client's network link entity to its spawned player entity and instance.
#[derive(Component)]
pub struct PlayerEntityLink(pub Entity);

/// Records which instance a client was assigned to, for cleanup on disconnect.
#[derive(Component)]
pub struct PlayerInstanceLink {
    pub instance_id: u32,
    pub peer_id: PeerId,
}

pub fn on_link_of_added(trigger: On<Add, LinkOf>, mut commands: Commands) {
    let link = trigger.event_target();
    info!("[SERVER] New client link {link:?} — inserting ReplicationSender");
    commands.entity(link).insert((
        ReplicationSender::new(Duration::from_millis(100), SendUpdatesMode::SinceLastAck, false),
        Name::new("ClientLink"),
    ));
}

pub fn debug_connected_sender(trigger: On<Add, (Connected, ReplicationSender)>) {
    let entity = trigger.event_target();
    info!("[SERVER] On<Add,(Connected,ReplicationSender)> fired for {entity:?}");
}

pub fn on_client_connected(trigger: On<Add, Connected>, _commands: Commands) {
    let link_entity = trigger.event_target();
    info!("[SERVER] Client link {link_entity:?} is now Connected — waiting for RequestSpawnMsg");
}

/// Despawns the player entity and removes them from their instance on disconnect.
pub fn on_client_disconnected(
    trigger: On<Add, Disconnected>,
    links: Query<(&PlayerEntityLink, Option<&PlayerInstanceLink>)>,
    mut reg: ResMut<InstanceRegistry>,
    mut commands: Commands,
) {
    let link_entity = trigger.event_target();
    info!("[SERVER] Client link {link_entity:?} disconnected");
    if let Ok((entity_link, inst_link)) = links.get(link_entity) {
        let player_entity = entity_link.0;
        if let Some(il) = inst_link {
            remove_player_from_instance(
                il.instance_id,
                il.peer_id,
                player_entity,
                &mut reg,
                &mut commands,
            );
        }
        commands.entity(player_entity).despawn();
        info!("[SERVER] Despawned player {:?}", player_entity);
    }
}

/// Reads `RequestSpawnMsg`, spawns the player, assigns them to their group's
/// overworld instance, and sends `InstanceEnteredMsg` back to the client.
pub fn process_spawn_requests(
    mut commands: Commands,
    mut link_query: Query<
        (Entity, &RemoteId, &mut MessageReceiver<RequestSpawnMsg>, Option<&mut MessageSender<InstanceEnteredMsg>>),
        (With<Connected>, Without<PlayerEntityLink>),
    >,
    mut reg: ResMut<InstanceRegistry>,
) {
    for (link_entity, remote_id, mut receiver, mut instance_sender) in link_query.iter_mut() {
        for req in receiver.receive() {
            let PeerId::Netcode(client_id) = remote_id.0 else { continue };
            let peer_id = remote_id.0;

            // Spread spawn positions in a circle so players don't overlap.
            let angle = (client_id % 8) as f32 * TAU / 8.0;
            let spawn_x = angle.cos() * 3.0;
            let spawn_z = angle.sin() * 3.0;

            // Use group 0 for all players until group selection UI is added.
            let group_id: u32 = 0;
            let kind = InstanceKind::Overworld;

            // Find or create the group's overworld instance.
            let instance_id = if let Some(&id) = reg.group_instances.get(&(group_id, kind)) {
                id
            } else {
                create_instance(kind, group_id, &mut reg)
            };

            let def = find_def(kind);

            let spawn_y = {
                let live = reg.instances.get(&instance_id).expect("instance missing");
                sample_height(&live.noise, spawn_x, spawn_z, &def.terrain) + 1.1
            };

            // Add this client to the registry before populating.
            let is_first_client = {
                let live = reg.instances.get_mut(&instance_id).expect("instance missing");
                let first = live.client_ids.is_empty();
                live.client_ids.push(peer_id);
                first
            };

            // Lazily populate mobs when the first client joins.
            if is_first_client {
                populate_instance(instance_id, &mut reg, &mut commands);
            }

            // Use NetworkTarget::All so Lightyear's handle_connection mechanism
            // automatically sends existing entities to each new client as they
            // connect.  NetworkTarget::Only(subset) breaks this because
            // handle_connection fires before process_spawn_requests can update
            // the target list, leaving newly-joined clients unable to see
            // pre-existing entities.  Per-instance filtering is handled
            // client-side via the replicated InstanceId component.
            let player_entity = commands.spawn((
                Name::new(format!("Player_{}", req.name)),
                PlayerId(client_id),
                GroupId(group_id),
                PlayerName(req.name.clone()),
                PlayerClass(req.class.clone()),
                PlayerSubclass(req.subclass.clone()),
                PlayerPosition { x: spawn_x, y: spawn_y, z: spawn_z },
                PlayerVelocity::default(),
                Health::default(),
                CombatState::default(),
                AbilityCooldowns::default(),
                InstanceId(instance_id),
                Replicate::to_clients(NetworkTarget::All),
            )).id();

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

            // Track the player entity in the registry for future joins.
            reg.instances.get_mut(&instance_id).expect("instance missing")
                .entities.push(player_entity);

            commands.entity(link_entity).insert((
                PlayerEntityLink(player_entity),
                PlayerInstanceLink { instance_id, peer_id },
            ));

            // Send InstanceEnteredMsg to this client so it rebuilds terrain.
            if let Some(ref mut sender) = instance_sender {
                sender.send::<GameChannel>(InstanceEnteredMsg {
                    instance_id,
                    kind,
                    terrain: def.terrain,
                    spawn_x,
                    spawn_z,
                });
            }

            info!(
                "[SERVER] Spawned '{}' (client={client_id}) as {player_entity:?} in instance {instance_id}",
                req.name
            );
        }
    }
}
