use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::player::RoleStance;

/// Replicated threat table for a mob — sent to clients for threat display.
/// Uses PlayerId (u64) instead of server-internal Entity.
/// Entries are sorted descending by threat value.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ReplicatedThreatList {
    pub entries: Vec<(u64, f32)>,
}

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

/// Kind of damage produced by an action. Mirrors the three playable classes so
/// damage carries class identity through the formula.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamageType {
    Physical,
    Arcane,
    Nature,
}

/// Per-type damage reduction on a target, in [0.0, 0.75]. Replicated so the
/// client can surface DR in UI. Values are clamped by [`Resistances::new`].
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Resistances {
    pub physical: f32,
    pub arcane: f32,
    pub nature: f32,
}

impl Resistances {
    pub const MAX: f32 = 0.75;

    pub fn new(physical: f32, arcane: f32, nature: f32) -> Self {
        Self {
            physical: physical.clamp(0.0, Self::MAX),
            arcane:   arcane.clamp(0.0, Self::MAX),
            nature:   nature.clamp(0.0, Self::MAX),
        }
    }

    pub fn get(&self, ty: DamageType) -> f32 {
        match ty {
            DamageType::Physical => self.physical,
            DamageType::Arcane   => self.arcane,
            DamageType::Nature   => self.nature,
        }
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
