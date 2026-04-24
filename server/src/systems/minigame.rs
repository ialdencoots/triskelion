use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use shared::components::combat::CombatState;
use shared::components::minigame::{
    arc::{ArcState, SecondaryArcState, CUBE_CRITICAL_MASS_CAP, QUALITY_HISTORY_CAPACITY},
    bar_fill::BarFillState,
    cube::{
        BonusTier, CubeState, PhysicalBonus, CUBE_FILL_CYCLE_SECS, CUBE_FILL_RESET_AT,
        CUBE_POP_SECS, CUBE_ROTATIONS_PER_ACTIVATION, CUBE_ROTATION_HOLD_SECS,
        CUBE_ROTATION_SECS,
    },
    heartbeat::HeartbeatState,
    value_lock::ValueLockState,
    wave_interference::WaveInterferenceState,
};
use shared::components::player::RoleStance;

use super::combat::next_unit;

fn tick_arc(arc: &mut ArcState, dt: f32) {
    arc.disruption_velocity *= (-dt * 0.5_f32).exp();
    arc.phase += arc.disruption_velocity * dt;
    arc.time += dt;
    arc.theta = FRAC_PI_2 + arc.amplitude * (arc.omega * arc.time + arc.phase).sin();

    // Apex-zone rising-edge detection drives two behaviors:
    //   1. Release lockout once the dot reaches the opposite apex after a commit.
    //   2. Break the streak after two consecutive apex visits with no commit
    //      (≈ one full oscillation of idleness).
    let apex_proximity = (arc.theta - FRAC_PI_2).abs() / arc.amplitude;
    let at_apex = apex_proximity >= 0.9;
    if at_apex && !arc.prev_at_apex {
        arc.commit.in_lockout = false;
        arc.apex_visits_since_commit = arc.apex_visits_since_commit.saturating_add(1);
        if arc.apex_visits_since_commit >= 2 && arc.streak > 0 {
            arc.streak = 0;
            arc.streak_at_last_activation = 0;
        }
    }
    arc.prev_at_apex = at_apex;
}

/// Evaluate a commit attempt on an arc. No-ops if in lockout.
pub fn process_arc_commit(arc: &mut ArcState) {
    if arc.commit.in_lockout {
        return;
    }
    let dot_vel = arc.amplitude * arc.omega * (arc.omega * arc.time + arc.phase).cos()
        + arc.disruption_velocity;
    let peak_vel = arc.amplitude * arc.omega;
    let quality = (dot_vel.abs() / peak_vel).min(1.0);
    arc.commit.push(quality, QUALITY_HISTORY_CAPACITY as usize);
    arc.last_commit_theta = arc.theta;

    let proximity = (arc.theta - FRAC_PI_2).abs() / arc.amplitude;
    if proximity < 0.2 {
        arc.streak += 1;
    } else if proximity > 0.8 {
        arc.streak = 0;
        // The baseline from which the next activation delta is measured travels
        // with the streak — clear it so a fresh streak activates at the cap again.
        arc.streak_at_last_activation = 0;
    }
    // Any commit (any zone) resets the idle-break counter — the streak only
    // breaks from apex visits that pass with no commit at all.
    arc.apex_visits_since_commit = 0;
    arc.commit.in_lockout = true;
}

/// Advance all active Arc states by one server tick.
pub fn tick_arc_states(time: Res<Time>, mut query: Query<&mut ArcState>) {
    let dt = time.delta_secs();
    for mut arc in query.iter_mut() {
        tick_arc(&mut arc, dt);
    }
}

/// Advance all secondary (DPS second-weapon) Arc states by one server tick.
pub fn tick_secondary_arc_states(time: Res<Time>, mut query: Query<&mut SecondaryArcState>) {
    let dt = time.delta_secs();
    for mut secondary in query.iter_mut() {
        tick_arc(&mut secondary.0, dt);
    }
}

// ── Cube ─────────────────────────────────────────────────────────────────────

/// Seed one edge's bonus drawn from a tier-appropriate pool. Skill tree weighting
/// is deferred; this picks uniformly within the tier band.
fn sample_bonus(tier: BonusTier) -> PhysicalBonus {
    let roll = next_unit();
    match tier {
        BonusTier::Default => match (roll * 3.0) as u32 {
            0 => PhysicalBonus::BaseDamage(8.0),
            1 => PhysicalBonus::AggroBonus(0.15),
            _ => PhysicalBonus::CooldownReduction(0.5),
        },
        BonusTier::Mid => match (roll * 3.0) as u32 {
            0 => PhysicalBonus::BaseDamage(20.0),
            1 => PhysicalBonus::DamageOverTime {
                damage_per_second: 10.0,
                duration_secs: 3.0,
            },
            _ => PhysicalBonus::StunOnHit { duration_secs: 0.5 },
        },
        BonusTier::Premium => match (roll * 3.0) as u32 {
            0 => PhysicalBonus::BaseDamage(40.0),
            1 => PhysicalBonus::StunOnHit { duration_secs: 1.0 },
            _ => PhysicalBonus::Healing(20.0),
        },
    }
}

/// Seed three fresh bonus markers for a new face. The dominant tier comes from
/// `activation_quality`; per the design, mid/premium activations still occasionally
/// surface default-tier entries (here: 20% chance to step down one tier per slot).
fn seed_face(activation_quality: f32) -> [Option<PhysicalBonus>; 3] {
    let base = BonusTier::from_quality(activation_quality);
    std::array::from_fn(|_| {
        let tier = if next_unit() < 0.2 {
            // Step down one tier for variety; premium drops to mid, mid to default.
            match base {
                BonusTier::Premium => BonusTier::Mid,
                BonusTier::Mid => BonusTier::Default,
                BonusTier::Default => BonusTier::Default,
            }
        } else {
            base
        };
        Some(sample_bonus(tier))
    })
}

/// Open the cube: freeze aggregate quality, seed a fresh face, reset fill.
/// Assumes the caller has already verified the streak cap and role gate.
pub fn activate_cube(cube: &mut CubeState, arc: &mut ArcState) {
    let q = arc.commit.mean();
    cube.active = true;
    cube.activation_quality = q;
    cube.fill_progress = 0.0;
    cube.rotations_remaining = CUBE_ROTATIONS_PER_ACTIVATION;
    cube.current_face = seed_face(q);
    cube.collected.clear();
    cube.rotating_edge = None;
    cube.pop_progress = 0.0;
    cube.rotation_progress = 0.0;
    cube.rotation_hold_remaining = 0.0;
    cube.new_face_pending = false;
    // Streak is NOT reset — it's a running consistency counter that survives cube
    // activation. Record the level at which this activation fired so the next one
    // triggers another CUBE_CRITICAL_MASS_CAP nadir commits later.
    arc.streak_at_last_activation = arc.streak;
}

/// Try to collect the bonus on `edge` of the current face. Returns true on
/// success. The collected bonus and timing precision are recorded and the
/// post-collect animation sequence begins (pop → rotation → hold); the face
/// swap and resolution are driven from `tick_cube_states`.
pub fn process_cube_collect(
    cube: &mut CubeState,
    edge: shared::components::minigame::cube::CubeEdge,
) -> bool {
    if !cube.active || cube.rotating_edge.is_some() {
        return false;
    }
    let Some(precision) =
        shared::components::minigame::cube::timing_precision(cube.fill_progress)
    else {
        return false;
    };
    let Some(bonus) = cube.current_face[edge.index()].clone() else {
        return false;
    };
    cube.collected.push((bonus, precision));
    cube.rotations_remaining = cube.rotations_remaining.saturating_sub(1);
    cube.rotating_edge = Some(edge);
    cube.pop_progress = 0.0;
    cube.rotation_progress = 0.0;
    cube.rotation_hold_remaining = 0.0;
    cube.new_face_pending = true;
    // `fill_progress` is left at whatever it was — the fills read as "full"
    // during the pop phase so the hit visually freezes a beat before rotating.
    true
}

/// Finalize the cube activation. Bonuses in `collected` are the eventual payload;
/// rider/window/charge resolution is a follow-up system (see design doc).
fn resolve_cube(cube: &mut CubeState) {
    cube.active = false;
    cube.fill_progress = 0.0;
    cube.rotations_remaining = 0;
    cube.current_face = [None, None, None];
    cube.rotating_edge = None;
    cube.pop_progress = 0.0;
    cube.rotation_progress = 0.0;
    cube.rotation_hold_remaining = 0.0;
    cube.new_face_pending = false;
    // `collected` is left populated for the resolution system to drain.
}

/// Discard an active cube and any bonuses collected so far. Used when the
/// player leaves the stance that spawned it — unlike `resolve_cube`, nothing
/// is paid out, because the player didn't complete the engagement.
pub fn cancel_cube(cube: &mut CubeState) {
    resolve_cube(cube);
    cube.collected.clear();
}

/// Advance all active cube overlays by one server tick.
///
/// Inactive cube on a Tank/Heal stance: if arc streak hits `CRITICAL_MASS_CAP`, activate.
/// Active cube: advance `fill_progress`; reset to 0 when it sweeps past the collect
/// window without a collect. Rotation/resolution are driven by input, not time.
pub fn tick_cube_states(
    time: Res<Time>,
    mut query: Query<(&mut CubeState, &mut ArcState, &CombatState)>,
) {
    let dt = time.delta_secs();
    for (mut cube, mut arc, combat) in query.iter_mut() {
        let is_tank_heal = matches!(
            combat.active_stance,
            Some(RoleStance::Tank) | Some(RoleStance::Heal)
        );

        if !cube.active {
            if is_tank_heal
                && arc.streak.saturating_sub(arc.streak_at_last_activation)
                    >= CUBE_CRITICAL_MASS_CAP
            {
                activate_cube(&mut cube, &mut arc);
            }
            continue;
        }

        // Post-collect animation pipeline takes precedence over fill advance.
        // Phases (in order, gated on `rotating_edge.is_some()`):
        //   1. Pop: landed marker pops (client-side visual), `pop_progress` 0→1
        //   2. Rotation: cube turns 90°, face swap at midpoint
        //   3. Hold: cube sits face-on at new face for a beat
        //   4. Exit: clear rotating_edge, reset fill, possibly resolve
        if cube.rotating_edge.is_some() {
            if cube.pop_progress < 1.0 {
                cube.pop_progress = (cube.pop_progress + dt / CUBE_POP_SECS).min(1.0);
            } else if cube.rotation_progress < 1.0 {
                cube.rotation_progress += dt / CUBE_ROTATION_SECS;
                if cube.new_face_pending && cube.rotation_progress >= 0.5 {
                    cube.new_face_pending = false;
                    if cube.rotations_remaining > 0 {
                        cube.current_face = seed_face(cube.activation_quality);
                    } else {
                        cube.current_face = [None, None, None];
                    }
                }
                if cube.rotation_progress >= 1.0 {
                    cube.rotation_progress = 1.0;
                    cube.rotation_hold_remaining = CUBE_ROTATION_HOLD_SECS;
                }
            } else if cube.rotation_hold_remaining > 0.0 {
                cube.rotation_hold_remaining =
                    (cube.rotation_hold_remaining - dt).max(0.0);
            } else {
                // Sequence complete — back to fill mode (or resolve).
                cube.rotating_edge = None;
                cube.pop_progress = 0.0;
                cube.rotation_progress = 0.0;
                cube.fill_progress = 0.0;
                if cube.rotations_remaining == 0 {
                    resolve_cube(&mut cube);
                }
            }
            continue;
        }

        cube.fill_progress += dt / CUBE_FILL_CYCLE_SECS;
        if cube.fill_progress >= CUBE_FILL_RESET_AT {
            // Swept past the window without a collect — cycle the face again.
            cube.fill_progress = 0.0;
        }
    }
}

/// Advance all active Bar Fill states by one server tick.
///
/// Per tick:
/// - Advance `fill` using `fill_rate(p) = base_rate * p^fill_exponent`.
/// - On auto-reset (fill >= 1.0): generate new bonus markers and reset fill to 0.
/// - Decay `arcane_pool` by `pool_decay_rate * dt`.
/// - Consume `drain_pending` from `fill` first, spilling overflow into
///   `arcane_pool` (negative — reducing banked pool). Drain spreads over
///   ~0.3 s so it reads as a rapid-but-visible dip rather than a snap.
pub fn tick_bar_fill_states(time: Res<Time>, mut query: Query<&mut BarFillState>) {
    const DRAIN_RATE: f32 = 1.0 / 0.3;
    let dt = time.delta_secs();
    for mut bf in query.iter_mut() {
        if bf.drain_pending > 0.0 {
            let drain_this_tick = (bf.drain_pending * DRAIN_RATE * dt).min(bf.drain_pending);
            bf.drain_pending -= drain_this_tick;
            let from_fill = drain_this_tick.min(bf.fill);
            bf.fill -= from_fill;
            let leftover = drain_this_tick - from_fill;
            if leftover > 0.0 {
                bf.arcane_pool = (bf.arcane_pool - leftover).max(0.0);
            }
        }
    }
}

/// Advance all active Wave Interference states by one server tick.
///
/// Per tick:
/// - Advance `time`.
/// - Advance `travel_progress` on each active segment; on exit compute area and
///   accumulate into `wave_accumulation`.
/// - For uncovered zone time, add raw traveling-wave area to `wave_accumulation`.
/// - Decay `disruption_amp` and `wave_accumulation` (slow passive decay).
/// - Decrement `commit_cooldown`.
pub fn tick_wave_interference_states(
    time: Res<Time>,
    mut query: Query<&mut WaveInterferenceState>,
) {
}

/// Advance all active Value Lock states by one server tick.
///
/// Per tick:
/// - If `is_held`: increment `hold_progress` at the fixed fill rate.
/// - Decay `target_frequency` by `frequency_decay_rate * dt` (fixed-rate stepped decay).
pub fn tick_value_lock_states(time: Res<Time>, mut query: Query<&mut ValueLockState>) {}

/// Advance all active Heartbeat states by one server tick.
///
/// Per tick:
/// - Interpolate `current_frequency` toward `target_frequency` via τ_interp.
/// - Add `frequency_spike` (from disruption) to `current_frequency`; decay the spike.
/// - Advance `phase` by `current_frequency * dt`, wrapping at 1.0.
/// - Decay `envelope_noise`.
/// - Release lockout at phase midpoint (0.5).
///
/// MVP scope: full phase/frequency simulation is deferred. This tick only
/// decays the two disruption fields so they don't accumulate indefinitely.
/// `frequency_spike` decays fast (1 s); `envelope_noise` decays slower (3 s)
/// to match the "sustained noise floor" design.
pub fn tick_heartbeat_states(time: Res<Time>, mut query: Query<&mut HeartbeatState>) {
    let dt = time.delta_secs();
    for mut hb in query.iter_mut() {
        hb.frequency_spike *= (-dt / 1.0_f32).exp();
        hb.envelope_noise  *= (-dt / 3.0_f32).exp();
    }
}
