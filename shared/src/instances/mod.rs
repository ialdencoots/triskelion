pub mod crystal_caverns;
pub mod overworld;

use fastnoise_lite::*;
use serde::{Deserialize, Serialize};

pub use crystal_caverns::CRYSTAL_CAVERNS;
pub use overworld::OVERWORLD;

// ── Enums ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum InstanceKind {
    Overworld,
    CrystalCaverns,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum MobKind {
    Goblin,
    Orc,
    Troll,
    CrystalGolem,
}

// ── Terrain ──────────────────────────────────────────────────────────────────

/// Noise and geometry parameters for one instance's terrain.
/// Sent from server to client in `InstanceEnteredMsg` so the client can
/// rebuild its terrain mesh without knowing the instance definition.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TerrainConfig {
    pub seed: i32,
    pub frequency: f32,
    pub height_scale: f32,
    pub octaves: i32,
    pub lacunarity: f32,
    pub gain: f32,
    /// Vertex count along each axis of the terrain tile.
    pub size: usize,
    /// World-space distance between adjacent vertices.
    pub tile_scale: f32,
}

impl Default for TerrainConfig {
    /// Matches the existing hardcoded constants so the game plays identically
    /// before any instance-specific config is applied.
    fn default() -> Self {
        Self {
            seed: 0,
            frequency: 0.04,
            height_scale: 6.0,
            octaves: 4,
            lacunarity: 2.0,
            gain: 0.5,
            size: 128,
            tile_scale: 1.0,
        }
    }
}

/// Build a `FastNoiseLite` configured from `cfg`.
/// Cache the result in `LiveInstance` / `CurrentInstanceTerrain`; never call
/// inside a per-frame hot path.
pub fn build_noise(cfg: &TerrainConfig) -> FastNoiseLite {
    let mut noise = FastNoiseLite::with_seed(cfg.seed);
    noise.set_noise_type(Some(NoiseType::Perlin));
    noise.set_frequency(Some(cfg.frequency));
    noise.set_fractal_type(Some(FractalType::FBm));
    noise.set_fractal_octaves(Some(cfg.octaves));
    noise.set_fractal_lacunarity(Some(cfg.lacunarity));
    noise.set_fractal_gain(Some(cfg.gain));
    noise
}

/// Sample the terrain height at world position (x, z).
/// `noise` must have been built with `build_noise(cfg)`.
#[inline]
pub fn sample_height(noise: &FastNoiseLite, x: f32, z: f32, cfg: &TerrainConfig) -> f32 {
    noise.get_noise_2d(x, z) * cfg.height_scale
}

// ── Instance definition ───────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct InstanceDef {
    pub kind: InstanceKind,
    pub terrain: TerrainConfig,
    /// Ordered list of path nodes.  Index 0 is always the entry/spawn point.
    pub nodes: &'static [PathNode],
    pub max_players: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct PathNode {
    /// World-space XZ position of this node (relative to instance center).
    pub position: (f32, f32),
    /// Optional mob pack placed at this node.
    pub pack: Option<PackDef>,
    pub node_kind: NodeKind,
    /// Outgoing edges: indices into the parent `InstanceDef::nodes` slice.
    pub connects_to: &'static [usize],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Normal,
    /// Boss arena — `connects_to` should be empty.
    BossArena,
}

#[derive(Clone, Copy, Debug)]
pub struct PackDef {
    pub mobs: &'static [MobSpawn],
    /// Scatter radius in world units around the node position.
    pub spread: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct MobSpawn {
    pub kind: MobKind,
    pub count: u8,
}

// ── Registry ──────────────────────────────────────────────────────────────────

pub const INSTANCE_DEFS: &[InstanceDef] = &[OVERWORLD, CRYSTAL_CAVERNS];

pub fn find_def(kind: InstanceKind) -> &'static InstanceDef {
    INSTANCE_DEFS
        .iter()
        .find(|d| d.kind == kind)
        .expect("InstanceKind missing from INSTANCE_DEFS")
}
