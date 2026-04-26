use bevy::prelude::*;
use lightyear::prelude::server::*;

use shared::components::combat::{
    AttackShape, Dead, Health, TargetSelector,
};
use shared::components::enemy::{
    EnemyAbilityCooldowns, EnemyCast, EnemyPosition, EnemyVelocity, MobTarget,
};
use shared::components::instance::InstanceId;
use shared::components::player::{PlayerId, PlayerPosition};
use shared::events::combat::{DamageEvent, DisruptionEvent};
use shared::instances::{find_def, layout_sdf, terrain_surface_y, InstanceKind};
use shared::settings::PLAYER_FLOAT_HEIGHT;

use super::combat::{ThreatEntry, ThreatList};
use super::instances::{create_instance, InstanceRegistry};
use super::mob_defs::{stats_for_ability, stats_for_kind, MobBehavior, MobKindComp};
use crate::util::cmp_f32;

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
    player_query: &Query<(Entity, &PlayerId, &PlayerPosition, &InstanceId, &Health), Without<Dead>>,
) -> Option<(f32, f32, f32, Entity)> {
    if threat_list.entries.is_empty() {
        return None;
    }
    threat_list.entries.iter()
        .filter_map(|entry| {
            // The query filter skips Dead entries via Err(_), so an aged
            // threat entry pointing at a corpse drops out automatically.
            let Ok((entity, _, ppos, piid, health)) = player_query.get(entry.player_entity) else { return None };
            if piid.0 != mob_iid { return None; }
            if !health.is_alive() { return None; }
            let dx = ppos.x - mob_pos.x;
            let dz = ppos.z - mob_pos.z;
            let dist = (dx * dx + dz * dz).sqrt();
            if dist > leash_range { return None; }
            Some((entry.threat, dist, dx, dz, entity))
        })
        .max_by(|a, b| cmp_f32(a.0, b.0).then(cmp_f32(b.1, a.1)))
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
    player_query: &Query<(Entity, &PlayerId, &PlayerPosition, &InstanceId, &Health), Without<Dead>>,
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
        .min_by(|a, b| cmp_f32(a.0, b.0))
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
    mut mob_query: Query<(&mut EnemyPosition, &mut EnemyVelocity, &mut MobBehavior, &InstanceId, &mut ThreatList, &mut MobTarget), Without<Dead>>,
    player_query: Query<(Entity, &PlayerId, &PlayerPosition, &InstanceId, &Health), Without<Dead>>,
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
                let nearest = find_nearest_player_in_instance(&player_query, iid.0, &pos, f32::INFINITY);
                let leash_range = *aggro_range * 1.5;
                let has_threat = !threat_list.entries.is_empty();

                if !*aggroed {
                    // Aggro from proximity or from any damage received.
                    if has_threat {
                        *aggroed = true;
                    } else if let Some((dist, _, _, entity)) = nearest {
                        if dist <= *aggro_range {
                            *aggroed = true;
                            seed_aggro_threat(&mut threat_list, entity);
                        }
                    }
                }

                if *aggroed {
                    let sep = compute_separation_force(Vec2::new(pos.x, pos.z), &mob_positions, iid.0);

                    // Damage-aggroed mobs chase with no distance cap so a player
                    // who DoT'd from range can't trivially walk away. Proximity-
                    // only aggro (empty threat list) uses the normal leash range.
                    let chase_leash = if has_threat { f32::INFINITY } else { leash_range };
                    let target = select_threat_target(&threat_list, iid.0, &pos, chase_leash, &player_query)
                        .or_else(|| nearest.filter(|(d, ..)| *d <= leash_range));

                    if target.is_none() {
                        // No reachable target — reset to patrol.
                        *aggroed = false;
                        threat_list.entries.clear();
                        threat_list.dirty = true;
                        mob_target.0 = None;
                    } else {
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
                        }
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

                // Chase highest-threat player with no distance cap (damage aggros
                // from any range); fall back to nearest in range for proximity aggro.
                let threat_leash = if threat_list.entries.is_empty() { *aggro_range } else { f32::INFINITY };
                let target = select_threat_target(&threat_list, iid.0, &pos, threat_leash, &player_query)
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
            pos.y = terrain_surface_y(&live.noise, fx, fz, def) + PLAYER_FLOAT_HEIGHT;
        } else {
            pos.x = new_x;
            pos.z = new_z;
        }
    }
}

// ── Ability execution ────────────────────────────────────────────────────────

/// Clearance above the sight line a terrain sample may occupy before LoS is
/// considered blocked. Values in world units. Set generously so rolling hills
/// don't eat every cast — only real ridges/walls break LoS.
const LOS_CLEARANCE: f32 = 0.8;
/// Number of terrain samples taken along the sight line. More = finer
/// detection of sharp ridges, at small cost per cast per tick.
const LOS_SAMPLES: usize = 5;

/// Return true if the ray from `ax,ay,az` to `bx,by,bz` is unobstructed by
/// terrain in the given instance. Samples terrain height at LOS_SAMPLES
/// interior points along the line and blocks if any sample rises above the
/// interpolated sight line by more than LOS_CLEARANCE. Layout-instance
/// walls (crystal caverns corridors) are not yet considered — treated as
/// open. Returns true outside any known instance.
fn has_los(
    reg: &InstanceRegistry,
    iid: u32,
    ax: f32, ay: f32, az: f32,
    bx: f32, by: f32, bz: f32,
) -> bool {
    let Some(live) = reg.instances.get(&iid) else { return true };
    let def = find_def(live.kind);
    for i in 1..LOS_SAMPLES {
        let t = i as f32 / LOS_SAMPLES as f32;
        let sx = ax + (bx - ax) * t;
        let sy = ay + (by - ay) * t;
        let sz = az + (bz - az) * t;
        let terrain = terrain_surface_y(&live.noise, sx, sz, def);
        if terrain > sy + LOS_CLEARANCE {
            return false;
        }
    }
    true
}

/// Picks the top-threat player entity in the given instance from a threat list.
/// Returns `None` if the list is empty or no entry resolves to a live player
/// in the mob's instance.
fn top_threat_target<'a>(
    threat_list: &ThreatList,
    mob_iid: u32,
    player_query: &'a Query<(Entity, &PlayerId, &PlayerPosition, &InstanceId, &Health), Without<Dead>>,
) -> Option<(Entity, u64, f32, f32, f32)> {
    threat_list.entries.iter()
        .max_by(|a, b| cmp_f32(a.threat, b.threat))
        .and_then(|entry| {
            let (e, pid, ppos, piid, health) = player_query.get(entry.player_entity).ok()?;
            if piid.0 != mob_iid || !health.is_alive() { return None; }
            Some((e, pid.0, ppos.x, ppos.y, ppos.z))
        })
}

/// Tick every enemy's ability cooldowns and, if no cast is active, potentially
/// fire an auto-attack and/or initiate a telegraphed special move.
///
/// Auto-attacks resolve instantly; they write `DamageEvent` + `DisruptionEvent`
/// and reset `auto_cd`. Special moves are initiated by inserting an
/// `EnemyCast` component; resolution is handled by `tick_enemy_casts`.
pub fn tick_enemy_abilities(
    time: Res<Time>,
    mut commands: Commands,
    reg: Res<InstanceRegistry>,
    mut mob_query: Query<(
        Entity,
        &MobKindComp,
        &EnemyPosition,
        &InstanceId,
        &ThreatList,
        &mut EnemyAbilityCooldowns,
        Option<&EnemyCast>,
    ), Without<Dead>>,
    player_query: Query<(Entity, &PlayerId, &PlayerPosition, &InstanceId, &Health), Without<Dead>>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut disruption_writer: MessageWriter<DisruptionEvent>,
) {
    let dt = time.delta_secs();

    for (mob_entity, kind_comp, mob_pos, mob_iid, threat_list, mut cooldowns, cast_opt)
        in mob_query.iter_mut()
    {
        cooldowns.auto_cd = (cooldowns.auto_cd - dt).max(0.0);
        for cd in cooldowns.specials_cd.iter_mut() {
            *cd = (*cd - dt).max(0.0);
        }

        // A mob in the middle of a telegraphed cast doesn't auto-attack and
        // doesn't initiate new casts — it's committed until resolve or cancel.
        if cast_opt.is_some() { continue; }

        let stats = stats_for_kind(kind_comp.0);
        let top = top_threat_target(threat_list, mob_iid.0, &player_query);

        // ── Auto-attack ────────────────────────────────────────────────────
        if cooldowns.auto_cd <= 0.0 {
            if let Some((target_entity, _pid, tx, ty, tz)) = top {
                let dx = tx - mob_pos.x;
                let dz = tz - mob_pos.z;
                let dist = (dx * dx + dz * dz).sqrt();
                let auto_stats = stats_for_ability(stats.auto_attack);
                let reach = stats.melee_range.max(auto_stats.range);
                if dist <= reach
                    && has_los(&reg, mob_iid.0, mob_pos.x, mob_pos.y, mob_pos.z, tx, ty, tz)
                {
                    damage_writer.write(DamageEvent {
                        attacker:    mob_entity,
                        target:      target_entity,
                        base:        auto_stats.damage,
                        ty:          auto_stats.ty,
                        additive:    0.0,
                        multipliers: 1.0,
                        quality:     1.0,
                        is_crit:     false,
                    });
                    disruption_writer.write(DisruptionEvent {
                        target:  target_entity,
                        profile: auto_stats.disruption,
                    });
                    cooldowns.auto_cd = auto_stats.cooldown;
                }
            }
        }

        // ── Special-move initiation ────────────────────────────────────────
        // Scan in order; the first ready ability whose preconditions pass
        // locks in. One cast per mob at a time.
        for (i, ability) in stats.specials.iter().copied().enumerate() {
            if cooldowns.specials_cd[i] > 0.0 { continue; }
            let a_stats = stats_for_ability(ability);

            let (target_pid, aim_x, aim_y, aim_z) = match a_stats.selector {
                TargetSelector::TopThreat => {
                    let Some((_, pid, tx, ty, tz)) = top else { continue };
                    let dx = tx - mob_pos.x;
                    let dz = tz - mob_pos.z;
                    let dist = (dx * dx + dz * dz).sqrt();
                    if dist > a_stats.range { continue; }
                    if !has_los(&reg, mob_iid.0, mob_pos.x, mob_pos.y, mob_pos.z, tx, ty, tz) {
                        continue;
                    }
                    (pid, tx, ty, tz)
                }
                TargetSelector::AllInRange => {
                    // Anchor the aim at the enemy itself — the resolve
                    // sphere/cone is centered on the mob for untargeted AoE.
                    (0u64, mob_pos.x, mob_pos.y, mob_pos.z)
                }
            };

            commands.entity(mob_entity).insert(EnemyCast {
                ability,
                shape:    a_stats.shape,
                target:   target_pid,
                aim_x,
                aim_y,
                aim_z,
                elapsed:  0.0,
                duration: a_stats.telegraph,
            });
            cooldowns.specials_cd[i] = a_stats.cooldown;
            break;
        }
    }
}

/// Advance any active `EnemyCast` on an enemy. Each tick:
/// 1. Increment `elapsed`.
/// 2. Re-check LoS to the target player (for TopThreat casts). If blocked →
///    cancel the cast and lower its cooldown to `foiled_cd`.
/// 3. If `elapsed >= duration`, resolve the attack at the locked aim point
///    per its `AttackShape` and remove the `EnemyCast` component.
pub fn tick_enemy_casts(
    time: Res<Time>,
    mut commands: Commands,
    reg: Res<InstanceRegistry>,
    mut cast_query: Query<(
        Entity,
        &MobKindComp,
        &EnemyPosition,
        &InstanceId,
        &mut EnemyCast,
        &mut EnemyAbilityCooldowns,
    ), Without<Dead>>,
    player_query: Query<(Entity, &PlayerId, &PlayerPosition, &InstanceId, &Health), Without<Dead>>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut disruption_writer: MessageWriter<DisruptionEvent>,
) {
    let dt = time.delta_secs();

    for (mob_entity, kind_comp, mob_pos, mob_iid, mut cast, mut cooldowns)
        in cast_query.iter_mut()
    {
        cast.elapsed += dt;

        let ability_stats = stats_for_ability(cast.ability);
        let stats = stats_for_kind(kind_comp.0);

        // Resolve cast.target (PlayerId) to current player data for LoS / Single resolve.
        // Non-zero target means a specific player; zero = AoE anchored on the enemy.
        let locked_player = if cast.target != 0 {
            player_query.iter()
                .find(|(_, pid, _, piid, _)| pid.0 == cast.target && piid.0 == mob_iid.0)
                .map(|(e, _, ppos, _, health)| (e, ppos.x, ppos.y, ppos.z, health.is_alive()))
        } else {
            None
        };

        // LoS check: only applies to casts with a specific locked player.
        let los_ok = if cast.target == 0 {
            true
        } else {
            match locked_player {
                Some((_, px, py, pz, alive)) if alive => {
                    has_los(&reg, mob_iid.0, mob_pos.x, mob_pos.y, mob_pos.z, px, py, pz)
                }
                _ => false,
            }
        };

        if !los_ok {
            if let Some(i) = stats.specials.iter().position(|&k| k == cast.ability) {
                if i < cooldowns.specials_cd.len() {
                    cooldowns.specials_cd[i] = ability_stats.foiled_cd;
                }
            }
            commands.entity(mob_entity).remove::<EnemyCast>();
            continue;
        }

        if cast.elapsed < cast.duration { continue; }

        // ── Resolve ────────────────────────────────────────────────────────
        let aim_x = cast.aim_x;
        let aim_z = cast.aim_z;

        match ability_stats.shape {
            AttackShape::Single => {
                if let Some((pent, _, _, _, alive)) = locked_player {
                    if alive {
                        emit_attack_hit(
                            mob_entity, pent, &ability_stats,
                            &mut damage_writer, &mut disruption_writer,
                        );
                    }
                }
            }
            AttackShape::Radius { radius } => {
                let r2 = radius * radius;
                for (pent, _pid, ppos, piid, health) in player_query.iter() {
                    if piid.0 != mob_iid.0 || !health.is_alive() { continue; }
                    let dx = ppos.x - aim_x;
                    let dz = ppos.z - aim_z;
                    if dx * dx + dz * dz <= r2 {
                        emit_attack_hit(
                            mob_entity, pent, &ability_stats,
                            &mut damage_writer, &mut disruption_writer,
                        );
                    }
                }
            }
            AttackShape::Cone { half_angle } => {
                let fwd = Vec3::new(aim_x - mob_pos.x, 0.0, aim_z - mob_pos.z)
                    .normalize_or_zero();
                if fwd == Vec3::ZERO {
                    commands.entity(mob_entity).remove::<EnemyCast>();
                    continue;
                }
                let cos_min = half_angle.cos();
                for (pent, _pid, ppos, piid, health) in player_query.iter() {
                    if piid.0 != mob_iid.0 || !health.is_alive() { continue; }
                    let dx = ppos.x - mob_pos.x;
                    let dz = ppos.z - mob_pos.z;
                    let dist_sq = dx * dx + dz * dz;
                    if dist_sq > ability_stats.range * ability_stats.range { continue; }
                    let to = Vec3::new(dx, 0.0, dz).normalize_or_zero();
                    if fwd.dot(to) >= cos_min {
                        emit_attack_hit(
                            mob_entity, pent, &ability_stats,
                            &mut damage_writer, &mut disruption_writer,
                        );
                    }
                }
            }
        }

        commands.entity(mob_entity).remove::<EnemyCast>();
    }
}

/// Write the damage + disruption pair for one resolved enemy hit. Quality is
/// hard-coded to 1.0 because enemies don't have minigame commit quality —
/// the ability's `damage` field is already the post-quality number.
fn emit_attack_hit(
    attacker: Entity,
    target: Entity,
    stats: &shared::components::combat::AbilityStats,
    damage_writer: &mut MessageWriter<DamageEvent>,
    disruption_writer: &mut MessageWriter<DisruptionEvent>,
) {
    damage_writer.write(DamageEvent {
        attacker,
        target,
        base:        stats.damage,
        ty:          stats.ty,
        additive:    0.0,
        multipliers: 1.0,
        quality:     1.0,
        is_crit:     false,
    });
    disruption_writer.write(DisruptionEvent {
        target,
        profile: stats.disruption,
    });
}
