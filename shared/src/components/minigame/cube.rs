use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Server-authoritative state for the Tank/Healer cube overlay.
///
/// Activates on arc streak cap. Three of the cube's visible edges (left, bottom, right)
/// each run a fill animation toward a bonus marker at the edge center. Timing an input
/// on a marker collects that bonus, rotates the cube, and reveals a fresh face with
/// three new markers. Arc play continues during the cube — multitasking accelerates
/// the next activation.
///
/// Bonus tier on each face is gated by the aggregate quality of the capped streak;
/// bonus magnitude is determined by per-bonus timing precision at collection.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CubeState {
    /// True while a cube activation is in progress.
    pub active: bool,
    /// Left, bottom, right bonus markers on the current face.
    pub current_face: [Option<PhysicalBonus>; 3],
    /// Fill animation progress [0, 1] from corners toward edge centers.
    pub fill_progress: f32,
    /// Rotations remaining this activation. Cube resolves when this reaches 0.
    pub rotations_remaining: u32,
    /// Collected bonuses with their timing-precision magnitudes.
    pub collected: Vec<(PhysicalBonus, f32)>,
}

/// A bonus delivered by a cube face or a grid node. Shared between the Tank/Healer
/// cube and the Duelist grid; distributions within the pool are role-skewed and
/// skill-tree-seeded.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PhysicalBonus {
    /// Flat addition to the action's base damage output.
    BaseDamage(f32),
    /// Apply a damage-over-time effect to the target.
    DamageOverTime {
        damage_per_second: f32,
        duration_secs: f32,
    },
    /// Apply a brief stun to the target on hit.
    StunOnHit { duration_secs: f32 },
    /// Reduce the cooldown of the next ability used.
    CooldownReduction(f32),
    /// Deliver healing to a friendly target.
    Healing(f32),
    /// Temporarily increase threat generation.
    AggroBonus(f32),
}
