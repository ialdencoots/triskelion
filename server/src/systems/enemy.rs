use bevy::prelude::*;
use lightyear::prelude::server::*;

use shared::components::combat::Health;
use shared::components::enemy::{EnemyPosition, EnemyVelocity, MobTarget};
use shared::components::instance::InstanceId;
use shared::components::player::{PlayerId, PlayerPosition};
use shared::instances::{find_def, layout_sdf, sample_height, InstanceKind};

use super::combat::{ThreatEntry, ThreatList};
use super::instances::{create_instance, InstanceRegistry};
use super::mob_defs::MobBehavior;

/// Threat seeded when a mob first aggros onto a player, before any damage is
/// dealt.  Small enough that a single hit immediately overtakes it.
const INITIAL_AGGRO_THREAT: f32 = 1.0;

/// Adds an initial threat entry for `entity` if one doesn't already exist.
fn seed_aggro_threat(threat_list: &mut ThreatList, entity: Entity) {
    if !threat_list.entries.iter().any(|e| e.player_entity == entity) {
        threat_list.entries.push(ThreatEntry { player_entity: entity, threat: INITIAL_AGGRO_THREAT });
        threat_list.dirty = true;
    }
}

/// Selects a chase target from the mob's threat list.  Returns `(dist, dx, dz)`
/// to the chosen target, or `None` if the threat list has no valid entries
/// (caller should fall back to nearest-player logic).
/// Returns `(dist, dx, dz, entity)` for the highest-threat valid target,
/// or `None` if the threat list is empty or no entries are in range.
fn select_threat_target(
    threat_list: &ThreatList,
    mob_iid: u32,
    mob_pos: &EnemyPosition,
    leash_range: f32,
    player_query: &Query<(Entity, &PlayerId, &PlayerPosition, &InstanceId, &Health)>,
) -> Option<(f32, f32, f32, Entity)> {
    if threat_list.entries.is_empty() {
        return None;
    }
    threat_list.entries.iter()
        .filter_map(|entry| {
            let Ok((entity, _, ppos, piid, health)) = player_query.get(entry.player_entity) else { return None };
            if piid.0 != mob_iid { return None; }
            if !health.is_alive() { return None; }
            let dx = ppos.x - mob_pos.x;
            let dz = ppos.z - mob_pos.z;
            let dist = (dx * dx + dz * dz).sqrt();
            if dist > leash_range { return None; }
            Some((entry.threat, dist, dx, dz, entity))
        })
        .max_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
        })
        .map(|(_, dist, dx, dz, entity)| (dist, dx, dz, entity))
}

/// Fires when the NetcodeServer reports `Started`.
/// Creates the group-0 overworld instance and populates it with mobs.
pub fn on_server_started(
    trigger: On<Add, Started>,
    server_q: Query<(), With<NetcodeServer>>,
    mut reg: ResMut<InstanceRegistry>,
    _commands: Commands,
) {
    let entity = trigger.event_target();
    if server_q.get(entity).is_err() {
        info!(
            "[SERVER] on_server_started: Started on {entity:?} is NOT a NetcodeServer — skipping"
        );
        return;
    }
    // Create the overworld instance so group_instances is populated before any
    // client connects.  Mobs are populated lazily in process_spawn_requests
    // when the first player joins, so they can be spawned with the correct
    // NetworkTarget::Only([first_client]) from the start.
    info!("[SERVER] on_server_started: creating group-0 Overworld instance");
    create_instance(InstanceKind::Overworld, 0, &mut reg);
}

const SPEED: f32 = 2.5;
// Radius within which aggro'd mobs push each other apart.
const SEPARATION_RADIUS: f32 = 3.5;
// Maximum contribution of the separation force relative to the unit chase direction.
const SEPARATION_STRENGTH: f32 = 1.2;

/// Returns `(dist, dx, dz, entity)` for the nearest player in the same instance
/// within `max_dist` world units. Pass `f32::INFINITY` for no distance cap.
fn find_nearest_player_in_instance(
    player_query: &Query<(Entity, &PlayerId, &PlayerPosition, &InstanceId, &Health)>,
    iid: u32,
    mob_pos: &EnemyPosition,
    max_dist: f32,
) -> Option<(f32, f32, f32, Entity)> {
    player_query
        .iter()
        .filter(|(_, _, _, piid, _)| piid.0 == iid)
        .map(|(entity, _, ppos, _, _)| {
            let dx = ppos.x - mob_pos.x;
            let dz = ppos.z - mob_pos.z;
            let dist = (dx * dx + dz * dz).sqrt();
            (dist, dx, dz, entity)
        })
        .filter(|(dist, _, _, _)| *dist <= max_dist)
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
}

/// Returns the clamped separation force vector for a mob at `my_xz` from all
/// other mobs in the same instance.
fn compute_separation_force(my_xz: Vec2, mob_positions: &[(Vec2, u32)], iid: u32) -> Vec2 {
    let mut sep = Vec2::ZERO;
    for (other_xz, other_iid) in mob_positions {
        if *other_iid != iid { continue; }
        let diff = my_xz - *other_xz;
        let dist = diff.length();
        if dist > 0.01 && dist < SEPARATION_RADIUS {
            sep += diff.normalize() * (1.0 - dist / SEPARATION_RADIUS);
        }
    }
    sep.clamp_length_max(1.0) * SEPARATION_STRENGTH
}

/// Sets velocity for a mob that has reached melee range.
///
/// Projects the separation force onto the tangent perpendicular to the
/// mob→target direction, so mobs can only shuffle sideways around the ring —
/// not drift outward past melee range. Stops entirely once settled.
fn melee_spread(vel: &mut EnemyVelocity, sep: Vec2, to_target: Vec2) {
    const SETTLE_THRESHOLD: f32 = 0.15;
    let radial = to_target.normalize_or_zero();
    let sep_tangential = sep - radial * sep.dot(radial);
    if sep_tangential.length() > SETTLE_THRESHOLD {
        vel.vx = sep_tangential.x * SPEED * 0.5;
        vel.vz = sep_tangential.y * SPEED * 0.5;
    } else {
        vel.vx = 0.0;
        vel.vz = 0.0;
    }
}

/// Advances all mob AI each fixed tick.
/// Dispatches on `MobBehavior`: Wander uses Lissajous paths, Aggro chases
/// the highest-threat player in the same instance (nearest as fallback).
pub fn tick_enemy_walk(
    time: Res<Time>,
    reg: Res<InstanceRegistry>,
    mut mob_query: Query<(&mut EnemyPosition, &mut EnemyVelocity, &mut MobBehavior, &InstanceId, &mut ThreatList, &mut MobTarget)>,
    player_query: Query<(Entity, &PlayerId, &PlayerPosition, &InstanceId, &Health)>,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();

    let prev = (t - dt) as u32;
    let curr = t as u32;
    if curr != prev && curr % 5 == 0 {
        let count = mob_query.iter().count();
        info!("[SERVER] tick_enemy_walk: {count} mobs at t={t:.1}s");
    }

    // Snapshot positions for separation calculations — collected before any
    // movement so all mobs react to the same frame state.
    let mob_positions: Vec<(Vec2, u32)> = mob_query
        .iter()
        .map(|(pos, _, _, iid, _, _)| (Vec2::new(pos.x, pos.z), iid.0))
        .collect();

    for (mut pos, mut vel, mut behavior, iid, mut threat_list, mut mob_target) in mob_query.iter_mut() {
        match &mut *behavior {
            MobBehavior::Patrol { phase, aggro_range, melee_range, aggroed } => {
                // Find nearest player for aggro trigger (still proximity-based).
                let nearest = find_nearest_player_in_instance(&player_query, iid.0, &pos, f32::INFINITY);

                // Aggro/de-aggro with hysteresis: break at 1.5× the aggro range
                // so mobs don't flicker when the player sits on the threshold.
                let leash_range = *aggro_range * 1.5;
                match nearest {
                    Some((dist, _, _, entity)) if !*aggroed && dist <= *aggro_range => {
                        *aggroed = true;
                        seed_aggro_threat(&mut threat_list, entity);
                    }
                    Some((dist, _, _, _)) if *aggroed && dist > leash_range => {
                        *aggroed = false;
                        threat_list.entries.clear();
                        threat_list.dirty = true;
                    }
                    None => {
                        *aggroed = false;
                        threat_list.entries.clear();
                        threat_list.dirty = true;
                    }
                    _ => {}
                }

                if *aggroed {
                    let sep = compute_separation_force(Vec2::new(pos.x, pos.z), &mob_positions, iid.0);

                    // Chase highest-threat player; fall back to nearest.
                    let target = select_threat_target(&threat_list, iid.0, &pos, leash_range, &player_query)
                        .or(nearest);

                    mob_target.0 = target.and_then(|(_, _, _, e)| {
                        player_query.get(e).ok().map(|(_, pid, _, _, _)| pid.0)
                    });

                    if let Some((dist, dx, dz, _)) = target {
                        if dist > *melee_range {
                            let chase = Vec2::new(dx, dz).normalize_or_zero();
                            let dir = (chase + sep).normalize_or_zero();
                            vel.vx = dir.x * SPEED;
                            vel.vz = dir.y * SPEED;
                        } else {
                            melee_spread(&mut vel, sep, Vec2::new(dx, dz));
                        }
                    } else {
                        vel.vx = 0.0;
                        vel.vz = 0.0;
                    }
                } else {
                    mob_target.0 = None;
                    // Resume Lissajous patrol.
                    let dx = (t * 0.4 + *phase).sin();
                    let dz = (t * 0.3 + *phase * 1.7).cos();
                    let dir = Vec2::new(dx, dz).normalize_or_zero();
                    vel.vx = dir.x * SPEED;
                    vel.vz = dir.y * SPEED;
                }
            }
            MobBehavior::Aggro { aggro_range, melee_range } => {
                let sep = compute_separation_force(Vec2::new(pos.x, pos.z), &mob_positions, iid.0);

                // Nearest player within aggro_range for proximity fallback.
                let nearest_in_range = find_nearest_player_in_instance(&player_query, iid.0, &pos, *aggro_range);

                // Seed initial threat when first engaging (threat list empty).
                if threat_list.entries.is_empty() {
                    if let Some((_, _, _, entity)) = nearest_in_range {
                        seed_aggro_threat(&mut threat_list, entity);
                    }
                }

                // Chase highest-threat player; fall back to nearest in range.
                let target = select_threat_target(&threat_list, iid.0, &pos, *aggro_range, &player_query)
                    .or(nearest_in_range);

                mob_target.0 = target.and_then(|(_, _, _, e)| {
                    player_query.get(e).ok().map(|(_, pid, _, _, _)| pid.0)
                });

                if let Some((dist, dx, dz, _)) = target {
                    if dist > *melee_range {
                        // Chase + spread sideways when closing in.
                        let chase = Vec2::new(dx, dz).normalize_or_zero();
                        let dir = (chase + sep).normalize_or_zero();
                        vel.vx = dir.x * SPEED;
                        vel.vz = dir.y * SPEED;
                    } else {
                        melee_spread(&mut vel, sep, Vec2::new(dx, dz));
                    }
                } else {
                    vel.vx = 0.0;
                    vel.vz = 0.0;
                    threat_list.entries.clear();
                    threat_list.dirty = true;
                    mob_target.0 = None;
                }
            }
        }

        let new_x = pos.x + vel.vx * dt;
        let new_z = pos.z + vel.vz * dt;

        if let Some(live) = reg.instances.get(&iid.0) {
            let def = find_def(live.kind);

            // For layout instances confine movement to corridors/rooms.
            // Try full move first, then axis-by-axis sliding, then stop.
            let (fx, fz) = if def.use_layout_terrain {
                if layout_sdf(new_x, new_z, def) <= 0.0 {
                    (new_x, new_z)
                } else if layout_sdf(new_x, pos.z, def) <= 0.0 {
                    (new_x, pos.z)
                } else if layout_sdf(pos.x, new_z, def) <= 0.0 {
                    (pos.x, new_z)
                } else {
                    (pos.x, pos.z)
                }
            } else {
                (new_x, new_z)
            };

            pos.x = fx;
            pos.z = fz;
            pos.y = sample_height(&live.noise, fx, fz, &def.terrain) + 1.1;
        } else {
            pos.x = new_x;
            pos.z = new_z;
        }
    }
}
