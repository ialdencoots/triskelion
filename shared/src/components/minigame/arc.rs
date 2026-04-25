use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::commit_tracker::CommitTracker;

/// Streak count at which the Tank/Heal cube activates.  Tuned shorter than the
/// grid cap so cube cycles feel tight and the per-activation payout is small.
pub const CUBE_CRITICAL_MASS_CAP: u32 = 2;

/// Streak count at which the DPS grid force-activates (it also activates on
/// streak break). Larger than the cube cap so the step budget is meaningful.
pub const GRID_CRITICAL_MASS_CAP: u32 = 10;

/// Capacity of the commit-quality history. Sized to the grid cap so per-step
/// magnitude lookups always have data; the cube reads the aggregate mean and
/// is unaffected by the extra tail entries.
pub const QUALITY_HISTORY_CAPACITY: u32 = GRID_CRITICAL_MASS_CAP;

/// Server-authoritative state for the Physical class Arc mechanic.
///
/// World axis: a semicircular track with a dot parameterised as
///   θ(t) = π/2 + A·sin(ω·t + φ)
/// Commit quality = dot_velocity / peak_velocity  [0, 1].
/// Streak = consecutive commits landing in the nadir zone (~innermost 20%).
///
/// Disruption opens a time-reversal window: while `disruption_remaining > 0`,
/// the simulation clock runs backward, so the dot literally retraces its sine
/// path until the window closes. This produces an unambiguous momentum
/// disruption — the dot's direction flips on the frame the hit lands and
/// snaps back to natural travel when the window expires.
/// Ghost arc history (visual record of commit angles) is client-side only.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ArcState {
    // ── World axis parameters (set on stance entry) ──────────────────────────
    /// Sweep width A (radians). Determines zone widths.
    pub amplitude: f32,
    /// Oscillation frequency ω (rad/s).
    pub omega: f32,
    /// Phase offset φ; randomised at stance entry so each activation feels fresh.
    pub phase: f32,

    // ── Live simulation ──────────────────────────────────────────────────────
    /// Internal time counter advancing each server tick while the stance is active.
    pub time: f32,
    /// Current dot angle θ in radians (derived from `time` each tick).
    pub theta: f32,

    // ── Disruption ───────────────────────────────────────────────────────────
    /// Seconds of remaining time-reversal. While positive, `tick_arc` runs
    /// the sine clock backwards, so the dot retraces its prior path. Hits
    /// add to this; clamped to a small upper bound so a single big hit can't
    /// pin the dot for half a cycle.
    pub disruption_remaining: f32,

    // ── Commit state ─────────────────────────────────────────────────────────
    /// Lockout flag, last commit quality, and recent-quality ring buffer.
    /// Cube reads `commit.mean()` at activation to gate bonus tier; grid reads
    /// per-step magnitudes from `commit.history`. The lockout is held between
    /// a commit and the moment the dot reaches the opposite apex (one commit
    /// per half-oscillation).
    pub commit: CommitTracker,
    /// Theta at the moment of the most recent commit. Set server-side; read by the
    /// client to push accurate ghost history entries.
    pub last_commit_theta: f32,
    /// Consecutive commits that landed in the nadir zone without an apex-zone commit.
    /// Resets to 0 only on an apex-zone commit — mid-zone commits neither increment nor
    /// reset, and cube activation does NOT reset the streak. This makes the streak a
    /// running consistency counter visible to the player, while activation is gated on
    /// the *delta* since the last activation (see `streak_at_last_activation`).
    pub streak: u32,
    /// Snapshot of `streak` at the moment the cube (or, in DPS, the grid) last
    /// activated. Activation fires when `streak - streak_at_last_activation` reaches
    /// the critical-mass cap. Reset to 0 alongside `streak` on an apex-zone break.
    pub streak_at_last_activation: u32,
    /// Count of apex-zone visits (proximity ≥ 0.9 rising edges) since the last
    /// commit. Reset on commit. When it reaches 2 (a full oscillation elapsed
    /// with no commit), the streak breaks — punishes idleness during an active
    /// streak.
    pub apex_visits_since_commit: u32,
    /// Previous tick's at-apex state — drives rising-edge detection for
    /// `apex_visits_since_commit`.
    pub prev_at_apex: bool,
}

/// Second independent arc for Physical DPS stance (one per weapon).
/// Structurally identical to `ArcState`; a separate component type so Bevy ECS
/// can hold both on the same entity. Secondary commit key (W) is deferred.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct SecondaryArcState(pub ArcState);

impl Default for ArcState {
    fn default() -> Self {
        Self {
            amplitude: std::f32::consts::FRAC_PI_2,
            omega: std::f32::consts::PI, // one full oscillation per ~2 s
            phase: 0.0,
            time: 0.0,
            theta: std::f32::consts::FRAC_PI_2,
            disruption_remaining: 0.0,
            commit: CommitTracker::with_capacity(QUALITY_HISTORY_CAPACITY as usize),
            last_commit_theta: std::f32::consts::FRAC_PI_2,
            streak: 0,
            streak_at_last_activation: 0,
            apex_visits_since_commit: 0,
            prev_at_apex: false,
        }
    }
}
