//! Shared terrain height function used by both the server (AI ground-following)
//! and the client (mesh generation).
//!
//! **Shim:** `height_at(x, z)` delegates to `instances::sample_height` using
//! the default `TerrainConfig`.  All call sites will migrate to
//! `instances::sample_height` with a cached noise object in Phase 4.

use crate::instances::{build_noise, sample_height, TerrainConfig};

/// Returns the terrain surface Y at world position (x, z), using the default
/// `TerrainConfig`.  Rebuilds the noise object on every call — kept for
/// backwards compatibility during migration; prefer `sample_height` with a
/// cached `FastNoiseLite`.
#[deprecated(note = "Use instances::sample_height with a cached noise object")]
pub fn height_at(x: f32, z: f32) -> f32 {
    let cfg = TerrainConfig::default();
    let noise = build_noise(&cfg);
    sample_height(&noise, x, z, &cfg)
}
