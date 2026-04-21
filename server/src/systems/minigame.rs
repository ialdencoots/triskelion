use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use shared::components::minigame::{
    arc::{ArcState, SecondaryArcState},
    bar_fill::BarFillState,
    cube::CubeState,
    heartbeat::HeartbeatState,
    value_lock::ValueLockState,
    wave_interference::WaveInterferenceState,
};

fn tick_arc(arc: &mut ArcState, dt: f32) {
    arc.disruption_velocity *= (-dt * 0.5_f32).exp();
    arc.phase += arc.disruption_velocity * dt;
    arc.time += dt;
    arc.theta = FRAC_PI_2 + arc.amplitude * (arc.omega * arc.time + arc.phase).sin();
    if arc.in_lockout {
        let apex_proximity = (arc.theta - FRAC_PI_2).abs() / arc.amplitude;
        if apex_proximity >= 0.9 {
            arc.in_lockout = false;
        }
    }
}

/// Evaluate a commit attempt on an arc. No-ops if in lockout.
pub fn process_arc_commit(arc: &mut ArcState) {
    if arc.in_lockout {
        return;
    }
    let dot_vel = arc.amplitude * arc.omega * (arc.omega * arc.time + arc.phase).cos()
        + arc.disruption_velocity;
    let peak_vel = arc.amplitude * arc.omega;
    arc.last_commit_quality = (dot_vel.abs() / peak_vel).min(1.0);
    arc.last_commit_theta = arc.theta;
    let proximity = (arc.theta - FRAC_PI_2).abs() / arc.amplitude;
    if proximity < 0.2 {
        arc.streak += 1;
    } else if proximity > 0.8 {
        arc.streak = 0;
    }
    arc.in_lockout = true;
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

/// Advance all active cube overlays by one server tick.
///
/// Per tick:
/// - If inactive: check arc streak cap; on trigger, seed face bonuses from
///   skill-tree × aggregate-quality and open the overlay.
/// - If active: advance `fill_progress`; on timing input, collect bonus, rotate,
///   decrement `rotations_remaining`. At 0: resolve `collected` and close the overlay.
pub fn tick_cube_states(time: Res<Time>, mut query: Query<&mut CubeState>) {}

/// Advance all active Bar Fill states by one server tick.
///
/// Per tick:
/// - Advance `fill` using `fill_rate(p) = base_rate * p^fill_exponent`.
/// - On auto-reset (fill >= 1.0): generate new bonus markers and reset fill to 0.
/// - Decay `arcane_pool` by `pool_decay_rate * dt`.
pub fn tick_bar_fill_states(time: Res<Time>, mut query: Query<&mut BarFillState>) {}

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
pub fn tick_heartbeat_states(time: Res<Time>, mut query: Query<&mut HeartbeatState>) {}
