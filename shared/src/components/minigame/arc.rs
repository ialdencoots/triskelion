use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Server-authoritative state for the Physical class Arc mechanic.
///
/// World axis: a semicircular track with a dot parameterised as
///   θ(t) = π/2 + A·sin(ω·t + φ)
/// Commit quality = dot_velocity / peak_velocity  [0, 1].
/// Streak = consecutive commits landing in the nadir zone (~innermost 20%).
///
/// Disruption applies a counter-directional velocity impulse that decays ~2 s.
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
    /// Additional signed velocity added by incoming hits (counter-directional impulse).
    /// Decays back to 0 over ~2 s. Multiple hits accumulate additively.
    pub disruption_velocity: f32,

    // ── Commit state ─────────────────────────────────────────────────────────
    /// True between a commit and the moment the dot reaches the opposite apex.
    /// At most one commit per half-oscillation.
    pub in_lockout: bool,
    /// Quality [0, 1] of the most recent commit, derived from dot velocity at commit time.
    pub last_commit_quality: f32,
    /// Consecutive commits that landed in the nadir zone without an apex-zone commit.
    /// Resets on an apex-zone commit; mid-zone commits neither increment nor reset.
    /// Primary input to DAG modifier path availability.
    pub streak: u32,
}

impl Default for ArcState {
    fn default() -> Self {
        Self {
            amplitude: std::f32::consts::FRAC_PI_2,
            omega: std::f32::consts::PI, // one full oscillation per ~2 s
            phase: 0.0,
            time: 0.0,
            theta: std::f32::consts::FRAC_PI_2,
            disruption_velocity: 0.0,
            in_lockout: false,
            last_commit_quality: 0.0,
            streak: 0,
        }
    }
}
