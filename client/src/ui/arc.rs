#![allow(unused_variables)]

use bevy::prelude::*;

use shared::components::minigame::arc::ArcState;

/// Client-local ghost arc history entry.
/// One entry is pushed per commit; the list is capped at MAX_GHOST_ENTRIES.
/// Not replicated — maintained purely for rendering.
#[derive(Component, Default)]
pub struct GhostArcHistory {
    /// Theta angle at each recent commit, most recent first.
    pub entries: Vec<f32>,
}

pub const MAX_GHOST_ENTRIES: usize = 6;

/// Renders the Arc minigame overlay for Physical-class players.
///
/// Draws:
/// - The semicircular track.
/// - The moving dot at `arc.theta`.
/// - Zone boundaries (nadir/mid/apex bands).
/// - The ghost arc history stack below the live arc.
/// - Lockout indicator.
pub fn render_arc(query: Query<(&ArcState, Option<&GhostArcHistory>)>) {
    todo!()
}
