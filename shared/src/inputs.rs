use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::player::RoleStance;

/// All inputs a player can submit to the server in a single tick.
/// Sent as a lightyear message each tick while connected.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PlayerInput {
    /// 2D movement direction. The server normalizes this before applying velocity.
    pub movement: Vec2,
    /// Client physics body XZ position this tick, relayed via `PlayerPosition` so
    /// other clients see the authoritative position rather than a server-integrated
    /// approximation that drifts from the client's physics simulation over time.
    pub x: f32,
    pub z: f32,
    /// Client physics body Y position this tick, used to relay vertical motion
    /// (jumps, falling) to other clients via the replicated `PlayerPosition`.
    pub y: f32,
    /// Client physics body vertical velocity this tick, relayed via `PlayerVelocity`
    /// so remote clients can dead-reckon Y between server updates.
    pub vy: f32,
    /// Discrete ability button presses this tick.
    pub abilities: AbilityInput,
    /// Minigame mechanic input events this tick.
    pub minigame: MinigameInput,
    /// Camera yaw (radians) this tick — used server-side for facing-direction checks.
    pub facing_yaw: f32,
}

/// Ability inputs for one tick.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct AbilityInput {
    /// Stance to enter this tick (1 = Tank, 2 = DPS, 3 = Heal).
    /// `None` means no stance change was requested.
    pub enter_stance: Option<RoleStance>,
    /// True when the player pressed Escape to exit their current stance.
    pub exit_stance: bool,
    /// Gap-closer or repositioning tool.
    pub ability_1: bool,
    /// Crowd control or disable.
    pub ability_2: bool,
    /// Threat management, support, or utility.
    pub ability_3: bool,
    /// Reaction, interrupt, or burst.
    pub ability_4: bool,
}

/// Minigame mechanic inputs for one tick.
/// Any combination may be true simultaneously; held inputs remain true across ticks.
/// The server resolves the meaning of each slot per class.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct MinigameInput {
    /// Primary timing or commit action.
    pub action_1: bool,
    /// Secondary mechanic input.
    pub action_2: bool,
    /// Additional mechanic inputs.
    pub action_3: bool,
    pub action_4: bool,
    pub action_5: bool,
    /// Sixth slot (`SecondaryUp`, default Q). Used as the grid's Up direction
    /// for the Duelist; no-op for Tank/Heal stances.
    pub action_6: bool,
}
