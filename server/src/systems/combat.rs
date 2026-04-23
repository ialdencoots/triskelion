use std::sync::atomic::{AtomicU64, Ordering};

use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::{AbilityCooldowns, CombatState, DamageType, Health, ReplicatedThreatList, Resistances};
use shared::components::enemy::{EnemyMarker, EnemyPosition};
use shared::components::instance::InstanceId;
use shared::components::minigame::arc::{ArcState, SecondaryArcState};
use shared::components::minigame::cube::{CubeEdge, CubeState};
use shared::components::player::{
    PlayerId, PlayerPosition, PlayerSelectedTarget, PlayerVelocity, RoleStance, SelectedMobOrPlayer,
};
use shared::channels::GameChannel;
use shared::events::combat::DamageEvent;
use shared::inputs::PlayerInput;
use shared::instances::{find_def, sample_height};
use shared::messages::{DamageNumberMsg, SelectTargetMsg};
use shared::settings::PLAYER_FLOAT_HEIGHT;

use super::connection::PlayerEntityLink;
use super::instances::InstanceRegistry;
use super::minigame::{cancel_cube, process_arc_commit, process_cube_collect};
use crate::util::cmp_f32;

/// Wipe per-stance state that shouldn't carry across a stance transition:
/// arc streak counters + quality history (both arcs), and any active cube.
///
/// Called from `process_player_inputs` when the player toggles stance via
/// input, and from `process_instance_requests` when an instance switch drops
/// stance to `None` server-side. Without the second call site, switching
/// instances would leave a mid-flight cube alive (the input handler gates on
/// stance, so it'd be unreachable but still rendered).
pub fn reset_on_stance_change(
    arc: Option<&mut ArcState>,
    secondary: Option<&mut SecondaryArcState>,
    cube: Option<&mut CubeState>,
) {
    if let Some(arc) = arc {
        arc.streak = 0;
        arc.streak_at_last_activation = 0;
        arc.apex_visits_since_commit = 0;
        arc.commit.history.clear();
    }
    if let Some(secondary) = secondary {
        secondary.0.streak = 0;
        secondary.0.streak_at_last_activation = 0;
        secondary.0.apex_visits_since_commit = 0;
        secondary.0.commit.history.clear();
    }
    if let Some(cube) = cube {
        if cube.active {
            cancel_cube(cube);
        }
    }
}

// ── Server-only threat components ────────────────────────────────────────────

/// One entry in a mob's server-side threat table.
#[derive(Clone, Debug)]
pub struct ThreatEntry {
    pub player_entity: Entity,
    pub threat: f32,
}

/// Server-side threat table on mob entities.  Never replicated — use
/// `ReplicatedThreatList` for the client-facing version.
#[derive(Component, Default, Debug)]
pub struct ThreatList {
    pub entries: Vec<ThreatEntry>,
    /// Set whenever entries change; cleared by `sync_replicated_threat_list`.
    pub dirty: bool,
}

/// A time-limited additive threat multiplier bonus from any source.
#[derive(Clone, Debug)]
pub struct ThreatBonus {
    pub multiplier: f32,
    pub expires_at: f32,
}

/// Server-side per-player threat generation modifiers.  Never replicated.
/// `role_multiplier` is derived from the active stance each tick.
/// `bonuses` holds stacking temporary bonuses from any source (cube/grid, skills, etc.)
#[derive(Component, Debug)]
pub struct ThreatModifiers {
    pub role_multiplier: f32,
    pub bonuses: Vec<ThreatBonus>,
}

impl Default for ThreatModifiers {
    fn default() -> Self {
        Self {
            role_multiplier: 0.5,
            bonuses: Vec::new(),
        }
    }
}

impl ThreatModifiers {
    pub fn effective_multiplier(&self) -> f32 {
        let bonus_sum: f32 = self.bonuses.iter().map(|b| b.multiplier).sum();
        self.role_multiplier * (1.0 + bonus_sum)
    }
}

// ── Threat systems ────────────────────────────────────────────────────────────

const PLAYER_SPEED: f32 = 6.0;

/// Read buffered `PlayerInput` messages from all connected clients and apply
/// movement and stance changes to their server-side components.
pub fn process_player_inputs(
    time: Res<Time>,
    reg: Res<InstanceRegistry>,
    mut link_query: Query<(&PlayerEntityLink, &mut MessageReceiver<PlayerInput>)>,
    mut player_query: Query<(
        &mut PlayerPosition,
        &mut PlayerVelocity,
        &mut CombatState,
        Option<&InstanceId>,
        Option<&mut ArcState>,
        Option<&mut SecondaryArcState>,
        Option<&mut CubeState>,
        Option<&PlayerSelectedTarget>,
    ), With<PlayerId>>,
    enemy_query: Query<(&EnemyPosition, Option<&Health>), With<EnemyMarker>>,
    mut damage_writer: MessageWriter<DamageEvent>,
) {
    let dt = time.delta_secs();

    for (link, mut receiver) in link_query.iter_mut() {
        // Use the most recent input in the buffer; ignore stale ones.
        let last_input = receiver.receive().last();
        let Ok((
            mut pos,
            mut vel,
            mut combat,
            iid_opt,
            mut arc_opt,
            mut secondary_arc_opt,
            mut cube_opt,
            target_opt,
        )) = player_query.get_mut(link.0) else { continue };

        if let Some(input) = last_input {
            // ── Movement ───────────────────────────────────────────────────────
            // XYZ position comes directly from the client's physics body so the
            // server position stays in sync with what the client actually simulates.
            // Velocity is derived from the input direction for dead-reckoning by
            // other clients; it is not used to integrate position server-side.
            let raw = Vec3::new(input.movement.x, 0.0, -input.movement.y);
            let dir = if raw.length_squared() > 0.01 { raw.normalize() } else { Vec3::ZERO };

            vel.vx = dir.x * PLAYER_SPEED;
            vel.vz = dir.z * PLAYER_SPEED;
            vel.vy = input.vy;
            pos.x = input.x;
            pos.z = input.z;
            pos.y = input.y;

            // ── Stance transitions ─────────────────────────────────────────────
            let prev_stance = combat.active_stance;
            if input.abilities.exit_stance {
                combat.active_stance = None;
            } else if let Some(stance) = input.abilities.enter_stance {
                // Pressing the active stance's key again toggles back to neutral.
                combat.active_stance = if combat.active_stance == Some(stance) {
                    None
                } else {
                    Some(stance)
                };
            }
            // Any stance change wipes arc streak state — a streak only means
            // something inside a contiguous stance session. Carrying streaks
            // across stances would let players farm Tank cubes and then cash
            // them in under DPS (or vice versa). The cube/grid also cancels:
            // an active cube that outlives its stance is unreachable (the
            // input handlers gate on stance) and would sit there orphaned.
            if prev_stance != combat.active_stance {
                reset_on_stance_change(
                    arc_opt.as_deref_mut(),
                    secondary_arc_opt.as_deref_mut(),
                    cube_opt.as_deref_mut(),
                );
            }

            // ── Arc commits (Physical class) ───────────────────────────────────
            if input.minigame.action_1 {
                debug!("[ARC] action_1 received — stance={:?} arc_present={}",
                    combat.active_stance, arc_opt.is_some());
            }
            if input.minigame.action_1 && combat.active_stance.is_some() {
                if let Some(ref mut arc) = arc_opt {
                    let was_unlocked = !arc.commit.in_lockout;
                    debug!("[ARC] commit attempt — was_unlocked={was_unlocked} quality={:.2} facing_yaw={:.2}",
                        arc.commit.last_quality, input.facing_yaw);
                    process_arc_commit(arc);
                    if was_unlocked {
                        emit_arc_damage(
                            arc.commit.last_quality,
                            input.facing_yaw,
                            link.0,
                            &pos,
                            target_opt,
                            &enemy_query,
                            &mut damage_writer,
                        );
                    }
                }
            }
            if input.minigame.action_2 && combat.active_stance == Some(RoleStance::Dps) {
                if let Some(ref mut secondary) = secondary_arc_opt {
                    let was_unlocked = !secondary.0.commit.in_lockout;
                    process_arc_commit(&mut secondary.0);
                    if was_unlocked {
                        emit_arc_damage(
                            secondary.0.commit.last_quality,
                            input.facing_yaw,
                            link.0,
                            &pos,
                            target_opt,
                            &enemy_query,
                            &mut damage_writer,
                        );
                    }
                }
            }

            // ── Cube collects (Tank/Heal only) ─────────────────────────────────
            // Client binds A/X/G → action_3/4/5 for Left/Bottom/Right; action_2
            // stays dedicated to the DPS secondary arc so slots never overlap.
            let in_tank_heal = matches!(
                combat.active_stance,
                Some(RoleStance::Tank) | Some(RoleStance::Heal)
            );
            if in_tank_heal {
                if let Some(ref mut cube) = cube_opt {
                    for (pressed, edge) in [
                        (input.minigame.action_3, CubeEdge::Left),
                        (input.minigame.action_4, CubeEdge::Bottom),
                        (input.minigame.action_5, CubeEdge::Right),
                    ] {
                        if pressed {
                            process_cube_collect(cube, edge);
                        }
                    }
                }
            }
        } else {
            // No input this tick — zero XZ motion.
            vel.vx = 0.0;
            vel.vz = 0.0;
            vel.vy = 0.0;
            let floor_y = if let Some(iid) = iid_opt {
                if let Some(live) = reg.instances.get(&iid.0) {
                    let def = find_def(live.kind);
                    sample_height(&live.noise, pos.x, pos.z, &def.terrain) + PLAYER_FLOAT_HEIGHT
                } else {
                    pos.y
                }
            } else {
                pos.y
            };
            if pos.y <= floor_y + 0.1 {
                pos.y = floor_y;
            }
        }
    }
}

/// Reads `SelectTargetMsg` from each client and updates `PlayerSelectedTarget`
/// on their player entity.
pub fn process_target_selections(
    mut link_query: Query<(&PlayerEntityLink, &mut MessageReceiver<SelectTargetMsg>)>,
    mut player_query: Query<&mut PlayerSelectedTarget>,
) {
    for (link, mut receiver) in link_query.iter_mut() {
        if let Some(msg) = receiver.receive().last() {
            if let Ok(mut target) = player_query.get_mut(link.0) {
                target.0 = msg.0;
            }
        }
    }
}

/// Decrement all ability cooldowns by the elapsed fixed-timestep delta.
pub fn tick_ability_cooldowns(time: Res<Time>, mut query: Query<&mut AbilityCooldowns>) {
    let dt = time.delta_secs();
    for mut cd in query.iter_mut() {
        cd.mobility_cd = (cd.mobility_cd - dt).max(0.0);
        cd.cc_cd = (cd.cc_cd - dt).max(0.0);
        cd.taunt_cd = (cd.taunt_cd - dt).max(0.0);
        cd.interrupt_cd = (cd.interrupt_cd - dt).max(0.0);
        cd.stance_cd = (cd.stance_cd - dt).max(0.0);
    }
}

/// Derives each player's threat `role_multiplier` from their active stance and
/// prunes any expired timed bonuses.
pub fn apply_stance_multipliers(
    time: Res<Time>,
    mut query: Query<(&CombatState, &mut ThreatModifiers)>,
) {
    let now = time.elapsed_secs();
    for (combat, mut modifiers) in query.iter_mut() {
        modifiers.role_multiplier = match combat.active_stance {
            Some(RoleStance::Tank) => 2.0,
            Some(RoleStance::Dps)  => 1.0,
            Some(RoleStance::Heal) => 0.0,
            None                   => 0.5,
        };
        modifiers.bonuses.retain(|b| b.expires_at == 0.0 || b.expires_at > now);
    }
}

/// Syncs each mob's server-side `ThreatList` to its replicated `ReplicatedThreatList`,
/// translating internal `Entity` keys to `PlayerId` (u64) for clients.
/// Entries are sorted descending by threat.
pub fn sync_replicated_threat_list(
    mut mob_query: Query<(&mut ThreatList, &mut ReplicatedThreatList)>,
    player_query: Query<(Entity, &PlayerId)>,
) {
    for (mut threat_list, mut replicated) in mob_query.iter_mut() {
        if !threat_list.dirty { continue; }
        threat_list.dirty = false;
        let mut entries: Vec<(u64, f32)> = threat_list.entries.iter()
            .filter_map(|entry| {
                player_query.get(entry.player_entity).ok()
                    .map(|(_, pid)| (pid.0, entry.threat))
            })
            .collect();
        entries.sort_by(|a, b| cmp_f32(b.1, a.1));
        replicated.entries = entries;
    }
}

/// Adds `amount` threat for `player` on `list`, creating an entry if needed.
fn add_threat(list: &mut ThreatList, player: Entity, amount: f32) {
    if let Some(entry) = list.entries.iter_mut().find(|e| e.player_entity == player) {
        entry.threat += amount;
    } else {
        list.entries.push(ThreatEntry { player_entity: player, threat: amount });
    }
    list.dirty = true;
}

/// Apply threat to a mob from a damage event.
/// Called by the damage system when it is implemented.
pub fn apply_damage_threat(
    threat_list: &mut ThreatList,
    attacker: Entity,
    damage: f32,
    modifiers: &ThreatModifiers,
) {
    add_threat(threat_list, attacker, damage * modifiers.effective_multiplier());
}

// ── Combat RNG ───────────────────────────────────────────────────────────────

/// Monotonic counter feeding the combat RNG. Any roll (crit, future: miss,
/// variance) should draw from `next_unit`.
static COMBAT_RNG: AtomicU64 = AtomicU64::new(0xDEADBEEF_CAFEBABE);

/// Returns a pseudo-random f32 in [0.0, 1.0). SplitMix64 scramble — cheap,
/// non-cryptographic, good enough for gameplay rolls. Not deterministic across
/// runs (counter starts from a fixed seed but order of calls varies).
pub(crate) fn next_unit() -> f32 {
    let seed = COMBAT_RNG.fetch_add(1, Ordering::Relaxed);
    let mut x = seed.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 31;
    (x as u32) as f32 / (u32::MAX as f32 + 1.0)
}

/// Quality at/above which a Physical arc commit can roll a crit.
const CRIT_NADIR_THRESHOLD: f32 = 0.85;
/// Max crit chance for quality strictly below 1.0 (the shoulder). Quality of
/// exactly 1.0 still crits 100% of the time (handled in `crit_chance`).
const CRIT_SHOULDER_CHANCE: f32 = 0.25;
/// Damage multiplier applied when a hit crits.
const CRIT_MULTIPLIER: f32 = 2.0;

/// Quality→crit-chance curve: linear 0%→65% over [0.85, 1.0), then a hard jump
/// to 100% at exactly 1.0. Rewards precision with a discontinuity at perfect.
fn crit_chance(quality: f32) -> f32 {
    if quality >= 1.0 {
        1.0
    } else if quality >= CRIT_NADIR_THRESHOLD {
        (quality - CRIT_NADIR_THRESHOLD) / (1.0 - CRIT_NADIR_THRESHOLD) * CRIT_SHOULDER_CHANCE
    } else {
        0.0
    }
}

// ── Arc damage emission ──────────────────────────────────────────────────────

/// Gates an arc commit against range/facing/alive checks and, on pass, emits a
/// `DamageEvent` for the resolver to apply. Does not touch HP or threat here.
fn emit_arc_damage(
    quality: f32,
    facing_yaw: f32,
    attacker: Entity,
    pos: &PlayerPosition,
    target: Option<&PlayerSelectedTarget>,
    enemy_query: &Query<(&EnemyPosition, Option<&Health>), With<EnemyMarker>>,
    damage_writer: &mut MessageWriter<DamageEvent>,
) {
    const BASE_DAMAGE: f32 = 15.0;
    /// Melee reach in world units.
    const MELEE_RANGE: f32 = 4.0;
    // Allow commits within a 120° cone (cos 60° = 0.5) — forgiving but not trivial.
    const FACING_COS_MIN: f32 = 0.5;

    let mob_entity = match target.and_then(|t| t.0.as_ref()) {
        Some(SelectedMobOrPlayer::Mob(e)) => *e,
        other => {
            debug!("[ARC DMG] no mob target — target={other:?}");
            return;
        }
    };

    let Ok((enemy_pos, health_opt)) = enemy_query.get(mob_entity) else {
        debug!("[ARC DMG] {mob_entity:?} not in enemy_query — missing EnemyMarker or EnemyPosition");
        return;
    };
    if let Some(h) = health_opt {
        if !h.is_alive() {
            debug!("[ARC DMG] target already dead");
            return;
        }
    }

    // Distance and facing checks in the XZ plane.
    let player_xz = Vec3::new(pos.x, 0.0, pos.z);
    let enemy_xz  = Vec3::new(enemy_pos.x, 0.0, enemy_pos.z);
    let delta      = enemy_xz - player_xz;
    let dist       = delta.length();

    if dist > MELEE_RANGE {
        debug!("[ARC DMG] out of range — dist={dist:.2} max={MELEE_RANGE}");
        return;
    }

    let to_target = delta.normalize_or_zero();
    if to_target == Vec3::ZERO {
        debug!("[ARC DMG] player standing on mob");
        return;
    }

    let forward = Quat::from_rotation_y(facing_yaw) * Vec3::NEG_Z;
    let dot = forward.dot(to_target);
    if dot < FACING_COS_MIN {
        debug!("[ARC DMG] not facing target — dot={dot:.2} min={FACING_COS_MIN}");
        return;
    }

    let is_crit = next_unit() < crit_chance(quality);
    let multipliers = if is_crit { CRIT_MULTIPLIER } else { 1.0 };

    damage_writer.write(DamageEvent {
        attacker,
        target: mob_entity,
        base: BASE_DAMAGE,
        ty: DamageType::Physical,
        additive: 0.0,
        multipliers,
        quality,
        is_crit,
    });
}

// ── Damage resolution ────────────────────────────────────────────────────────

/// Pure formula. Clamped non-negative.
/// `final = base × quality × (1 + additive) × multipliers × (1 − resist)`
fn resolve_damage(ev: &DamageEvent, resist: f32) -> f32 {
    let raw = ev.base
        * ev.quality
        * (1.0 + ev.additive)
        * ev.multipliers
        * (1.0 - resist);
    raw.max(0.0)
}

/// Consumes every queued `DamageEvent` and applies it: HP subtraction against
/// the target's `Resistances`, threat credit to the attacker's list using
/// their `ThreatModifiers`, and a broadcast `DamageNumberMsg` so every client
/// can pop a floating number over the target.
pub fn apply_damage_events(
    mut messages: MessageReader<DamageEvent>,
    mut target_query: Query<(&mut Health, &Resistances, Option<&mut ThreatList>, Option<&InstanceId>), With<EnemyMarker>>,
    threat_mods_query: Query<&ThreatModifiers>,
    attacker_instance_query: Query<&InstanceId>,
    mut number_senders: Query<(&PlayerEntityLink, &mut MessageSender<DamageNumberMsg>)>,
) {
    for ev in messages.read() {
        let Ok((mut health, resist, threat_opt, target_inst)) = target_query.get_mut(ev.target) else {
            info!("[DMG] target {:?} missing Health/Resistances (or not an enemy)", ev.target);
            continue;
        };
        if !health.is_alive() { continue; }

        let r = resist.get(ev.ty);
        let final_dmg = resolve_damage(ev, r);
        health.current = (health.current - final_dmg).max(0.0);

        debug!(
            "[DMG] {:?}->{:?} ty={:?} base={:.1} q={:.2} +{:.2} x{:.2} resist={:.2} final={:.1} hp={:.1}/{:.1}",
            ev.attacker, ev.target, ev.ty, ev.base, ev.quality, ev.additive, ev.multipliers,
            r, final_dmg, health.current, health.max,
        );

        match threat_opt {
            None => {
                warn!("[DMG] target {:?} has no ThreatList — skipping threat", ev.target);
            }
            Some(mut tl) => {
                match threat_mods_query.get(ev.attacker) {
                    Err(_) => {
                        warn!("[DMG] attacker {:?} has no ThreatModifiers — skipping threat", ev.attacker);
                    }
                    Ok(mods) => {
                        apply_damage_threat(&mut tl, ev.attacker, final_dmg, mods);
                    }
                }
            }
        }

        // Floating damage numbers are a personal cue — send only to the
        // attacker's client. Other players will see the hit through the
        // forthcoming combat log, not as a world-space popup.
        //
        // Suppress the number when attacker and target are in different
        // instances (e.g. a DoT the attacker applied before leaving the
        // instance is still ticking on the old target). Without this the
        // attacker keeps seeing damage numbers pop from enemies in a scene
        // they no longer occupy.
        let attacker_inst = attacker_instance_query.get(ev.attacker).ok().map(|i| i.0);
        let target_inst = target_inst.map(|i| i.0);
        if let (Some(a), Some(t)) = (attacker_inst, target_inst) {
            if a != t {
                continue;
            }
        }

        let payload = DamageNumberMsg {
            target: ev.target,
            amount: final_dmg,
            ty: ev.ty,
            is_crit: ev.is_crit,
        };
        for (link, mut sender) in number_senders.iter_mut() {
            if link.0 == ev.attacker {
                sender.send::<GameChannel>(payload.clone());
                break;
            }
        }
    }
}

// ── Damage over time ─────────────────────────────────────────────────────────

/// One active DoT entry on a target. Fires `remaining_ticks` times at
/// `interval`-second spacing, each firing a [`DamageEvent`] of `per_tick` base
/// damage with `ty` and no modifier stack (quality 1.0).
#[derive(Clone, Debug)]
pub struct DamageOverTime {
    pub source: Entity,
    pub ty: DamageType,
    pub per_tick: f32,
    pub interval: f32,
    pub remaining_ticks: u32,
    pub since_last: f32,
}

/// Server-only stack of active DoTs on an entity. Each entry ticks independently.
#[derive(Component, Default, Debug)]
pub struct DamageOverTimes(pub Vec<DamageOverTime>);

/// Advances every DoT stack, emitting a [`DamageEvent`] for each entry that
/// crosses its interval boundary. Removes entries that have fired their last tick.
pub fn tick_dots(
    time: Res<Time>,
    mut targets: Query<(Entity, &mut DamageOverTimes)>,
    mut damage_writer: MessageWriter<DamageEvent>,
) {
    let dt = time.delta_secs();
    for (target_entity, mut dots) in targets.iter_mut() {
        dots.0.retain_mut(|dot| {
            dot.since_last += dt;
            if dot.since_last >= dot.interval && dot.remaining_ticks > 0 {
                dot.since_last -= dot.interval;
                dot.remaining_ticks -= 1;
                damage_writer.write(DamageEvent::hit(
                    dot.source, target_entity, dot.per_tick, dot.ty, 1.0,
                ));
            }
            dot.remaining_ticks > 0
        });
    }
}

/// Distribute healing threat across all engaged mobs in an instance.
/// Called when a heal is applied. Rate: 0.4 per point healed, full amount per mob.
pub fn distribute_healing_threat(
    healer: Entity,
    heal_amount: f32,
    instance_id: u32,
    mob_query: &mut Query<(&mut ThreatList, &InstanceId)>,
) {
    let generated = heal_amount * 0.4;
    for (mut threat_list, mob_iid) in mob_query.iter_mut() {
        if mob_iid.0 != instance_id || threat_list.entries.is_empty() {
            continue;
        }
        add_threat(&mut threat_list, healer, generated);
    }
}
