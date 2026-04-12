#![allow(unused_variables)]

use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::{AbilityCooldowns, CombatState};
use shared::components::player::PlayerId;
use shared::inputs::PlayerInput;

/// Read buffered `PlayerInput` messages from all connected clients and apply them:
/// movement, ability activations, stance entry/exit.
/// Minigame mechanic inputs (commit, branch, hold) are processed separately
/// in the minigame tick systems.
pub fn process_player_inputs(
    input_receivers: Query<&mut MessageReceiver<PlayerInput>>,
    players: Query<(&PlayerId, &mut CombatState, &mut AbilityCooldowns, &mut Transform)>,
) {
}

/// Decrement all ability cooldowns by the elapsed fixed-timestep delta.
pub fn tick_ability_cooldowns(time: Res<Time>, mut query: Query<&mut AbilityCooldowns>) {}
