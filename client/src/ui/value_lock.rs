#![allow(unused_variables)]

use bevy::prelude::*;

use shared::components::minigame::value_lock::ValueLockState;

/// Renders the Value Lock minigame overlay for Nature-class players.
///
/// Draws:
/// - The horizontal bar showing current `hold_progress` while held.
/// - A marker at `locked_value` (previous lock, held for reference).
/// - A ghost marker at `previous_locked` showing the entrainment target zone (±delta).
/// - Entrainment streak counter.
/// - Target frequency readout and its decay progress.
pub fn render_value_lock(query: Query<&ValueLockState>) {
}
