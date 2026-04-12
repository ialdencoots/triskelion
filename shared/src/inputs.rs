use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// All inputs a player can submit to the server in a single tick.
/// Sent as a lightyear message each tick while connected.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PlayerInput {
    /// 2D movement direction. The server normalizes this before applying.
    pub movement: Vec2,
    /// Discrete ability button presses this tick.
    pub abilities: AbilityInput,
    /// Minigame mechanic input events this tick.
    pub minigame: MinigameInput,
}

/// One bit per ability slot; true = pressed this tick.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct AbilityInput {
    pub use_stance: bool,
    pub exit_stance: bool,
    pub use_mobility: bool,
    pub use_cc: bool,
    pub use_taunt: bool,
    pub use_interrupt: bool,
}

/// Minigame mechanic inputs shared across all classes.
/// Each flag is true if the corresponding action was pressed this tick.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct MinigameInput {
    /// Arc commit (Physical) / Bar Fill commit (Arcane) / Heartbeat commit (Nature).
    pub commit: bool,
    /// DAG branch selection (Physical only).
    pub branch: bool,
    /// Value Lock hold state (Nature only); true while button is held down.
    pub hold: bool,
}
