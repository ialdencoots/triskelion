use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Server-authoritative state for the Physical class DAG mechanic.
///
/// Each action activation presents its own DAG. Flow advances from entry to terminal
/// at a fixed speed regardless of player input. At each branch point the player may
/// press the branch input within a window to select the next path in the rotation;
/// otherwise flow auto-routes to the default path.
///
/// The current Arc streak gates which modifier paths are available at each branch.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DagState {
    /// True while a DAG flow is in progress (i.e., an action has been activated).
    pub active: bool,
    /// Autonomous flow position [0, 1] from entry node to terminal node.
    pub flow_progress: f32,
    /// Index into the current action's branch list of the next upcoming branch point.
    pub next_branch_index: usize,
    /// Modifier set accumulated from branch choices so far this activation.
    /// Applied to the action's base output when the terminal node is reached.
    pub collected_modifiers: Vec<DagModifier>,
}

/// A single modifier delivered by a DAG branch path.
/// The server applies the full `collected_modifiers` set at the terminal node.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum DagModifier {
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
