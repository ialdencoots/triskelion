#![allow(unused_variables)]

use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::inputs::{AbilityInput, MinigameInput, PlayerInput};

/// Reads keyboard/gamepad state each frame, constructs a `PlayerInput`, and
/// sends it to the server as a lightyear message.
///
/// The server is authoritative — the client never applies these inputs locally
/// to game state. The input is sent and the client waits for the replicated
/// server state to come back.
pub fn gather_and_send_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut sender_query: Query<&mut MessageSender<PlayerInput>>,
) {
}
