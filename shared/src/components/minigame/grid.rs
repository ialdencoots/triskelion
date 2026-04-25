use std::collections::VecDeque;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::cube::PhysicalBonus;

/// Default grid dimensions. Wider than tall so the path reads left-to-right;
/// odd row count puts entry/exit on a centered middle row.
pub const GRID_COLS_DEFAULT: u8 = 5;
pub const GRID_ROWS_DEFAULT: u8 = 3;
/// Random per-activation dimension ranges. Always odd row count so the entry
/// and exit cells sit centered, regardless of which size rolled.
pub const GRID_COLS_MIN: u8 = 4;
pub const GRID_COLS_MAX: u8 = 6;
pub const GRID_ROWS_MIN: u8 = 3;
pub const GRID_ROWS_MAX: u8 = 5;

/// Probability a non-entry cell carries a regular bonus.
pub const GRID_BONUS_DENSITY: f32 = 0.40;
/// Probability a non-entry cell carries a structural special node
/// (Echo / Mimic / Anchor) — applied after the bonus roll fails.
pub const GRID_SPECIAL_DENSITY: f32 = 0.15;

/// Minimum `shared_delta` value at which a streak-break fires the grid.
/// Trivial sub-threshold streak losses don't fire — both because tiny grids
/// always blow out and because we don't want small-recovery primary deltas
/// blocking break-triggers from the other arc. See the
/// `project_grid_activation_model` memory.
pub const MIN_GRID_BUDGET: u32 = 4;

/// Seconds the grid lingers post-resolution to play the shatter animation
/// before it actually clears. Set on Blowout/AnchorSaved; Exit clears
/// immediately (the player succeeded — no need to dwell on it).
pub const GRID_BLOWOUT_ANIM_SECS: f32 = 0.55;

/// Edge of the grid the player enters from or exits through.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridEdge {
    Top,
    Right,
    #[default]
    Bottom,
    Left,
}

impl GridEdge {
    pub fn opposite(self) -> Self {
        match self {
            GridEdge::Top => GridEdge::Bottom,
            GridEdge::Bottom => GridEdge::Top,
            GridEdge::Left => GridEdge::Right,
            GridEdge::Right => GridEdge::Left,
        }
    }
}

/// One of the four cardinal moves a player can make on the grid.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridDir {
    Up,
    Right,
    Down,
    Left,
}

impl GridDir {
    /// Apply this direction to a `(col, row)` cursor, returning the next signed
    /// position. Callers convert back to `u8` and check bounds.
    pub fn step(self, cursor: (u8, u8)) -> (i32, i32) {
        let (c, r) = (cursor.0 as i32, cursor.1 as i32);
        match self {
            GridDir::Up => (c, r - 1),
            GridDir::Down => (c, r + 1),
            GridDir::Left => (c - 1, r),
            GridDir::Right => (c + 1, r),
        }
    }
}

/// Contents of a single grid cell. Empty/Bonus are the standard variants;
/// Echo / Mimic / Anchor are structural specials that alter routing payouts
/// rather than directly delivering a modifier.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub enum GridNode {
    #[default]
    Empty,
    Bonus(PhysicalBonus),
    /// Duplicates the modifier of the last bonus visited (no-op if none).
    Echo,
    /// On exit, adopts the type of the most-frequently-collected modifier on
    /// the run; magnitude is read from the snapshot at this step's index.
    Mimic,
    /// Designates the current collected pool as a floor against blowout.
    /// Most recent Anchor supersedes earlier ones.
    Anchor,
}

/// How a grid run resolved. Drained by `tick_grid_states` after the move that
/// produced it; the system logs the collected list and clears active state.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridResolution {
    /// Player reached the exit edge — full collected list is paid out.
    Exit,
    /// Dead-end with no anchor floor; everything is lost.
    Blowout,
    /// Dead-end after passing an Anchor; the floored list is paid out.
    AnchorSaved,
}

/// Server-authoritative state for the Duelist traversal grid overlay.
///
/// Activates on either a streak-break (apex commit) that drops `shared_delta`
/// below `MIN_GRID_BUDGET`, or on a streak-cap event where `shared_delta`
/// reaches `GRID_CRITICAL_MASS_CAP`. While active, both arcs' commit input is
/// suspended (their oscillation continues to tick). The player routes
/// directionally through the grid; reaching the exit edge cashes out, a
/// dead-end blowouts (or pays out the anchor floor if one was passed).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct GridState {
    /// True while a grid activation is in progress.
    pub active: bool,
    /// Active grid dimensions; both ≤ their respective MAX constants.
    pub cols: u8,
    pub rows: u8,
    /// Edge the cursor entered from. Always opposite of `exit_edge`.
    pub entry_edge: GridEdge,
    /// Edge the cursor must reach to cash out. Always opposite of `entry_edge`.
    pub exit_edge: GridEdge,
    /// Cell contents indexed `grid[row][col]`. Sized exactly `rows × cols`.
    pub grid: Vec<Vec<GridNode>>,
    /// `(col, row)` cells visited in order, including entry. The single
    /// no-revisit check enforces both no-backtrack and no-self-intersection.
    pub path: Vec<(u8, u8)>,
    /// Current cursor position.
    pub cursor: (u8, u8),
    /// Steps remaining; budget at activation = activating shared_delta.
    pub steps_remaining: u32,
    /// Bonuses collected so far this run, with their per-step magnitudes
    /// looked up from `quality_history`.
    pub collected: Vec<(PhysicalBonus, f32)>,
    /// If a Mimic or final-resolve adoption is needed, the floor at the time
    /// the most recent Anchor was passed. Replaces `collected` on
    /// `AnchorSaved` resolutions.
    pub anchor_floor: Option<Vec<(PhysicalBonus, f32)>>,
    /// Most-recent `(bonus, magnitude)` collected — used by Echo cells.
    pub last_bonus: Option<(PhysicalBonus, f32)>,
    /// Snapshot of the winning arc's commit-quality history at activation.
    /// Used directly for per-step magnitude lookups; live arc commits would
    /// otherwise pollute the buffer (and arc commits are suspended anyway).
    pub quality_history: VecDeque<f32>,
    /// Set by `process_grid_move` when the run resolves; drained by
    /// `tick_grid_states` which logs/applies the result and clears state.
    pub resolved: Option<GridResolution>,
    /// Seconds remaining before a blowout/anchor-saved resolution actually
    /// clears the grid. While > 0 the client plays a shatter animation; once
    /// it hits 0 the cleanup runs and `active` flips false. Exit resolutions
    /// set this to 0 and clear immediately.
    pub resolve_anim_remaining: f32,
}

/// Player-level handoff for break-trigger grid activation.
///
/// The cross-arc apex-break detection lives at the call site in
/// `process_player_inputs` (where both arcs are visible). When a meaningful
/// streak loss is detected, the budget and a snapshot of the winning arc's
/// pre-apex commit history are stashed here. `tick_grid_states` consumes them
/// on its next run and clears the fields.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DpsGridTrigger {
    pub pending_break_budget: Option<u32>,
    pub pending_break_history: Option<VecDeque<f32>>,
}

impl DpsGridTrigger {
    pub fn clear(&mut self) {
        self.pending_break_budget = None;
        self.pending_break_history = None;
    }
}
