#![allow(unused_variables)]

use bevy::prelude::*;

use shared::components::minigame::cube::{CubeState, PhysicalBonus};
use shared::components::minigame::arc::ArcState;

/// Renders the Tank/Healer cube overlay.
///
/// Draws:
/// - Cube frame around the arc with left/bottom/right interactive edges.
/// - Edge fill animation sweeping from corners to edge centers.
/// - Bonus marker at each edge center (current face).
/// - Rotation animation on successful collect.
/// - Collected bonus list and rotations-remaining counter.
///
/// Requires both `ArcState` (for context on the streak that triggered this cube)
/// and `CubeState` (for current face, fill progress, and collected bonuses).
pub fn render_cube(query: Query<(&CubeState, &ArcState)>) {
}
