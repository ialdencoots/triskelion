#![allow(unused_variables)]

use bevy::prelude::*;

use shared::components::minigame::dag::{DagModifier, DagState};
use shared::components::minigame::arc::ArcState;

/// Renders the DAG minigame overlay for Physical-class players.
///
/// Draws:
/// - Node graph with current flow progress marker.
/// - Branch windows (highlighted when open).
/// - Available paths at each branch — dimmed/absent for paths locked by streak tier.
/// - Collected modifier icons along the chosen path so far.
///
/// Requires both `ArcState` (for current streak, to shade unavailable paths)
/// and `DagState` (for flow position and collected modifiers).
pub fn render_dag(query: Query<(&DagState, &ArcState)>) {
}
