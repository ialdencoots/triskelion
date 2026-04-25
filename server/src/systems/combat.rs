use std::sync::atomic::{AtomicU64, Ordering};

use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::{AbilityCooldowns, CombatState, DamageType, DisruptionKind, Health, ReplicatedThreatList, Resistances};
use shared::components::enemy::{EnemyMarker, EnemyName, EnemyPosition};
use shared::components::instance::InstanceId;
use shared::components::minigame::arc::{ArcState, SecondaryArcState};
use shared::components::minigame::bar_fill::BarFillState;
use shared::components::minigame::cube::{CubeEdge, CubeState};
use shared::components::minigame::grid::{DpsGridTrigger, GridDir, GridState, MIN_GRID_BUDGET};
use shared::components::minigame::heartbeat::HeartbeatState;
use shared::components::player::{
    GroupId, PlayerId, PlayerName, PlayerPosition, PlayerSelectedTarget, PlayerVelocity, RoleStance, SelectedMobOrPlayer,
};
use shared::channels::GameChannel;
use shared::events::combat::{DamageEvent, DisruptionEvent};
use shared::inputs::PlayerInput;
use shared::instances::{find_def, terrain_surface_y};
use shared::messages::{CombatLogMsg, DamageNumberMsg, SelectTargetMsg};
use shared::settings::PLAYER_FLOAT_HEIGHT;

use super::connection::PlayerEntityLink;
use super::instances::InstanceRegistry;
use super::minigame::{
    cancel_cube, cancel_grid, process_arc_commit, process_cube_collect, process_grid_move,
};
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
    grid: Option<&mut GridState>,
    grid_trigger: Option<&mut DpsGridTrigger>,
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
    if let Some(grid) = grid {
        if grid.active {
            cancel_grid(grid);
        }
    }
    if let Some(trigger) = grid_trigger {
        trigger.clear();
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
        Option<&mut GridState>,
        Option<&mut DpsGridTrigger>,
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
            mut grid_opt,
            mut grid_trigger_opt,
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
                    grid_opt.as_deref_mut(),
                    grid_trigger_opt.as_deref_mut(),
                );
            }

            // ── Grid routing or arc commits ────────────────────────────────────
            //
            // While the DPS grid overlay is active, both arcs' commit input is
            // suspended (their oscillation continues to tick) and the
            // directional secondary slots route into the grid instead.
            //
            // Otherwise normal arc commits flow, with DPS apex commits
            // additionally triggering cross-arc break detection: if the apex
            // drops `shared_delta` from ≥ MIN_GRID_BUDGET to < MIN_GRID_BUDGET,
            // we stash the budget + winning-arc history on `DpsGridTrigger`
            // for `tick_grid_states` to consume next tick.
            let grid_active = grid_opt.as_deref().map(|g| g.active).unwrap_or(false);
            let is_dps = combat.active_stance == Some(RoleStance::Dps);

            if grid_active && is_dps {
                if let Some(ref mut grid) = grid_opt {
                    // Priority: Right > Up > Down > Left (forward bias).
                    let dir = if input.minigame.action_5 {
                        Some(GridDir::Right)
                    } else if input.minigame.action_6 {
                        Some(GridDir::Up)
                    } else if input.minigame.action_4 {
                        Some(GridDir::Down)
                    } else if input.minigame.action_3 {
                        Some(GridDir::Left)
                    } else {
                        None
                    };
                    if let Some(d) = dir {
                        process_grid_move(grid, d);
                    }
                }
            } else {
                if input.minigame.action_1 {
                    debug!("[ARC] action_1 received — stance={:?} arc_present={}",
                        combat.active_stance, arc_opt.is_some());
                }
                if input.minigame.action_1 && combat.active_stance.is_some() {
                    if let Some(ref mut arc) = arc_opt {
                        let was_unlocked = !arc.commit.in_lockout;
                        debug!("[ARC] commit attempt — was_unlocked={was_unlocked} quality={:.2} facing_yaw={:.2}",
                            arc.commit.last_quality, input.facing_yaw);

                        // Pre-commit: capture cross-arc state for DPS break detection.
                        let (pre_shared, apex_was_winning) = if is_dps {
                            if let Some(sec) = secondary_arc_opt.as_deref() {
                                let pa = arc.streak.saturating_sub(arc.streak_at_last_activation);
                                let ps = sec.0.streak.saturating_sub(sec.0.streak_at_last_activation);
                                // Primary wins ties (apex_arc == primary here).
                                (pa.max(ps), pa >= ps)
                            } else {
                                let pa = arc.streak.saturating_sub(arc.streak_at_last_activation);
                                (pa, true)
                            }
                        } else {
                            (0u32, false)
                        };

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

                        // Post-commit: detect break trigger (DPS only). The
                        // apex push has already happened inside
                        // `process_arc_commit`, so `arc.commit.history` is the
                        // correct snapshot when the apex arc was the winning
                        // arc — the breaking apex's quality feeds step 1 with
                        // a low magnitude per the design doc.
                        if is_dps {
                            if let (Some(sec), Some(trig)) = (
                                secondary_arc_opt.as_deref(),
                                grid_trigger_opt.as_deref_mut(),
                            ) {
                                let pa = arc.streak.saturating_sub(arc.streak_at_last_activation);
                                let ps = sec.0.streak.saturating_sub(sec.0.streak_at_last_activation);
                                let post_shared = pa.max(ps);
                                if pre_shared >= MIN_GRID_BUDGET && post_shared < MIN_GRID_BUDGET {
                                    let history = if apex_was_winning {
                                        arc.commit.history.clone()
                                    } else {
                                        sec.0.commit.history.clone()
                                    };
                                    trig.pending_break_budget = Some(pre_shared);
                                    trig.pending_break_history = Some(history);
                                }
                            }
                        }
                    }
                }
                if input.minigame.action_2 && is_dps {
                    if let Some(ref mut secondary) = secondary_arc_opt {
                        let was_unlocked = !secondary.0.commit.in_lockout;

                        let (pre_shared, apex_was_winning) =
                            if let Some(arc_ref) = arc_opt.as_deref() {
                                let pa = arc_ref.streak.saturating_sub(arc_ref.streak_at_last_activation);
                                let ps = secondary.0.streak.saturating_sub(secondary.0.streak_at_last_activation);
                                // Primary wins ties; apex_arc is secondary, so winning_is_apex == ps > pa.
                                (pa.max(ps), ps > pa)
                            } else {
                                let ps = secondary.0.streak.saturating_sub(secondary.0.streak_at_last_activation);
                                (ps, true)
                            };

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

                        if let (Some(arc_ref), Some(trig)) = (
                            arc_opt.as_deref(),
                            grid_trigger_opt.as_deref_mut(),
                        ) {
                            let pa = arc_ref.streak.saturating_sub(arc_ref.streak_at_last_activation);
                            let ps = secondary.0.streak.saturating_sub(secondary.0.streak_at_last_activation);
                            let post_shared = pa.max(ps);
                            if pre_shared >= MIN_GRID_BUDGET && post_shared < MIN_GRID_BUDGET {
                                let history = if apex_was_winning {
                                    secondary.0.commit.history.clone()
                                } else {
                                    arc_ref.commit.history.clone()
                                };
                                trig.pending_break_budget = Some(pre_shared);
                                trig.pending_break_history = Some(history);
                            }
                        }
                    }
                }

                // ── Cube collects (Tank/Heal only) ─────────────────────────
                // Client binds A/X/G → action_3/4/5 for Left/Bottom/Right;
                // action_2 stays dedicated to the DPS secondary arc so slots
                // never overlap.
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
            }
        } else {
            // No input this tick — zero XZ motion.
            vel.vx = 0.0;
            vel.vz = 0.0;
            vel.vy = 0.0;
            let floor_y = if let Some(iid) = iid_opt {
                if let Some(live) = reg.instances.get(&iid.0) {
                    let def = find_def(live.kind);
                    terrain_surface_y(&live.noise, pos.x, pos.z, def) + PLAYER_FLOAT_HEIGHT
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

/// Consumes every queued `DamageEvent` and routes it by target kind. An event
/// whose target is an enemy runs the player→enemy path (damage + threat credit +
/// damage-number to attacker). An event whose target is a player runs the
/// enemy→player path (damage only — no threat, no number yet). Friendly-fire
/// is rejected: player→player and enemy→enemy events are dropped silently.
///
/// The `Without` filters on the two target queries make them provably disjoint
/// so `&mut Health` on both compiles without a runtime conflict.
pub fn apply_damage_events(
    mut messages: MessageReader<DamageEvent>,
    mut enemy_targets: Query<
        (&mut Health, &Resistances, &mut ThreatList, Option<&InstanceId>),
        (With<EnemyMarker>, Without<PlayerId>),
    >,
    mut player_targets: Query<
        (&mut Health, Option<&Resistances>, Option<&InstanceId>),
        (With<PlayerId>, Without<EnemyMarker>),
    >,
    threat_mods_query: Query<&ThreatModifiers>,
    attacker_kind_query: Query<(Option<&PlayerId>, Option<&EnemyMarker>, Option<&InstanceId>)>,
    player_name_query: Query<&PlayerName>,
    enemy_name_query: Query<&EnemyName>,
    group_query: Query<&GroupId>,
    mut number_senders: Query<(&PlayerEntityLink, &mut MessageSender<DamageNumberMsg>)>,
    mut log_senders: Query<(&PlayerEntityLink, &mut MessageSender<CombatLogMsg>)>,
) {
    for ev in messages.read() {
        let (atk_is_player, atk_is_enemy, atk_inst) = match attacker_kind_query.get(ev.attacker) {
            Ok((pid, em, inst)) => (pid.is_some(), em.is_some(), inst.map(|i| i.0)),
            Err(_) => (false, false, None),
        };

        // ── Player → Enemy ─────────────────────────────────────────────────
        if let Ok((mut health, resist, mut threat_list, target_inst))
            = enemy_targets.get_mut(ev.target)
        {
            if !atk_is_player {
                // Enemy-on-enemy isn't a feature yet; drop silently.
                continue;
            }
            if !health.is_alive() { continue; }

            let r = resist.get(ev.ty);
            let final_dmg = resolve_damage(ev, r);
            health.current = (health.current - final_dmg).max(0.0);

            debug!(
                "[DMG] P->E {:?}->{:?} ty={:?} base={:.1} q={:.2} +{:.2} x{:.2} resist={:.2} final={:.1} hp={:.1}/{:.1}",
                ev.attacker, ev.target, ev.ty, ev.base, ev.quality, ev.additive, ev.multipliers,
                r, final_dmg, health.current, health.max,
            );

            if let Ok(mods) = threat_mods_query.get(ev.attacker) {
                apply_damage_threat(&mut threat_list, ev.attacker, final_dmg, mods);
            } else {
                warn!("[DMG] attacker {:?} has no ThreatModifiers — skipping threat", ev.attacker);
            }

            broadcast_combat_log(
                ev.attacker, ev.target, final_dmg, ev.ty, ev.is_crit, true,
                &player_name_query, &enemy_name_query, &group_query, &mut log_senders,
            );

            // Damage numbers: personal cue to the attacker only. Suppress
            // cross-instance (a DoT applied before leaving keeps ticking on
            // the old target; we don't want stale numbers popping in the
            // attacker's new scene).
            let tinst = target_inst.map(|i| i.0);
            if let (Some(a), Some(t)) = (atk_inst, tinst) {
                if a != t { continue; }
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
            continue;
        }

        // ── Enemy → Player ─────────────────────────────────────────────────
        if let Ok((mut health, resist_opt, _target_inst))
            = player_targets.get_mut(ev.target)
        {
            if !atk_is_enemy {
                // Friendly fire (player→player) — reject.
                continue;
            }
            if !health.is_alive() { continue; }

            let r = resist_opt.map(|r| r.get(ev.ty)).unwrap_or(0.0);
            let final_dmg = resolve_damage(ev, r);
            health.current = (health.current - final_dmg).max(0.0);

            debug!(
                "[DMG] E->P {:?}->{:?} ty={:?} base={:.1} resist={:.2} final={:.1} hp={:.1}/{:.1}",
                ev.attacker, ev.target, ev.ty, ev.base, r, final_dmg, health.current, health.max,
            );

            broadcast_combat_log(
                ev.attacker, ev.target, final_dmg, ev.ty, ev.is_crit, false,
                &player_name_query, &enemy_name_query, &group_query, &mut log_senders,
            );
            continue;
        }

        // Target is neither a live enemy nor a live player entity. Most
        // likely a stale reference (despawned mid-tick). Drop silently.
    }
}

/// Resolve an entity to a display name. Players carry `PlayerName`, enemies
/// carry `EnemyName`. Falls back to "?" for entities with neither — shouldn't
/// happen for real damage sources but defensible for stale references.
fn display_name(
    e: Entity,
    players: &Query<&PlayerName>,
    enemies: &Query<&EnemyName>,
) -> String {
    if let Ok(n) = players.get(e) { return n.0.clone(); }
    if let Ok(n) = enemies.get(e) { return n.0.clone(); }
    "?".to_string()
}

/// Send a `CombatLogMsg` to every player whose `GroupId` matches either the
/// attacker's or the target's group. Enemies have no `GroupId`, so an
/// enemy-attacker / enemy-target contributes nothing from its own side — but
/// as long as the *other* side is a player, that side's whole party receives
/// the log entry.
fn broadcast_combat_log(
    attacker: Entity,
    target: Entity,
    amount: f32,
    ty: DamageType,
    is_crit: bool,
    attacker_is_player: bool,
    player_names: &Query<&PlayerName>,
    enemy_names: &Query<&EnemyName>,
    groups: &Query<&GroupId>,
    senders: &mut Query<(&PlayerEntityLink, &mut MessageSender<CombatLogMsg>)>,
) {
    let a_group = groups.get(attacker).ok().map(|g| g.0);
    let t_group = groups.get(target).ok().map(|g| g.0);
    if a_group.is_none() && t_group.is_none() { return; }

    let payload = CombatLogMsg {
        attacker_name: display_name(attacker, player_names, enemy_names),
        target_name:   display_name(target,   player_names, enemy_names),
        amount,
        ty,
        is_crit,
        attacker_is_player,
    };

    for (link, mut sender) in senders.iter_mut() {
        if let Ok(g) = groups.get(link.0) {
            if Some(g.0) == a_group || Some(g.0) == t_group {
                sender.send::<GameChannel>(payload.clone());
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

// ── Disruption resolution ────────────────────────────────────────────────────

/// Arc `disruption_velocity` per unit of Spike magnitude — tuned so a small
/// auto-attack (magnitude 0.15) produces a perceptible wobble on the dot and
/// a full GroundSlam (magnitude 0.9) briefly throws it off rhythm.
const SPIKE_ARC: f32 = 3.0;
/// Arc drift per unit of Sustained magnitude. Smaller than Spike — sustained
/// disruption is meant to be an ambient noise floor, not a punch.
const DRIFT_ARC: f32 = 0.8;
/// Heartbeat frequency spike (Hz) per unit of Spike magnitude.
const SPIKE_HB: f32 = 0.6;
/// Heartbeat envelope noise amplitude per unit of Sustained magnitude.
const NOISE_HB: f32 = 0.25;
/// Bar-fill drain per unit of magnitude (both Spike and Sustained — drain
/// is a single mechanic; magnitude gates size).
const DRAIN_BF: f32 = 0.35;

/// Consume every queued `DisruptionEvent` and apply it to whichever minigame
/// component the target currently has. Each player's active stance only has
/// one primary minigame component, so at most one branch fires per event.
/// Arc's `disruption_velocity` sign is intentionally unchanged (additive) —
/// accumulates counter-directional impulses.
pub fn apply_disruption_events(
    mut messages: MessageReader<DisruptionEvent>,
    mut arcs:       Query<&mut ArcState>,
    mut heartbeats: Query<&mut HeartbeatState>,
    mut bars:       Query<&mut BarFillState>,
) {
    for ev in messages.read() {
        let mag = ev.profile.magnitude;
        if let Ok(mut arc) = arcs.get_mut(ev.target) {
            match ev.profile.kind {
                DisruptionKind::Spike     => arc.disruption_velocity += SPIKE_ARC * mag,
                DisruptionKind::Sustained => arc.disruption_velocity += DRIFT_ARC * mag,
            }
        }
        if let Ok(mut hb) = heartbeats.get_mut(ev.target) {
            match ev.profile.kind {
                DisruptionKind::Spike     => hb.frequency_spike += SPIKE_HB * mag,
                DisruptionKind::Sustained => hb.envelope_noise  += NOISE_HB * mag,
            }
        }
        if let Ok(mut bf) = bars.get_mut(ev.target) {
            bf.drain_pending += DRAIN_BF * mag;
        }
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

#[cfg(test)]
mod tests {
    //! Pure-function coverage for the damage formula and threat math.
    //!
    //! `apply_damage_events` itself is not tested here: it consumes
    //! Lightyear `MessageReader<DamageEvent>` and writes through
    //! `MessageSender<…>`, which would require standing up the Lightyear
    //! plugin stack to exercise. The gameplay-relevant logic — the formula,
    //! the crit curve, threat accumulation — is all in the pure helpers
    //! tested below; the system function itself is plumbing that breaks
    //! loudly (compile errors) rather than silently.
    use super::*;

    fn dmg(base: f32, quality: f32, additive: f32, multipliers: f32) -> DamageEvent {
        DamageEvent {
            attacker: Entity::PLACEHOLDER,
            target: Entity::PLACEHOLDER,
            base,
            ty: DamageType::Physical,
            additive,
            multipliers,
            quality,
            is_crit: false,
        }
    }

    // ── resolve_damage ──────────────────────────────────────────────────────

    #[test]
    fn resolve_damage_baseline() {
        // base=100, quality=1, no additive, no multiplier, no resist → 100.
        let ev = dmg(100.0, 1.0, 0.0, 1.0);
        assert_eq!(resolve_damage(&ev, 0.0), 100.0);
    }

    #[test]
    fn resolve_damage_applies_quality_linearly() {
        let ev = dmg(100.0, 0.5, 0.0, 1.0);
        assert_eq!(resolve_damage(&ev, 0.0), 50.0);
    }

    #[test]
    fn resolve_damage_applies_additive_then_multipliers() {
        // 100 × 1.0 × (1 + 0.5) × 2.0 = 300.
        let ev = dmg(100.0, 1.0, 0.5, 2.0);
        assert_eq!(resolve_damage(&ev, 0.0), 300.0);
    }

    #[test]
    fn resolve_damage_applies_resist() {
        // 100 × (1 - 0.25) = 75.
        let ev = dmg(100.0, 1.0, 0.0, 1.0);
        assert_eq!(resolve_damage(&ev, 0.25), 75.0);
    }

    #[test]
    fn resolve_damage_full_formula() {
        // base=80, q=0.8, +0.25, ×1.5, resist 0.2:
        // 80 × 0.8 × 1.25 × 1.5 × 0.8 = 96.0
        let ev = dmg(80.0, 0.8, 0.25, 1.5);
        let got = resolve_damage(&ev, 0.2);
        assert!((got - 96.0).abs() < 1e-4, "got {got}");
    }

    #[test]
    fn resolve_damage_clamps_to_zero_under_negative_multiplier() {
        // Defensive: a negative multiplier (e.g. a buggy debuff stack) must
        // never heal the target by going below zero damage.
        let ev = dmg(100.0, 1.0, 0.0, -1.0);
        assert_eq!(resolve_damage(&ev, 0.0), 0.0);
    }

    // ── crit_chance ─────────────────────────────────────────────────────────

    #[test]
    fn crit_chance_below_threshold_is_zero() {
        assert_eq!(crit_chance(0.0), 0.0);
        assert_eq!(crit_chance(0.84), 0.0);
    }

    #[test]
    fn crit_chance_at_threshold_is_zero_and_ramps_linearly() {
        // At exactly the threshold the curve starts at 0%.
        assert!(crit_chance(0.85).abs() < 1e-5);
        // Midpoint of the shoulder → half of CRIT_SHOULDER_CHANCE.
        let mid = 0.85 + (1.0 - 0.85) / 2.0;
        assert!((crit_chance(mid) - CRIT_SHOULDER_CHANCE / 2.0).abs() < 1e-5);
    }

    #[test]
    fn crit_chance_at_perfect_quality_is_one() {
        // The discontinuity at q=1.0 is intentional — perfect commits always crit.
        assert_eq!(crit_chance(1.0), 1.0);
    }

    // ── Threat math ─────────────────────────────────────────────────────────

    #[test]
    fn effective_multiplier_with_no_bonuses_is_role_multiplier() {
        let m = ThreatModifiers { role_multiplier: 2.0, bonuses: vec![] };
        assert_eq!(m.effective_multiplier(), 2.0);
    }

    #[test]
    fn effective_multiplier_sums_bonuses_additively() {
        // role × (1 + Σbonuses) = 2.0 × (1 + 0.5 + 0.25) = 3.5
        let m = ThreatModifiers {
            role_multiplier: 2.0,
            bonuses: vec![
                ThreatBonus { multiplier: 0.5, expires_at: 0.0 },
                ThreatBonus { multiplier: 0.25, expires_at: 0.0 },
            ],
        };
        assert!((m.effective_multiplier() - 3.5).abs() < 1e-5);
    }

    #[test]
    fn add_threat_creates_then_accumulates_and_marks_dirty() {
        let mut list = ThreatList::default();
        let a = Entity::from_raw_u32(1).unwrap();
        let b = Entity::from_raw_u32(2).unwrap();
        add_threat(&mut list, a, 10.0);
        assert_eq!(list.entries.len(), 1);
        assert_eq!(list.entries[0].threat, 10.0);
        assert!(list.dirty);
        list.dirty = false;

        add_threat(&mut list, a, 5.0);
        add_threat(&mut list, b, 7.0);
        assert_eq!(list.entries.len(), 2);
        let entry_a = list.entries.iter().find(|e| e.player_entity == a).unwrap();
        assert_eq!(entry_a.threat, 15.0);
        assert!(list.dirty);
    }

    #[test]
    fn apply_damage_threat_credits_attacker_with_role_scaled_amount() {
        let mut list = ThreatList::default();
        let attacker = Entity::from_raw_u32(42).unwrap();
        let mods = ThreatModifiers {
            role_multiplier: 2.0, // Tank
            bonuses: vec![],
        };
        apply_damage_threat(&mut list, attacker, 50.0, &mods);
        assert_eq!(list.entries.len(), 1);
        assert_eq!(list.entries[0].threat, 100.0); // 50 × 2.0
    }
}
