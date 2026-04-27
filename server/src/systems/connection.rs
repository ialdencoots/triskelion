use std::f32::consts::TAU;
use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::components::combat::{AbilityCooldowns, CombatState, Health, ReplicatedMitigationPool};
use shared::components::instance::InstanceId;
use shared::components::minigame::{
    arc::{ArcState, SecondaryArcState}, bar_fill::BarFillState, cube::CubeState,
    grid::{DpsGridTrigger, GridState}, heartbeat::HeartbeatState,
    value_lock::ValueLockState, wave_interference::WaveInterferenceState,
};
use shared::components::player::{
    Class, GroupId, PlayerClass, PlayerId, PlayerName, PlayerPosition, PlayerSelectedTarget,
    PlayerSubclass, PlayerVelocity,
};
use shared::instances::{find_def, terrain_surface_y, InstanceKind};
use shared::messages::{InstanceEnteredMsg, RequestInstanceMsg, RequestSpawnMsg};
use shared::settings::PLAYER_FLOAT_HEIGHT;

use super::combat::{reset_on_stance_change, CritStreak, MitigationPool, ThreatModifiers};
use super::instances::{create_instance, populate_instance, remove_player_from_instance, InstanceRegistry};

/// Spread spawn positions in a circle of 8 slots so clients don't stack on top
/// of each other. Any stable hash of `client_id` would do; `% 8` is fine while
/// instances cap well below 8 clients.
const SPAWN_SLOTS: u32 = 8;
const SPAWN_RADIUS: f32 = 3.0;

fn spawn_slot(client_id: u64) -> (f32, f32) {
    let angle = (client_id as u32 % SPAWN_SLOTS) as f32 * TAU / SPAWN_SLOTS as f32;
    (angle.cos() * SPAWN_RADIUS, angle.sin() * SPAWN_RADIUS)
}

/// Single source of truth for "what group does this player belong to?" —
/// currently hardcoded to 0 until group selection UI exists. Threaded through
/// `PeerId` so it reads as a lookup rather than a magic constant at call sites.
fn default_group_id(_peer: PeerId) -> u32 {
    0
}

/// Find or create the group's instance of the given kind, returning its id.
fn ensure_group_instance(
    kind: InstanceKind,
    group_id: u32,
    reg: &mut InstanceRegistry,
) -> u32 {
    if let Some(&id) = reg.group_instances.get(&(group_id, kind)) {
        id
    } else {
        create_instance(kind, group_id, reg)
    }
}

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

            let (spawn_x, spawn_z) = spawn_slot(client_id);
            let group_id = default_group_id(peer_id);
            let kind = InstanceKind::Overworld;
            let instance_id = ensure_group_instance(kind, group_id, &mut reg);
            let def = find_def(kind);

            let spawn_y = {
                let live = reg.instances.get(&instance_id).expect("instance missing");
                terrain_surface_y(&live.noise, spawn_x, spawn_z, def) + PLAYER_FLOAT_HEIGHT
            };

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
                ThreatModifiers::default(),
                PlayerSelectedTarget::default(),
                InstanceId(instance_id),
                Replicate::to_clients(NetworkTarget::All),
            )).id();

            match &req.class {
                Class::Physical => {
                    commands.entity(player_entity).insert((
                        ArcState::default(),
                        SecondaryArcState::default(),
                        CubeState::default(),
                        GridState::default(),
                        DpsGridTrigger::default(),
                        // Heal stance is available to any Physical subclass —
                        // the mitigation pool is gated by stance, not subclass.
                        // Cost when not in Heal stance is just two empty
                        // components, so attach them universally.
                        MitigationPool::default(),
                        CritStreak::default(),
                        // Replicated mirror — server keeps it in sync each
                        // tick so clients can render the pool fill.
                        ReplicatedMitigationPool::default(),
                    ));
                }
                Class::Arcane => {
                    commands.entity(player_entity).insert((BarFillState::default(), WaveInterferenceState::default()));
                }
                Class::Nature => {
                    commands.entity(player_entity).insert((ValueLockState::default(), HeartbeatState::default()));
                }
            }

            // Single mutable borrow: push the new client + the new player entity
            // and decide whether mobs need spawning. Persisted instances keep
            // their existing mobs across rejoins, so only populate on first use.
            let needs_population = {
                let live = reg.instances.get_mut(&instance_id).expect("instance missing");
                let needs = live.pack_entities.is_empty();
                live.client_ids.push(peer_id);
                live.entities.push(player_entity);
                needs
            };
            if needs_population {
                populate_instance(instance_id, &mut reg, &mut commands);
            }

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

/// Handles `RequestInstanceMsg` from already-spawned clients.
/// Moves the player from their current instance into the requested one,
/// creating it if this is the first member of their group to enter.
pub fn process_instance_requests(
    mut commands: Commands,
    mut link_query: Query<
        (Entity, &RemoteId, &PlayerEntityLink, &mut PlayerInstanceLink,
         &mut MessageReceiver<RequestInstanceMsg>, Option<&mut MessageSender<InstanceEnteredMsg>>),
        With<Connected>,
    >,
    mut instance_id_q: Query<&mut InstanceId>,
    mut player_state_q: Query<(
        &mut CombatState,
        Option<&mut ArcState>,
        Option<&mut SecondaryArcState>,
        Option<&mut CubeState>,
        Option<&mut GridState>,
        Option<&mut DpsGridTrigger>,
    )>,
    mut reg: ResMut<InstanceRegistry>,
) {
    for (_link_entity, remote_id, entity_link, mut inst_link, mut receiver, mut instance_sender) in
        link_query.iter_mut()
    {
        for req in receiver.receive() {
            let PeerId::Netcode(client_id) = remote_id.0 else { continue };
            let peer_id = remote_id.0;
            let player_entity = entity_link.0;
            let old_instance_id = inst_link.instance_id;

            // Skip if already in the requested instance kind.
            if let Some(live) = reg.instances.get(&old_instance_id) {
                if live.kind == req.kind {
                    continue;
                }
            }

            // Remove player from their current instance (tears it down if now empty).
            remove_player_from_instance(old_instance_id, peer_id, player_entity, &mut reg, &mut commands);

            let group_id = default_group_id(peer_id);
            let kind = req.kind;
            let new_instance_id = ensure_group_instance(kind, group_id, &mut reg);
            let def = find_def(kind);

            let (spawn_x, spawn_z) = spawn_slot(client_id);

            // Add this client to the new instance registry.
            let needs_population = {
                let live = reg.instances.get_mut(&new_instance_id).expect("instance missing");
                let needs = live.pack_entities.is_empty();
                live.client_ids.push(peer_id);
                live.entities.push(player_entity);
                needs
            };

            // Populate mobs only when the instance has never been populated;
            // persisted instances keep their existing mobs across rejoins.
            if needs_population {
                populate_instance(new_instance_id, &mut reg, &mut commands);
            }

            // Update the player's InstanceId so instance-based filtering works.
            if let Ok(mut inst_id) = instance_id_q.get_mut(player_entity) {
                inst_id.0 = new_instance_id;
            }

            // Reset stance on instance entry, and run the same per-stance
            // cleanup that process_player_inputs does on any stance change:
            // cancels an active cube and clears arc streak/history. Without
            // this, switching instances would leave the cube rendered but
            // unreachable (the input handler gates on stance).
            if let Ok((mut combat, mut arc_opt, mut sec_opt, mut cube_opt, mut grid_opt, mut grid_trig_opt)) =
                player_state_q.get_mut(player_entity)
            {
                if combat.active_stance.is_some() {
                    combat.active_stance = None;
                    reset_on_stance_change(
                        arc_opt.as_deref_mut(),
                        sec_opt.as_deref_mut(),
                        cube_opt.as_deref_mut(),
                        grid_opt.as_deref_mut(),
                        grid_trig_opt.as_deref_mut(),
                    );
                }
            }

            // Update the link's instance tracking for disconnect cleanup.
            inst_link.instance_id = new_instance_id;

            // Send InstanceEnteredMsg so the client rebuilds its terrain.
            if let Some(ref mut sender) = instance_sender {
                sender.send::<GameChannel>(InstanceEnteredMsg {
                    instance_id: new_instance_id,
                    kind,
                    terrain: def.terrain,
                    spawn_x,
                    spawn_z,
                });
            }

            info!(
                "[SERVER] Transferred client={client_id} from instance {old_instance_id} \
                 to instance {new_instance_id} (kind={kind:?})"
            );
        }
    }
}
