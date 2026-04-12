#![allow(unused_variables)]

use bevy::prelude::*;

use shared::components::minigame::wave_interference::{WaveInterferenceState, WaveOrientation};
use shared::components::minigame::bar_fill::BarFillState;

/// Renders the Wave Interference minigame overlay for Arcane-class players.
///
/// Draws:
/// - Incoming traveling wave (raw, outside the commit zone).
/// - Commit zone with live interference result (tw + sw).
/// - Committed segments traveling leftward through the zone.
/// - Wave Accumulation meter (Pressure or Potential label depends on orientation).
/// - Disruption state visualised as noise on the standing wave.
///
/// Reads both `WaveInterferenceState` and `BarFillState` since pool level
/// affects spending decisions the player may want to see together.
pub fn render_wave_interference(query: Query<(&WaveInterferenceState, &BarFillState)>) {
}
