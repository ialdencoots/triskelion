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

/// Replicated mirror of the server-side `MitigationPool` for healers.
/// Synced each tick by `sync_replicated_mitigation_pool` so clients can show
/// the pool fill, drain, and invuln state in the HUD without depending on
/// server-only types. `invuln_active` is a boolean computed server-side from
/// the `now < invuln_until` check, so the client doesn't need server time.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ReplicatedMitigationPool {
    pub amount: f32,
    pub invuln_active: bool,
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

/// Marker inserted by the server when an entity's HP first reaches 0. Replicated
/// to clients so they can react with the death visual. Once present, an entity
/// cannot attack, be attacked, healed, or take damage.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Dead;

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

// ── Enemy abilities ───────────────────────────────────────────────────────────

/// Identifier for every enemy ability. The static parameter table lives in
/// `server::systems::mob_defs::stats_for_ability`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbilityKind {
    /// Instant-resolve melee auto-attack: short cooldown, small Spike disruption.
    MeleeAuto,
    /// Telegraphed radius AoE centered on the locked target's position.
    GroundSlam,
}

/// Shape of an attack resolution against the world.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum AttackShape {
    /// Hits only the locked target entity.
    Single,
    /// Hits everyone within `radius` of the aim point.
    Radius { radius: f32 },
    /// Hits everyone within `range` and within `half_angle` of direction
    /// from the attacker toward the aim point.
    Cone { half_angle: f32 },
}

/// How an enemy picks whom to aim at when a cooldown becomes ready.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum TargetSelector {
    /// Current top-of-threat (falls back to nearest if threat is empty).
    TopThreat,
    /// Everyone in the enemy's instance within range — for AoE abilities
    /// that don't need a specific anchor target; the aim is the enemy itself.
    AllInRange,
}

/// Sharp vs. sustained disruption. Each player class interprets these
/// differently in `apply_disruption_events` — Spike produces a one-shot
/// impulse, Sustained produces a decaying noise floor.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum DisruptionKind {
    Spike,
    Sustained,
}

/// Disruption payload carried by an attack. Magnitude is in [0.0, 1.0];
/// class-specific scalars in `apply_disruption_events` convert to the
/// right units (arc rad/s, heartbeat Hz, bar-fill proportion).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct DisruptionProfile {
    pub kind:      DisruptionKind,
    pub magnitude: f32,
}

/// Static per-ability parameters. Values come from the table in
/// `server::systems::mob_defs::stats_for_ability`; this struct is the
/// shared shape used by the server's ability tick systems.
#[derive(Clone, Copy, Debug)]
pub struct AbilityStats {
    /// Seconds from cast start to resolve. 0 = instant (auto-attack).
    pub telegraph:      f32,
    /// Full cooldown after a clean resolve.
    pub cooldown:       f32,
    /// Cooldown after a line-of-sight break cancels a cast (shorter than
    /// `cooldown` so the mob can re-attempt soon without being permanently
    /// locked out by corner-kiting).
    pub foiled_cd:      f32,
    /// Cooldown after a player Interrupt cancels a cast (longer than
    /// `cooldown` so Interrupt is a real punish).
    pub interrupted_cd: f32,
    pub range:          f32,
    pub shape:          AttackShape,
    pub selector:       TargetSelector,
    pub damage:         f32,
    pub ty:             DamageType,
    pub disruption:     DisruptionProfile,
}
