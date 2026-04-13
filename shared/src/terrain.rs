//! Shared terrain height function used by both the server (AI ground-following)
//! and the client (mesh generation).  Parameters must stay in sync.

use fastnoise_lite::*;

pub const TERRAIN_SCALE: f32 = 1.0;
pub const HEIGHT_SCALE: f32 = 6.0;

/// Returns the terrain surface Y at world position (x, z).
pub fn height_at(x: f32, z: f32) -> f32 {
    let mut noise = FastNoiseLite::new();
    noise.set_noise_type(Some(NoiseType::Perlin));
    noise.set_frequency(Some(0.04));
    noise.set_fractal_type(Some(FractalType::FBm));
    noise.set_fractal_octaves(Some(4));
    noise.set_fractal_lacunarity(Some(2.0));
    noise.set_fractal_gain(Some(0.5));
    noise.get_noise_2d(x, z) * HEIGHT_SCALE
}
