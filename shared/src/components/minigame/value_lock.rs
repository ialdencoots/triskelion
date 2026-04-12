use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Server-authoritative state for the Nature class Value Lock mechanic.
///
/// Unlike Arc or Bar Fill, this bar has no autonomous fill — it fills only
/// while the player holds the input and locks in place on release.
///
/// Locked value maps linearly to a target heartbeat frequency:
///   target_freq = freq_min + locked_value·(freq_max - freq_min)
///
/// Entrainment: releasing within ±delta of the previous locked value increments the
/// entrainment streak. Releasing elsewhere resets streak to 0 (valid strategic choice).
///
/// Target frequency decays at a fixed rate (not proportional to value):
///   d(target_freq)/dt = -k_decay
/// This "stepped decay" means the hold window before signal loss is the same
/// at low and high frequency — high-frequency operation is already more demanding
/// because heartbeat commit windows compress proportionally.
///
/// Disruption: incoming hits inject a transient frequency spike to the heartbeat
/// (see HeartbeatState), not directly to this mechanic.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ValueLockState {
    // ── Hold input ───────────────────────────────────────────────────────────
    /// True while the player is holding the lock input; false after release.
    pub is_held: bool,
    /// Fill progress accumulated while `is_held` is true [0, 1].
    /// Fills at a fixed rate; locked to zero on release.
    pub hold_progress: f32,

    // ── Lock state ───────────────────────────────────────────────────────────
    /// The most recently locked fill value [0, 1].
    pub locked_value: f32,
    /// The lock value from the previous lock event; used to evaluate entrainment.
    pub previous_locked: f32,

    // ── Entrainment ───────────────────────────────────────────────────────────
    /// Tolerance for an entrainment match: |locked_value - previous_locked| < delta.
    pub entrainment_delta: f32,
    /// Consecutive matching re-locks. Multiplies delivered heartbeat value.
    pub entrainment_streak: u32,

    // ── Frequency ────────────────────────────────────────────────────────────
    /// Current target heartbeat frequency (Hz), derived from the last lock and then
    /// decaying at `frequency_decay_rate`.
    pub target_frequency: f32,
    /// Fixed decay rate k_decay (Hz per second). Constant regardless of current frequency.
    pub frequency_decay_rate: f32,
}
