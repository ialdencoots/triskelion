use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Server-authoritative state for the Arcane class Bar Fill mechanic.
///
/// Fill rate: `fill_rate(p) = base_rate * p^exponent`
/// where `p` is current fill [0,1] and `exponent > 1` gives slow initial fill
/// with rapid acceleration near full. At p = 1.0 the bar resets instantly.
///
/// On commit: `pool_gain = current_fill` (plus any hit bonus markers).
/// Arcane Pool caps at 5.0 (500%) and decays passively.
/// Pool above 1.0 produces super-linear output scaling on action use.
///
/// Disruption: incoming hits drain the bar by a hit-determined amount.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BarFillState {
    // ── World axis ───────────────────────────────────────────────────────────
    /// Current fill proportion [0, 1].
    pub fill: f32,
    /// Base fill rate coefficient (design parameter).
    pub base_rate: f32,
    /// Power-curve exponent (> 1). Higher = more back-loaded acceleration.
    pub fill_exponent: f32,

    // ── Arcane Pool ──────────────────────────────────────────────────────────
    /// Accumulated pool in [0, 5.0] where 1.0 = 100%. Decays passively.
    pub arcane_pool: f32,
    /// Passive pool decay rate (pool units per second).
    pub pool_decay_rate: f32,

    // ── Bonus markers ────────────────────────────────────────────────────────
    /// Bonus markers placed at random fill positions at the start of each fill cycle.
    /// Committing within `delta` of a marker's position adds its bonus to the pool.
    pub bonus_markers: Vec<BonusMarker>,
}

impl Default for BarFillState {
    fn default() -> Self {
        Self {
            fill: 0.0,
            base_rate: 0.1,
            fill_exponent: 3.0,
            arcane_pool: 0.0,
            pool_decay_rate: 0.05,
            bonus_markers: Vec::new(),
        }
    }
}

/// A single bonus marker visible on the bar for the current fill cycle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BonusMarker {
    /// Fill proportion where this marker sits [0, 1].
    pub position: f32,
    /// Tolerance window: commit within ±delta to receive the bonus.
    pub delta: f32,
    /// The bonus added to Arcane Pool if the player commits here.
    pub bonus: BarBonus,
}

/// Bonus types that can appear on bar fill markers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum BarBonus {
    /// Multiply the pool gain from this commit by a factor (> 1).
    PoolOutputMultiplier(f32),
    /// Reduce the pool cost of the next action fired.
    ReducedActionCost(f32),
    /// Trigger a secondary effect on the next action (exact effect is action-specific).
    SecondaryEffect,
}
