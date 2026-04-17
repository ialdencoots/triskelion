use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::player::RoleStance;

/// Hit points. Replicated server-to-client; interpolated for smooth display.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn is_alive(&self) -> bool {
        self.current > 0.0
    }
}

impl Default for Health {
    fn default() -> Self {
        Self::new(100.0)
    }
}

/// Tracks whether this player is currently in combat and which role stance, if
/// any, is active.  Exiting a stance suspends the minigame and starts `stance_cd`.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CombatState {
    pub in_combat: bool,
    pub active_stance: Option<RoleStance>,
}

/// Remaining cooldown in seconds for each ability slot.
/// The server decrements these each tick; values are replicated to the client for UI display.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct AbilityCooldowns {
    /// Lunge / Phase Step / Burrow
    pub mobility_cd: f32,
    /// Stun / Displacement / Root
    pub cc_cd: f32,
    pub taunt_cd: f32,
    pub interrupt_cd: f32,
    /// Delay before re-entering a stance after exiting one.
    pub stance_cd: f32,
}
