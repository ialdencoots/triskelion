use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Server-authoritative state for the Arcane class Wave Interference mechanic.
///
/// Traveling wave (scrolls right-to-left):
///   tw(x,t) = A·(c1·sin(K1·(x+SPD·t)) + c2·sin(K2·(x+SPD·t)+φ2) + c3·sin(K3·(x+SPD·t)+φ3))
/// Standing wave (commit zone only):
///   sw(x,t) = A·PAMP·(sin(K1·x+φ) + da·sin(dk·x+dp))·cos(ω_sw·t)
///
/// Within the commit zone the player sees tw+sw (interference result).
/// On commit, the current zone-width segment is frozen and travels leftward.
/// Area under each segment on exit adds to `wave_accumulation`.
///
/// Orientation determines meaning of accumulation:
///   Destructive (Aegis/tank): accumulation = Pressure; goal is to keep it low.
///   Constructive (Conduit/Arcanist): accumulation = Potential; goal is to build it.
///
/// Arcane Pool (from BarFill) is spent to act on Wave Accumulation.
///
/// Disruption: adds high-frequency spatial noise to the standing wave.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WaveInterferenceState {
    // ── World axis ───────────────────────────────────────────────────────────
    /// Elapsed time used to evaluate both wave functions.
    pub time: f32,
    /// PAMP: standing wave amplitude, derived from player stats (not adjustable in real time).
    pub player_amp: f32,

    // ── Disruption state ─────────────────────────────────────────────────────
    /// `da`: disruption amplitude added to standing wave spatial profile. Decays ~3 s.
    pub disruption_amp: f32,
    /// `dk`: random spatial frequency of the disruption component (set per hit).
    pub disruption_freq: f32,
    /// `dp`: random phase of the disruption component (set per hit).
    pub disruption_phase: f32,

    // ── Accumulation ─────────────────────────────────────────────────────────
    /// Accumulated Pressure (destructive) or Potential (constructive).
    pub wave_accumulation: f32,
    /// Whether this subclass operates in destructive or constructive interference mode.
    pub orientation: WaveOrientation,

    // ── Commit state ─────────────────────────────────────────────────────────
    /// Time remaining until the next commit is permitted (one per half standing-wave period).
    pub commit_cooldown: f32,
    /// Segments committed but not yet fully exited the zone.
    /// Each frame we advance `travel_progress`; on exit we add `area` to `wave_accumulation`.
    pub active_segments: Vec<CommittedSegment>,
}

impl Default for WaveInterferenceState {
    fn default() -> Self {
        Self {
            time: 0.0,
            player_amp: 1.0,
            disruption_amp: 0.0,
            disruption_freq: 0.0,
            disruption_phase: 0.0,
            wave_accumulation: 0.0,
            orientation: WaveOrientation::default(),
            commit_cooldown: 0.0,
            active_segments: Vec::new(),
        }
    }
}

/// Determines whether a subclass benefits from destructive or constructive interference.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub enum WaveOrientation {
    /// Tank (Aegis): minimize accumulation. `wave_accumulation` = incoming Pressure.
    /// Pool expenditure directly relieves Pressure.
    #[default]
    Destructive,
    /// DPS/Healer (Arcanist/Conduit): maximize accumulation. `wave_accumulation` = Potential.
    /// Pool expenditure converts Potential into output; more Potential = larger effect.
    Constructive,
}

/// A snapshot of the interference result at commit time, traveling leftward through the zone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommittedSegment {
    /// Server time at which this segment was committed; used to reconstruct its shape on the client.
    pub commit_time: f32,
    /// Travel progress [0, 1]; segment exits the zone and contributes area at 1.0.
    pub travel_progress: f32,
    /// Pre-computed |∫ committed_segment(x) dx| over zone width. Added to accumulation on exit.
    pub area: f32,
}
