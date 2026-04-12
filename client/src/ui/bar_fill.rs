#![allow(unused_variables)]

use bevy::prelude::*;

use shared::components::minigame::bar_fill::BarFillState;

/// Renders the Bar Fill minigame overlay for Arcane-class players.
///
/// Draws:
/// - The horizontal fill bar with the current fill level.
/// - Bonus marker indicators at their fill positions.
/// - The Arcane Pool meter (0–500%).
/// - Visual cue when fill is near a bonus marker's position (±delta).
pub fn render_bar_fill(query: Query<&BarFillState>) {
    todo!()
}
