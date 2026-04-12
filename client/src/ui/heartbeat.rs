#![allow(unused_variables)]

use bevy::prelude::*;

use shared::components::minigame::heartbeat::HeartbeatState;
use shared::components::minigame::value_lock::ValueLockState;

/// Renders the Heartbeat minigame overlay for Nature-class players.
///
/// Draws:
/// - The pulsing circle with radius driven by the cardiac envelope `r(t_n)`.
/// - Primary spike window and secondary echo window highlighted.
/// - Current and target frequency indicator (shows interpolation gap).
/// - Envelope noise visual (distortion on the circle outline under disruption).
/// - Lockout indicator.
///
/// Reads both `HeartbeatState` and `ValueLockState` so the player can see
/// the target frequency decay and entrainment streak alongside the live beat.
pub fn render_heartbeat(query: Query<(&HeartbeatState, &ValueLockState)>) {
    todo!()
}
