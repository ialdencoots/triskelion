use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Marker component replicated to every client so the client can identify
/// enemy entities and attach local rendering.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq)]
pub struct EnemyMarker;

/// Display name for an enemy, replicated once at spawn.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq)]
pub struct EnemyName(pub String);

/// World-space position of an enemy, replicated every tick.
///
/// Using individual floats avoids needing bevy's `serialize` feature flag
/// (which would be required for `Vec3: Serialize`).
#[derive(Component, Serialize, Deserialize, Clone, PartialEq)]
pub struct EnemyPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl EnemyPosition {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn to_vec3(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

impl From<Vec3> for EnemyPosition {
    fn from(v: Vec3) -> Self {
        Self { x: v.x, y: v.y, z: v.z }
    }
}

/// Marks a boss-tier enemy. Replicated once at spawn so the client can
/// render it at a larger scale.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq)]
pub struct BossMarker;

/// XZ velocity of an enemy, replicated every tick alongside EnemyPosition.
///
/// Clients use this for dead-reckoning: they extrapolate the enemy's position
/// every frame between server updates instead of snapping.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq)]
pub struct EnemyVelocity {
    pub vx: f32,
    pub vz: f32,
}

/// The `PlayerId` of the player this mob is currently targeting.
/// `None` when the mob is not aggroed or has no valid target.
/// Replicated so clients can display target-of-target when inspecting a mob.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct MobTarget(pub Option<u64>);

/// An enemy's in-flight telegraphed cast. Replicated so clients can render
/// the telegraph geometry (ring for Radius, line for Single, cone for Cone).
/// Server inserts at cast start, despawns on resolve / LoS break / interrupt.
///
/// Fields use individual floats for the aim point to match `EnemyPosition`,
/// avoiding the `bevy/serialize` feature flag that `Vec3: Serialize` needs.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EnemyCast {
    pub ability:  crate::components::combat::AbilityKind,
    /// Shape of the attack at resolve. Replicated (in addition to being
    /// implicit in `ability`) so the client can render the correct telegraph
    /// without mirroring the server's ability parameter table.
    pub shape:    crate::components::combat::AttackShape,
    /// PlayerId of the target locked at cast start. Clients may highlight
    /// this player specifically. Zero means "no single target" (AllInRange).
    pub target:   u64,
    /// Aim point in world space — snapshot of the target's position at cast
    /// start. AoE shapes resolve against this, not against the target's
    /// live position, so kiting after the cast starts doesn't move the hit.
    pub aim_x:    f32,
    pub aim_y:    f32,
    pub aim_z:    f32,
    pub elapsed:  f32,
    pub duration: f32,
}

/// Server-only cooldown tracking for an enemy's abilities. Parallel to the
/// `MobStats.specials` list: index i here tracks the cooldown of
/// `MobStats.specials[i]`. Auto-attack gets its own dedicated field.
#[derive(Component, Debug, Default)]
pub struct EnemyAbilityCooldowns {
    pub auto_cd:     f32,
    pub specials_cd: Vec<f32>,
}
