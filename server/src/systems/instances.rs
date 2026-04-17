use std::collections::HashMap;
use std::f32::consts::PI;

use bevy::prelude::*;
use fastnoise_lite::FastNoiseLite;
use lightyear::prelude::*;

use shared::instances::{build_noise, find_def, sample_height, InstanceKind, MobKind};

use super::mob_defs::{floor_offset_for_kind, scatter_offset, spawn_mob};

// ── Runtime state ─────────────────────────────────────────────────────────────

pub struct LiveInstance {
    pub kind: InstanceKind,
    pub group_id: u32,
    /// Lightyear peer IDs currently in this instance.
    pub client_ids: Vec<PeerId>,
    /// All entities (mobs + players) belonging to this instance.
    pub entities: Vec<Entity>,
    /// node_index → mob entities spawned at that pack node.
    pub pack_entities: HashMap<usize, Vec<Entity>>,
    /// Cached noise for this instance's terrain (avoids rebuilding per-frame).
    pub noise: FastNoiseLite,
}

#[derive(Resource, Default)]
pub struct InstanceRegistry {
    next_id: u32,
    pub instances: HashMap<u32, LiveInstance>,
    /// (group_id, InstanceKind) → instance_id.
    /// The first group member to request a kind creates the instance;
    /// subsequent members join the existing one.
    pub group_instances: HashMap<(u32, InstanceKind), u32>,
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

/// Allocate a new `LiveInstance` and register it.
/// Call `populate_instance` afterwards to spawn mobs.
pub fn create_instance(kind: InstanceKind, group_id: u32, reg: &mut InstanceRegistry) -> u32 {
    let id = reg.next_id;
    reg.next_id += 1;

    let def = find_def(kind);
    let noise = build_noise(&def.terrain);

    reg.instances.insert(id, LiveInstance {
        kind,
        group_id,
        client_ids: Vec::new(),
        entities: Vec::new(),
        pack_entities: HashMap::new(),
        noise,
    });
    reg.group_instances.insert((group_id, kind), id);
    info!("[INSTANCES] Created instance {id} kind={kind:?} group={group_id}");
    id
}

/// Spawn all mobs defined by the instance's node graph.
///
/// Uses a two-pass approach to avoid holding a mutable borrow on `reg` while
/// also needing `commands`:
///  1. Read-only pass: sample heights from the cached noise, collect params.
///  2. Spawn pass: issue `commands.spawn` with no registry borrow held.
///  3. Write pass: push spawned entities back into the registry.
pub fn populate_instance(instance_id: u32, reg: &mut InstanceRegistry, commands: &mut Commands) {
    // --- Pass 1: collect spawn params (read-only) ---
    struct MobParam {
        kind: MobKind,
        x: f32,
        y: f32,
        z: f32,
        phase: f32,
        node_idx: usize,
    }

    let params: Vec<MobParam> = {
        let live = reg.instances.get(&instance_id).expect("populate: instance not found");
        let def = find_def(live.kind);
        let mut out = Vec::new();
        for (node_idx, node) in def.nodes.iter().enumerate() {
            let Some(pack) = &node.pack else { continue };
            let mut mob_i = 0usize;
            for mob_spawn in pack.mobs {
                for _ in 0..mob_spawn.count {
                    let (ox, oz) = scatter_offset(node_idx, mob_i, pack.spread);
                    let wx = node.position.0 + ox;
                    let wz = node.position.1 + oz;
                    let y = sample_height(&live.noise, wx, wz, &def.terrain) + floor_offset_for_kind(mob_spawn.kind);
                    let phase = (wx * 3.7 + wz * 5.3).abs() % (2.0 * PI);
                    out.push(MobParam { kind: mob_spawn.kind, x: wx, y, z: wz, phase, node_idx });
                    mob_i += 1;
                }
            }
        }
        out
    };

    // --- Pass 2: spawn entities (registry not borrowed) ---
    let spawned: Vec<(usize, Entity)> = params
        .iter()
        .map(|p| {
            let e = spawn_mob(commands, p.kind, p.x, p.y, p.z, p.phase, instance_id);
            (p.node_idx, e)
        })
        .collect();

    // --- Pass 3: register entities (write borrow) ---
    let live = reg.instances.get_mut(&instance_id).expect("populate: instance not found");
    for (node_idx, entity) in spawned {
        live.entities.push(entity);
        live.pack_entities.entry(node_idx).or_default().push(entity);
    }

    info!(
        "[INSTANCES] Populated instance {instance_id}: {} mobs in {} packs",
        live.entities.len(),
        live.pack_entities.len()
    );
}

/// Add a player to a running instance and update replication targets on all
/// pre-existing entities so the new client immediately receives them.
pub fn assign_player_to_instance(
    instance_id: u32,
    peer_id: PeerId,
    player_entity: Entity,
    reg: &mut InstanceRegistry,
    replicate_query: &mut Query<&mut Replicate>,
) {
    let live = reg.instances.get_mut(&instance_id).expect("assign: instance not found");
    live.client_ids.push(peer_id);
    live.entities.push(player_entity);

    let target = NetworkTarget::Only(live.client_ids.clone());
    for &entity in &live.entities {
        if let Ok(mut rep) = replicate_query.get_mut(entity) {
            *rep = Replicate::to_clients(target.clone());
        }
    }
    info!(
        "[INSTANCES] Assigned {peer_id:?} to instance {instance_id} \
         ({} clients now)",
        live.client_ids.len()
    );
}

/// Remove a player from its instance.
/// If the instance becomes empty, despawns all mob entities and removes the
/// instance from the registry.
pub fn remove_player_from_instance(
    instance_id: u32,
    peer_id: PeerId,
    player_entity: Entity,
    reg: &mut InstanceRegistry,
    commands: &mut Commands,
) {
    let Some(live) = reg.instances.get_mut(&instance_id) else { return };
    live.client_ids.retain(|&id| id != peer_id);
    live.entities.retain(|&e| e != player_entity);

    if !live.client_ids.is_empty() {
        return;
    }

    info!("[INSTANCES] Instance {instance_id} is now empty — tearing down");
    let mob_entities: Vec<Entity> = live.entities.drain(..).collect();
    for e in mob_entities {
        commands.entity(e).despawn();
    }
    let key = (live.group_id, live.kind);
    reg.instances.remove(&instance_id);
    reg.group_instances.remove(&key);
}

// ── System ────────────────────────────────────────────────────────────────────

/// Runs in `FixedUpdate` as a safety-net GC.  Immediate teardown is handled
/// inline in `remove_player_from_instance`; this catches any edge cases.
pub fn tick_instance_teardown(
    _reg: ResMut<InstanceRegistry>,
    _commands: Commands,
) {
}
