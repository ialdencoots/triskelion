use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Shared commit-rate-limit + quality-history shape used by minigames whose
/// player input is structured as discrete commits with a per-commit quality.
///
/// Currently embedded in `ArcState`. Heartbeat and Wave Interference are
/// expected to adopt this same shape when their server-side simulation lands —
/// see `shared/src/components/minigame/{heartbeat,wave_interference}.rs` for
/// the matching design fields. Keeping them aligned now stops three parallel
/// implementations from drifting later.
///
/// Capacity is a caller concern: each minigame sizes the history to its own
/// activation cap. See `ArcState::default()` for the canonical sizing pattern.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommitTracker {
    /// True between a commit and the next legal commit window (e.g. opposite
    /// apex for the arc, half-period midpoint for the heartbeat).
    pub in_lockout: bool,
    /// Quality [0, 1] of the most recent commit.
    pub last_quality: f32,
    /// Ring buffer of recent commit qualities (newest at front). Bounded by
    /// the capacity passed to `push`.
    pub history: VecDeque<f32>,
}

impl CommitTracker {
    /// Construct an empty tracker with a reserved history capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            in_lockout: false,
            last_quality: 0.0,
            history: VecDeque::with_capacity(cap),
        }
    }

    /// Record a commit: stores `quality` as the latest, prepends it to the
    /// history, and truncates to `capacity`.
    pub fn push(&mut self, quality: f32, capacity: usize) {
        self.last_quality = quality;
        self.history.push_front(quality);
        if self.history.len() > capacity {
            self.history.pop_back();
        }
    }

    /// Mean of the history clamped to [0, 1]. Returns 0 if empty.
    pub fn mean(&self) -> f32 {
        if self.history.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.history.iter().sum();
        (sum / self.history.len() as f32).clamp(0.0, 1.0)
    }
}

impl Default for CommitTracker {
    fn default() -> Self {
        Self::with_capacity(0)
    }
}
