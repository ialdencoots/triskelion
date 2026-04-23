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
    CrystalGolemLord,
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
    /// When true, terrain generation carves walkable corridors/rooms from the
    /// node graph and raises impassable walls everywhere else.
    pub use_layout_terrain: bool,
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

// ── Layout SDF ────────────────────────────────────────────────────────────────

/// Half-width of corridors connecting nodes (world units).
pub const CORRIDOR_RADIUS: f32 = 5.0;
/// Radius of walkable room circles around Normal nodes.
pub const ROOM_RADIUS: f32 = 9.0;
/// Radius of walkable room circles around BossArena nodes.
pub const ROOM_RADIUS_BOSS: f32 = 14.0;
/// How far walls rise above the Perlin floor outside the layout.
pub const WALL_HEIGHT: f32 = 15.0;
/// Horizontal distance (world units) over which the wall ramps up from 0 to
/// `WALL_HEIGHT`. Should span at least 1–2 vertex steps (tile_scale) so normals
/// are captured correctly. At tile_scale=2 and BLEND_DIST=3 the slope is ~82°.
pub const BLEND_DIST: f32 = 3.0;

/// Signed distance from `(px, pz)` to the walkable layout of `def`.
/// Returns ≤ 0 inside rooms and corridors, > 0 in wall territory.
/// Only meaningful when `def.use_layout_terrain` is true.
pub fn layout_sdf(px: f32, pz: f32, def: &InstanceDef) -> f32 {
    let mut min_d = f32::INFINITY;
    for node in def.nodes {
        let (ax, az) = node.position;
        let r = match node.node_kind {
            NodeKind::BossArena => ROOM_RADIUS_BOSS,
            NodeKind::Normal    => ROOM_RADIUS,
        };
        // Room circle
        min_d = min_d.min(((px - ax).powi(2) + (pz - az).powi(2)).sqrt() - r);
        // Corridor capsule toward each neighbour
        for &ni in node.connects_to {
            let (bx, bz) = def.nodes[ni].position;
            min_d = min_d.min(segment_dist(px, pz, ax, az, bx, bz) - CORRIDOR_RADIUS);
        }
    }
    min_d
}

/// World-space surface height at `(x, z)` for `def`'s terrain.
///
/// For layout instances this is `sample_height + clamp(layout_sdf/BLEND_DIST, 0, 1) * WALL_HEIGHT`,
/// matching the formula in `build_layout_terrain_mesh`. For non-layout instances
/// it is just `sample_height`. Use this anywhere player/mob placement needs to
/// land on the visible mesh — bare `sample_height` underplaces entities into
/// walls in layout instances.
#[inline]
pub fn terrain_surface_y(noise: &FastNoiseLite, x: f32, z: f32, def: &InstanceDef) -> f32 {
    let base = sample_height(noise, x, z, &def.terrain);
    if def.use_layout_terrain {
        let sdf = layout_sdf(x, z, def);
        base + (sdf / BLEND_DIST).clamp(0.0, 1.0) * WALL_HEIGHT
    } else {
        base
    }
}

/// Distance from point `(px, pz)` to the finite line segment `(ax,az)–(bx,bz)`.
fn segment_dist(px: f32, pz: f32, ax: f32, az: f32, bx: f32, bz: f32) -> f32 {
    let dx = bx - ax;
    let dz = bz - az;
    let len_sq = dx * dx + dz * dz;
    if len_sq < 1e-9 {
        return ((px - ax).powi(2) + (pz - az).powi(2)).sqrt();
    }
    let t = (((px - ax) * dx + (pz - az) * dz) / len_sq).clamp(0.0, 1.0);
    let cx = ax + t * dx;
    let cz = az + t * dz;
    ((px - cx).powi(2) + (pz - cz).powi(2)).sqrt()
}
