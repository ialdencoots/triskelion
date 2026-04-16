use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::combat::{AbilityCooldowns, CombatState, Health};
use super::minigame::{
    arc::ArcState, bar_fill::BarFillState, dag::DagState, heartbeat::HeartbeatState,
    value_lock::ValueLockState, wave_interference::WaveInterferenceState,
};

// ── Identity ──────────────────────────────────────────────────────────────────

/// Lightyear client ID, stored on the player entity for server-side lookups.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerId(pub u64);

/// Display name chosen at spawn.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerName(pub String);

// ── Class / Subclass / Stance ─────────────────────────────────────────────────

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerClass(pub Class);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerSubclass(pub Subclass);

/// The three playable classes, each with two coupled minigame mechanics.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Class {
    /// Arc + DAG. Commit quality drives streak; streak gates DAG modifier paths.
    Physical,
    /// Bar Fill + Wave Interference. Pool fuels wave pressure relief or potential conversion.
    Arcane,
    /// Value Lock + Heartbeat. Locked frequency and entrainment streak scale pulse output.
    Nature,
}

/// One subclass per class per role. Subclass determines stance behaviour and
/// ability flavour, not the underlying minigame mechanic.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Subclass {
    // Physical ─────────────────────────────────────
    /// Tank. Iron Stance. DAG skews toward aggro/stun/reflection at high streak.
    Bulwark,
    /// Healer. Flowing Guard. Chi burst heal is streak-gated; forced prevention-first design.
    Intercessor,
    /// DPS. Edge Form. High-streak DAG paths unlock DoT stacking and multi-hit modifiers.
    Duelist,
    // Arcane ───────────────────────────────────────
    /// Tank. Null Field. Destructive interference orientation; wave accumulation = Pressure.
    Aegis,
    /// Healer. Resonant Flow. Constructive orientation; bonus markers incentivise non-max commits.
    Conduit,
    /// DPS. Overcharge. Super-linear pool scaling above 100%; detonation-window gameplay.
    Arcanist,
    // Nature ───────────────────────────────────────
    /// Tank. Deep Root. Low-frequency operation; large per-commit multiplier, wide windows.
    Wardbark,
    /// Healer. Pulse. Mid-frequency; entrainment streak is continuous healing multiplier.
    Mender,
    /// DPS. Overgrowth. High-frequency DoT; narrow windows, high entrainment demand.
    Thornweave,
}

/// Active role output stances. Entering a stance starts the class minigame.
/// Outside of a stance the player has no output.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Stance {
    // Physical ─────────────────────────────────────
    IronStance,
    FlowingGuard,
    EdgeForm,
    // Arcane ───────────────────────────────────────
    NullField,
    ResonantFlow,
    Overcharge,
    // Nature ───────────────────────────────────────
    DeepRoot,
    Pulse,
    Overgrowth,
}

// ── Position / Velocity ───────────────────────────────────────────────────────

/// Server-authoritative world-space position, replicated every tick.
/// Clients use this for dead-reckoning (same pattern as EnemyPosition).
#[derive(Component, Serialize, Deserialize, Clone, PartialEq)]
pub struct PlayerPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl PlayerPosition {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn to_vec3(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

impl From<Vec3> for PlayerPosition {
    fn from(v: Vec3) -> Self {
        Self { x: v.x, y: v.y, z: v.z }
    }
}

/// XZ and vertical velocity replicated alongside PlayerPosition for client dead-reckoning.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct PlayerVelocity {
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
}

// ── Bundles ───────────────────────────────────────────────────────────────────

/// Components shared by all player entities regardless of class.
#[derive(Bundle)]
pub struct PlayerBaseBundle {
    pub id: PlayerId,
    pub name: PlayerName,
    pub class: PlayerClass,
    pub subclass: PlayerSubclass,
    pub health: Health,
    pub combat: CombatState,
    pub cooldowns: AbilityCooldowns,
    pub transform: Transform,
}

/// Full bundle for a Physical player (Arc + DAG mechanics).
#[derive(Bundle)]
pub struct PhysicalPlayerBundle {
    pub base: PlayerBaseBundle,
    pub arc: ArcState,
    pub dag: DagState,
}

/// Full bundle for an Arcane player (Bar Fill + Wave Interference mechanics).
#[derive(Bundle)]
pub struct ArcanePlayerBundle {
    pub base: PlayerBaseBundle,
    pub bar_fill: BarFillState,
    pub wave: WaveInterferenceState,
}

/// Full bundle for a Nature player (Value Lock + Heartbeat mechanics).
#[derive(Bundle)]
pub struct NaturePlayerBundle {
    pub base: PlayerBaseBundle,
    pub value_lock: ValueLockState,
    pub heartbeat: HeartbeatState,
}
