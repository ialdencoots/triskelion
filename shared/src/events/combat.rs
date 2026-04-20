use bevy::prelude::*;

use crate::components::combat::DamageType;

/// Server-local damage message. Emitted by any damage source (arc commit, DoT
/// tick, future ability) and consumed by a single resolver system that applies
/// the formula, subtracts HP, and generates threat.
///
/// Formula: `final = base × quality × (1 + additive) × multipliers × (1 − resist[ty])`
///
/// - `additive` is the sum of flat bonuses (DAG modifiers, gear). Default 0.0.
/// - `multipliers` is the product of buff/debuff multipliers. Default 1.0.
/// - `quality` is the minigame commit quality in [0, 1]. Default 1.0 for
///   non-minigame sources (DoT ticks, fixed-damage abilities).
#[derive(Message, Clone, Debug)]
pub struct DamageEvent {
    pub attacker: Entity,
    pub target: Entity,
    pub base: f32,
    pub ty: DamageType,
    pub additive: f32,
    pub multipliers: f32,
    pub quality: f32,
    /// Set by the damage source when a hit is a critical. Informational — the
    /// actual crit damage scaling is already folded into `multipliers` by the
    /// source. This flag exists so the client can render crits distinctly.
    pub is_crit: bool,
}

impl DamageEvent {
    /// Convenience for a single-hit event with no modifier stack. Non-crit.
    pub fn hit(attacker: Entity, target: Entity, base: f32, ty: DamageType, quality: f32) -> Self {
        Self {
            attacker,
            target,
            base,
            ty,
            additive: 0.0,
            multipliers: 1.0,
            quality,
            is_crit: false,
        }
    }
}
