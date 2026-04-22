use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Seconds for the fill to sweep corner → marker on each edge.
pub const CUBE_FILL_CYCLE_SECS: f32 = 1.5;
/// Width of the collect window around `fill_progress == 1.0`. A press at exactly
/// 1.0 yields max timing precision; precision falls off linearly to 0 at the edge.
pub const CUBE_COLLECT_WINDOW: f32 = 0.18;
/// Fill progress value at which an unclaimed face cycles back to 0.
pub const CUBE_FILL_RESET_AT: f32 = 1.0 + CUBE_COLLECT_WINDOW;
/// Number of rotations granted per cube activation before resolution.
pub const CUBE_ROTATIONS_PER_ACTIVATION: u32 = 4;
/// Total seconds for a single 90° cube rotation animation (half shrink, half grow).
pub const CUBE_ROTATION_SECS: f32 = 0.35;

/// Index of the interactive edge on a cube face. Order matches the
/// `CubeState::current_face` array.
#[repr(u8)]
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeEdge {
    Left = 0,
    Bottom = 1,
    Right = 2,
}

impl CubeEdge {
    pub const ALL: [CubeEdge; 3] = [CubeEdge::Left, CubeEdge::Bottom, CubeEdge::Right];

    pub fn index(self) -> usize {
        self as usize
    }
}

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
    /// Fill animation progress in [0, `CUBE_FILL_RESET_AT`] — sweeps from corners
    /// toward the edge-center marker; resets to 0 if the marker isn't collected.
    pub fill_progress: f32,
    /// Rotations remaining this activation. Cube resolves when this reaches 0.
    pub rotations_remaining: u32,
    /// Aggregate commit quality of the streak that triggered this activation.
    /// Used to tier-gate bonuses when seeding each face. Held constant for the
    /// whole activation even as new arc commits accumulate during the cube.
    pub activation_quality: f32,
    /// Collected bonuses with their timing-precision magnitudes in [0, 1].
    /// Resolved on activation close.
    pub collected: Vec<(PhysicalBonus, f32)>,
    /// Edge toward which the cube is currently rotating, if any. `Some` for
    /// the duration of the rotation animation; `None` when idle or filling.
    pub rotating_edge: Option<CubeEdge>,
    /// Progress [0, 1] through the current rotation. 0 = pre-rotation pose,
    /// 0.5 = edge-on (face swap happens here), 1 = post-rotation pose.
    pub rotation_progress: f32,
    /// True between collect and the midpoint of the rotation — signals that
    /// `current_face` still holds the pre-collect face and needs replacing.
    pub new_face_pending: bool,
}

/// Tier of bonus that can appear on a cube face, gated by the aggregate quality
/// of the streak that triggered activation.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BonusTier {
    Default,
    Mid,
    Premium,
}

impl BonusTier {
    /// Quality thresholds per the design doc:
    ///   q < 0.4         → default
    ///   0.4 ≤ q < 0.7   → default/mid mix
    ///   q ≥ 0.7         → mid/premium with occasional default
    /// This function returns the *dominant* tier; face seeding can still draw
    /// from the surrounding tiers as per the design.
    pub fn from_quality(q: f32) -> Self {
        if q < 0.4 {
            BonusTier::Default
        } else if q < 0.7 {
            BonusTier::Mid
        } else {
            BonusTier::Premium
        }
    }
}

/// Timing precision from fill progress at collect time. Peak at 1.0, falling
/// off linearly to 0 at the window edges. Returns `None` if outside the window.
pub fn timing_precision(fill_progress: f32) -> Option<f32> {
    let offset = (fill_progress - 1.0).abs();
    if offset > CUBE_COLLECT_WINDOW {
        None
    } else {
        Some(1.0 - offset / CUBE_COLLECT_WINDOW)
    }
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

impl PhysicalBonus {
    /// Short label for the visual stand-in marker until proper icons exist.
    pub fn short_label(&self) -> &'static str {
        match self {
            PhysicalBonus::BaseDamage(_) => "DMG",
            PhysicalBonus::DamageOverTime { .. } => "DoT",
            PhysicalBonus::StunOnHit { .. } => "STUN",
            PhysicalBonus::CooldownReduction(_) => "CDR",
            PhysicalBonus::Healing(_) => "HEAL",
            PhysicalBonus::AggroBonus(_) => "AGGRO",
        }
    }
}
