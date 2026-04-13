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
