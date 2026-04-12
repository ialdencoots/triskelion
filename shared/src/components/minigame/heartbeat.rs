use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Server-authoritative state for the Nature class Heartbeat mechanic.
///
/// The heartbeat circle grows and shrinks following a cardiac envelope per period:
///   0.00–0.07: primary systole rise (high-value commit window)
///   0.07–0.18: fall from primary peak
///   0.18–0.25: secondary echo (~40% of primary peak)
///   0.25–0.36: fall from secondary peak
///   0.36–1.00: diastolic flat (near-zero value)
///
/// Current frequency interpolates toward the target (from Value Lock):
///   d(freq)/dt = (target_freq - current_freq) / τ_interp
///
/// Commit quality = r(t_n) where t_n = (t mod 1/freq) · freq.
///
/// Delivered value per commit:
///   pulse_charge · freq_multiplier(current_freq) · entrainment_multiplier(streak)
///
/// Disruption: incoming hits inject a tachycardia spike and envelope noise.
/// The spike compresses all windows; noise makes peak/trough harder to distinguish.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HeartbeatState {
    // ── Frequency ────────────────────────────────────────────────────────────
    /// Current (interpolated) frequency in Hz.
    pub current_frequency: f32,
    /// Target frequency from Value Lock; `current_frequency` interpolates toward this.
    pub target_frequency: f32,
    /// Interpolation time constant τ_interp (~0.8 s). Smaller = snappier response.
    pub interp_tau: f32,

    // ── Envelope ─────────────────────────────────────────────────────────────
    /// Normalised phase within the current beat period [0, 1].
    pub phase: f32,
    /// Disruption envelope noise amplitude. Decays ~3 s. Multiple hits accumulate up to a cap.
    pub envelope_noise: f32,
    /// Tachycardia impulse added to `current_frequency` on a hit. Decays independently.
    pub frequency_spike: f32,

    // ── Commit state ─────────────────────────────────────────────────────────
    /// True between a commit and the next phase midpoint (one commit per half period).
    pub in_lockout: bool,
}

impl Default for HeartbeatState {
    fn default() -> Self {
        Self {
            current_frequency: 1.0,
            target_frequency: 1.0,
            interp_tau: 0.8,
            phase: 0.0,
            envelope_noise: 0.0,
            frequency_spike: 0.0,
            in_lockout: false,
        }
    }
}
