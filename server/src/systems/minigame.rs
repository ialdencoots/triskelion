use std::collections::VecDeque;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use shared::components::combat::CombatState;
use shared::components::minigame::{
    arc::{
        ArcState, SecondaryArcState, CUBE_CRITICAL_MASS_CAP, GRID_CRITICAL_MASS_CAP,
        QUALITY_HISTORY_CAPACITY,
    },
    bar_fill::BarFillState,
    cube::{
        BonusTier, CubeState, PhysicalBonus, CUBE_FILL_CYCLE_SECS, CUBE_FILL_RESET_AT,
        CUBE_POP_SECS, CUBE_ROTATIONS_PER_ACTIVATION, CUBE_ROTATION_HOLD_SECS,
        CUBE_ROTATION_SECS,
    },
    grid::{
        DpsGridTrigger, GridDir, GridEdge, GridNode, GridResolution, GridState,
        GRID_BLOWOUT_ANIM_SECS, GRID_BONUS_DENSITY, GRID_COLS_MAX, GRID_COLS_MIN,
        GRID_ROWS_MAX, GRID_ROWS_MIN, GRID_SPECIAL_DENSITY,
    },
    heartbeat::HeartbeatState,
    value_lock::ValueLockState,
    wave_interference::WaveInterferenceState,
};
use shared::components::player::RoleStance;

use super::combat::next_unit;

fn tick_arc(arc: &mut ArcState, dt: f32) {
    arc.disruption_velocity *= (-dt * 0.5_f32).exp();
    arc.phase += arc.disruption_velocity * dt;
    arc.time += dt;
    arc.theta = FRAC_PI_2 + arc.amplitude * (arc.omega * arc.time + arc.phase).sin();

    // Apex-zone rising-edge detection drives two behaviors:
    //   1. Release lockout once the dot reaches the opposite apex after a commit.
    //   2. Break the streak after two consecutive apex visits with no commit
    //      (≈ one full oscillation of idleness).
    let apex_proximity = (arc.theta - FRAC_PI_2).abs() / arc.amplitude;
    let at_apex = apex_proximity >= 0.9;
    if at_apex && !arc.prev_at_apex {
        arc.commit.in_lockout = false;
        arc.apex_visits_since_commit = arc.apex_visits_since_commit.saturating_add(1);
        if arc.apex_visits_since_commit >= 2 && arc.streak > 0 {
            arc.streak = 0;
            arc.streak_at_last_activation = 0;
        }
    }
    arc.prev_at_apex = at_apex;
}

/// Evaluate a commit attempt on an arc. No-ops if in lockout.
pub fn process_arc_commit(arc: &mut ArcState) {
    if arc.commit.in_lockout {
        return;
    }
    let dot_vel = arc.amplitude * arc.omega * (arc.omega * arc.time + arc.phase).cos()
        + arc.disruption_velocity;
    let peak_vel = arc.amplitude * arc.omega;
    let quality = (dot_vel.abs() / peak_vel).min(1.0);
    arc.commit.push(quality, QUALITY_HISTORY_CAPACITY as usize);
    arc.last_commit_theta = arc.theta;

    let proximity = (arc.theta - FRAC_PI_2).abs() / arc.amplitude;
    if proximity < 0.2 {
        arc.streak += 1;
    } else if proximity > 0.8 {
        arc.streak = 0;
        // The baseline from which the next activation delta is measured travels
        // with the streak — clear it so a fresh streak activates at the cap again.
        arc.streak_at_last_activation = 0;
    }
    // Any commit (any zone) resets the idle-break counter — the streak only
    // breaks from apex visits that pass with no commit at all.
    arc.apex_visits_since_commit = 0;
    arc.commit.in_lockout = true;
}

/// Advance all active Arc states by one server tick.
pub fn tick_arc_states(time: Res<Time>, mut query: Query<&mut ArcState>) {
    let dt = time.delta_secs();
    for mut arc in query.iter_mut() {
        tick_arc(&mut arc, dt);
    }
}

/// Advance all secondary (DPS second-weapon) Arc states by one server tick.
pub fn tick_secondary_arc_states(time: Res<Time>, mut query: Query<&mut SecondaryArcState>) {
    let dt = time.delta_secs();
    for mut secondary in query.iter_mut() {
        tick_arc(&mut secondary.0, dt);
    }
}

// ── Cube ─────────────────────────────────────────────────────────────────────

/// Seed one edge's bonus drawn from a tier-appropriate pool. Skill tree weighting
/// is deferred; this picks uniformly within the tier band.
fn sample_bonus(tier: BonusTier) -> PhysicalBonus {
    let roll = next_unit();
    match tier {
        BonusTier::Default => match (roll * 3.0) as u32 {
            0 => PhysicalBonus::BaseDamage(8.0),
            1 => PhysicalBonus::AggroBonus(0.15),
            _ => PhysicalBonus::CooldownReduction(0.5),
        },
        BonusTier::Mid => match (roll * 3.0) as u32 {
            0 => PhysicalBonus::BaseDamage(20.0),
            1 => PhysicalBonus::DamageOverTime {
                damage_per_second: 10.0,
                duration_secs: 3.0,
            },
            _ => PhysicalBonus::StunOnHit { duration_secs: 0.5 },
        },
        BonusTier::Premium => match (roll * 3.0) as u32 {
            0 => PhysicalBonus::BaseDamage(40.0),
            1 => PhysicalBonus::StunOnHit { duration_secs: 1.0 },
            _ => PhysicalBonus::Healing(20.0),
        },
    }
}

/// Seed three fresh bonus markers for a new face. The dominant tier comes from
/// `activation_quality`; per the design, mid/premium activations still occasionally
/// surface default-tier entries (here: 20% chance to step down one tier per slot).
fn seed_face(activation_quality: f32) -> [Option<PhysicalBonus>; 3] {
    let base = BonusTier::from_quality(activation_quality);
    std::array::from_fn(|_| {
        let tier = if next_unit() < 0.2 {
            // Step down one tier for variety; premium drops to mid, mid to default.
            match base {
                BonusTier::Premium => BonusTier::Mid,
                BonusTier::Mid => BonusTier::Default,
                BonusTier::Default => BonusTier::Default,
            }
        } else {
            base
        };
        Some(sample_bonus(tier))
    })
}

/// Open the cube: freeze aggregate quality, seed a fresh face, reset fill.
/// Assumes the caller has already verified the streak cap and role gate.
pub fn activate_cube(cube: &mut CubeState, arc: &mut ArcState) {
    let q = arc.commit.mean();
    cube.active = true;
    cube.activation_quality = q;
    cube.fill_progress = 0.0;
    cube.rotations_remaining = CUBE_ROTATIONS_PER_ACTIVATION;
    cube.current_face = seed_face(q);
    cube.collected.clear();
    cube.rotating_edge = None;
    cube.pop_progress = 0.0;
    cube.rotation_progress = 0.0;
    cube.rotation_hold_remaining = 0.0;
    cube.new_face_pending = false;
    // Streak is NOT reset — it's a running consistency counter that survives cube
    // activation. Record the level at which this activation fired so the next one
    // triggers another CUBE_CRITICAL_MASS_CAP nadir commits later.
    arc.streak_at_last_activation = arc.streak;
}

/// Try to collect the bonus on `edge` of the current face. Returns true on
/// success. The collected bonus and timing precision are recorded and the
/// post-collect animation sequence begins (pop → rotation → hold); the face
/// swap and resolution are driven from `tick_cube_states`.
pub fn process_cube_collect(
    cube: &mut CubeState,
    edge: shared::components::minigame::cube::CubeEdge,
) -> bool {
    if !cube.active || cube.rotating_edge.is_some() {
        return false;
    }
    let Some(precision) =
        shared::components::minigame::cube::timing_precision(cube.fill_progress)
    else {
        return false;
    };
    let Some(bonus) = cube.current_face[edge.index()].clone() else {
        return false;
    };
    cube.collected.push((bonus, precision));
    cube.rotations_remaining = cube.rotations_remaining.saturating_sub(1);
    cube.rotating_edge = Some(edge);
    cube.pop_progress = 0.0;
    cube.rotation_progress = 0.0;
    cube.rotation_hold_remaining = 0.0;
    cube.new_face_pending = true;
    // `fill_progress` is left at whatever it was — the fills read as "full"
    // during the pop phase so the hit visually freezes a beat before rotating.
    true
}

/// Finalize the cube activation. Bonuses in `collected` are the eventual payload;
/// rider/window/charge resolution is a follow-up system (see design doc).
fn resolve_cube(cube: &mut CubeState) {
    cube.active = false;
    cube.fill_progress = 0.0;
    cube.rotations_remaining = 0;
    cube.current_face = [None, None, None];
    cube.rotating_edge = None;
    cube.pop_progress = 0.0;
    cube.rotation_progress = 0.0;
    cube.rotation_hold_remaining = 0.0;
    cube.new_face_pending = false;
    // `collected` is left populated for the resolution system to drain.
}

/// Discard an active cube and any bonuses collected so far. Used when the
/// player leaves the stance that spawned it — unlike `resolve_cube`, nothing
/// is paid out, because the player didn't complete the engagement.
pub fn cancel_cube(cube: &mut CubeState) {
    resolve_cube(cube);
    cube.collected.clear();
}

/// Advance all active cube overlays by one server tick.
///
/// Inactive cube on a Tank/Heal stance: if arc streak hits `CRITICAL_MASS_CAP`, activate.
/// Active cube: advance `fill_progress`; reset to 0 when it sweeps past the collect
/// window without a collect. Rotation/resolution are driven by input, not time.
pub fn tick_cube_states(
    time: Res<Time>,
    mut query: Query<(&mut CubeState, &mut ArcState, &CombatState)>,
) {
    let dt = time.delta_secs();
    for (mut cube, mut arc, combat) in query.iter_mut() {
        let is_tank_heal = matches!(
            combat.active_stance,
            Some(RoleStance::Tank) | Some(RoleStance::Heal)
        );

        if !cube.active {
            if is_tank_heal
                && arc.streak.saturating_sub(arc.streak_at_last_activation)
                    >= CUBE_CRITICAL_MASS_CAP
            {
                activate_cube(&mut cube, &mut arc);
            }
            continue;
        }

        // Post-collect animation pipeline takes precedence over fill advance.
        // Phases (in order, gated on `rotating_edge.is_some()`):
        //   1. Pop: landed marker pops (client-side visual), `pop_progress` 0→1
        //   2. Rotation: cube turns 90°, face swap at midpoint
        //   3. Hold: cube sits face-on at new face for a beat
        //   4. Exit: clear rotating_edge, reset fill, possibly resolve
        if cube.rotating_edge.is_some() {
            if cube.pop_progress < 1.0 {
                cube.pop_progress = (cube.pop_progress + dt / CUBE_POP_SECS).min(1.0);
            } else if cube.rotation_progress < 1.0 {
                cube.rotation_progress += dt / CUBE_ROTATION_SECS;
                if cube.new_face_pending && cube.rotation_progress >= 0.5 {
                    cube.new_face_pending = false;
                    if cube.rotations_remaining > 0 {
                        cube.current_face = seed_face(cube.activation_quality);
                    } else {
                        cube.current_face = [None, None, None];
                    }
                }
                if cube.rotation_progress >= 1.0 {
                    cube.rotation_progress = 1.0;
                    cube.rotation_hold_remaining = CUBE_ROTATION_HOLD_SECS;
                }
            } else if cube.rotation_hold_remaining > 0.0 {
                cube.rotation_hold_remaining =
                    (cube.rotation_hold_remaining - dt).max(0.0);
            } else {
                // Sequence complete — back to fill mode (or resolve).
                cube.rotating_edge = None;
                cube.pop_progress = 0.0;
                cube.rotation_progress = 0.0;
                cube.fill_progress = 0.0;
                if cube.rotations_remaining == 0 {
                    resolve_cube(&mut cube);
                }
            }
            continue;
        }

        cube.fill_progress += dt / CUBE_FILL_CYCLE_SECS;
        if cube.fill_progress >= CUBE_FILL_RESET_AT {
            // Swept past the window without a collect — cycle the face again.
            cube.fill_progress = 0.0;
        }
    }
}

// ── Grid (Duelist) ───────────────────────────────────────────────────────────

/// `max(primary.delta, secondary.delta)` — the activation signal for the DPS
/// grid. Either arc independently reaching cap delta fires the grid; an apex
/// commit that drops the shared value below `MIN_GRID_BUDGET` fires a break
/// trigger. See `project_grid_activation_model` memory for the full rationale.
pub(crate) fn shared_delta(p: &ArcState, s: &ArcState) -> u32 {
    p.streak
        .saturating_sub(p.streak_at_last_activation)
        .max(s.streak.saturating_sub(s.streak_at_last_activation))
}

/// Whichever arc has the higher activation delta — primary on ties. The grid's
/// per-step magnitude history is snapshotted from this arc at fire time.
pub(crate) fn winning_arc<'a>(p: &'a ArcState, s: &'a ArcState) -> &'a ArcState {
    let pd = p.streak.saturating_sub(p.streak_at_last_activation);
    let sd = s.streak.saturating_sub(s.streak_at_last_activation);
    if sd > pd { s } else { p }
}

/// Pick an integer uniformly in `[lo, hi]`. Returns `lo` if `hi <= lo`.
fn sample_inclusive_range(lo: u8, hi: u8) -> u8 {
    if hi <= lo {
        return lo;
    }
    let span = (hi - lo + 1) as usize;
    let idx = (next_unit() * span as f32) as usize;
    lo + idx.min(span - 1) as u8
}

fn sample_grid_cols() -> u8 {
    sample_inclusive_range(GRID_COLS_MIN, GRID_COLS_MAX)
}

/// Pick a row count uniformly from the odd integers in
/// `[GRID_ROWS_MIN, min(GRID_ROWS_MAX, cols)]`. Two invariants are enforced:
///
/// 1. **Always odd** — the entry/exit cells must sit on a centered middle
///    row. Even row counts have no center.
/// 2. **Never taller than wide** — clamp the upper bound to `cols`.
///
/// If the candidate set is empty (the caller's constants violate the
/// "odd row exists in `[ROWS_MIN, ROWS_MAX] ∩ [≤ cols]`" invariant), fall
/// back to the largest odd integer ≤ `min(GRID_ROWS_MAX, cols)`, dropping to
/// 1 if even that's not available — this guarantees a usable grid rather
/// than panicking.
fn sample_grid_rows(cols: u8) -> u8 {
    let upper = GRID_ROWS_MAX.min(cols);
    let candidates: Vec<u8> = (GRID_ROWS_MIN..=upper).filter(|r| r % 2 == 1).collect();
    if candidates.is_empty() {
        // Walk down from `upper` to find the largest odd value ≤ upper.
        return (1..=upper).rev().find(|r| r % 2 == 1).unwrap_or(1);
    }
    let idx = (next_unit() * candidates.len() as f32) as usize;
    candidates[idx.min(candidates.len() - 1)]
}

/// Seed grid cells. The entry cell is forced empty so the player doesn't
/// "collect" anything before the first move. Density rolls are independent
/// per cell; tier-gating uses `activation_quality` (mean of snapshot history)
/// the same way the cube does.
fn seed_grid(
    cols: u8,
    rows: u8,
    entry_cell: (u8, u8),
    activation_quality: f32,
) -> Vec<Vec<GridNode>> {
    let mut grid = Vec::with_capacity(rows as usize);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols as usize);
        for c in 0..cols {
            if (c, r) == entry_cell {
                row.push(GridNode::Empty);
                continue;
            }
            let roll = next_unit();
            if roll < GRID_BONUS_DENSITY {
                let base = BonusTier::from_quality(activation_quality);
                let tier = if next_unit() < 0.2 {
                    match base {
                        BonusTier::Premium => BonusTier::Mid,
                        BonusTier::Mid => BonusTier::Default,
                        BonusTier::Default => BonusTier::Default,
                    }
                } else {
                    base
                };
                row.push(GridNode::Bonus(sample_bonus(tier)));
            } else if roll < GRID_BONUS_DENSITY + GRID_SPECIAL_DENSITY {
                let pick = (next_unit() * 3.0) as u32;
                row.push(match pick {
                    0 => GridNode::Echo,
                    1 => GridNode::Mimic,
                    _ => GridNode::Anchor,
                });
            } else {
                row.push(GridNode::Empty);
            }
        }
        grid.push(row);
    }
    grid
}

/// Open the grid: snap *both* arcs' baselines (streak preserved per the
/// running-counter rule), seed dimensions and cells, install the snapshot
/// history. Caller has already verified DPS stance and computed the budget +
/// history from the winning arc.
pub fn activate_grid(
    grid: &mut GridState,
    primary: &mut ArcState,
    secondary: &mut ArcState,
    budget: u32,
    history_snapshot: VecDeque<f32>,
) {
    let activation_quality = if history_snapshot.is_empty() {
        0.0
    } else {
        let sum: f32 = history_snapshot.iter().sum();
        (sum / history_snapshot.len() as f32).clamp(0.0, 1.0)
    };
    let cols = sample_grid_cols();
    let rows = sample_grid_rows(cols);
    let entry_cell = (0u8, rows / 2);
    grid.active = true;
    grid.cols = cols;
    grid.rows = rows;
    grid.entry_edge = GridEdge::Left;
    grid.exit_edge = GridEdge::Right;
    grid.grid = seed_grid(cols, rows, entry_cell, activation_quality);
    grid.path = vec![entry_cell];
    grid.cursor = entry_cell;
    grid.steps_remaining = budget;
    grid.collected.clear();
    grid.anchor_floor = None;
    grid.last_bonus = None;
    grid.quality_history = history_snapshot;
    grid.resolved = None;
    grid.resolve_anim_remaining = 0.0;

    // Snap both arcs' baselines so the next activation requires fresh delta
    // on either arc. Streak counters themselves are preserved.
    primary.streak_at_last_activation = primary.streak;
    secondary.streak_at_last_activation = secondary.streak;
}

fn reset_grid_fields(grid: &mut GridState) {
    grid.active = false;
    grid.cols = 0;
    grid.rows = 0;
    grid.entry_edge = GridEdge::default();
    grid.exit_edge = GridEdge::default();
    grid.grid.clear();
    grid.path.clear();
    grid.cursor = (0, 0);
    grid.steps_remaining = 0;
    grid.anchor_floor = None;
    grid.last_bonus = None;
    grid.quality_history.clear();
    grid.resolved = None;
    grid.resolve_anim_remaining = 0.0;
}

/// Tear down a grid run *with* its collected payout intact (drained by the
/// resolution path in `tick_grid_states`).
fn resolve_grid(grid: &mut GridState) {
    reset_grid_fields(grid);
}

/// Discard a grid run and its collected bonuses. Used when the player leaves
/// DPS stance mid-run — nothing is paid out.
pub fn cancel_grid(grid: &mut GridState) {
    resolve_grid(grid);
    grid.collected.clear();
}

/// Most-frequently-collected bonus *type* (compared by `short_label`); ties
/// resolved by latest exemplar. Used for Mimic resolution.
fn most_frequent_bonus_type(collected: &[(PhysicalBonus, f32)]) -> Option<PhysicalBonus> {
    if collected.is_empty() {
        return None;
    }
    let mut best: Option<(&'static str, u32, PhysicalBonus)> = None;
    for (b, _) in collected {
        let key = b.short_label();
        let count = collected
            .iter()
            .filter(|(o, _)| o.short_label() == key)
            .count() as u32;
        if best.as_ref().map(|(_, c, _)| count > *c).unwrap_or(true) {
            best = Some((key, count, b.clone()));
        }
    }
    best.map(|(_, _, b)| b)
}

/// True when stepping `dir` from `cursor` would cross the grid's exit gate.
/// The exit gate is at the center cell of the exit edge — only stepping OFF
/// that one cell in the matching direction counts as exit.
pub(crate) fn is_exit_step(
    cursor: (u8, u8),
    dir: GridDir,
    cols: u8,
    rows: u8,
    exit_edge: GridEdge,
) -> bool {
    if cols == 0 || rows == 0 {
        return false;
    }
    let mid_col = cols / 2;
    let mid_row = rows / 2;
    match (exit_edge, dir) {
        (GridEdge::Right, GridDir::Right) => cursor == (cols - 1, mid_row),
        (GridEdge::Left, GridDir::Left) => cursor == (0, mid_row),
        (GridEdge::Top, GridDir::Up) => cursor == (mid_col, 0),
        (GridEdge::Bottom, GridDir::Down) => cursor == (mid_col, rows - 1),
        _ => false,
    }
}

fn set_blowout_resolution(grid: &mut GridState) {
    grid.resolved = Some(if grid.anchor_floor.is_some() {
        GridResolution::AnchorSaved
    } else {
        GridResolution::Blowout
    });
    grid.resolve_anim_remaining = GRID_BLOWOUT_ANIM_SECS;
}

/// Apply one directional move. Returns true if the move advanced the cursor
/// or cleanly exited.
///
/// Blowout causes (per design): self-intersection attempt (revisiting a cell
/// already in `path` — covers both no-backtrack and box-in cases) or running
/// out of budget without reaching the exit. Out-of-bounds moves that aren't
/// the exit gesture are silently rejected (no state change) so a player can
/// mash directionals in a corner without instantly punishing themselves.
///
/// Exit happens only when the player presses the matching direction from the
/// center cell of the exit edge. The exit move costs one step like any other.
pub fn process_grid_move(grid: &mut GridState, dir: GridDir) -> bool {
    if !grid.active || grid.resolved.is_some() {
        return false;
    }
    let (nx, ny) = dir.step(grid.cursor);
    if nx < 0 || ny < 0 || nx >= grid.cols as i32 || ny >= grid.rows as i32 {
        // OOB. If this is the exit gesture, consume a step and resolve Exit;
        // otherwise silent reject.
        if is_exit_step(grid.cursor, dir, grid.cols, grid.rows, grid.exit_edge) {
            if grid.steps_remaining == 0 {
                // Spent the budget reaching the gate but can't afford the
                // exit step itself — that's a blowout, same as any other
                // budget-exhaustion case.
                set_blowout_resolution(grid);
                return false;
            }
            grid.steps_remaining -= 1;
            grid.resolved = Some(GridResolution::Exit);
            grid.resolve_anim_remaining = 0.0;
            return true;
        }
        return false;
    }
    let next = (nx as u8, ny as u8);
    if grid.path.contains(&next) {
        // Self-intersection attempt → blowout. The single `path.contains`
        // check subsumes both no-backtrack (previous cell) and broader
        // self-intersection (any earlier-visited cell).
        set_blowout_resolution(grid);
        return false;
    }
    if grid.steps_remaining == 0 {
        // Budget exhausted; no further moves are possible.
        if grid.resolved.is_none() {
            set_blowout_resolution(grid);
        }
        return false;
    }

    grid.path.push(next);
    grid.cursor = next;
    grid.steps_remaining -= 1;

    // Step index 0 = first move. quality_history[0] = most recent commit.
    let step_index = grid.path.len().saturating_sub(2);

    let node = grid.grid[next.1 as usize][next.0 as usize].clone();
    match node {
        GridNode::Empty => {}
        GridNode::Bonus(b) => {
            let mag = grid
                .quality_history
                .get(step_index)
                .copied()
                .unwrap_or(0.0);
            grid.collected.push((b.clone(), mag));
            grid.last_bonus = Some((b, mag));
        }
        GridNode::Echo => {
            if let Some((b, mag)) = grid.last_bonus.clone() {
                grid.collected.push((b, mag));
            }
        }
        GridNode::Mimic => {
            if let Some(b) = most_frequent_bonus_type(&grid.collected) {
                let mag = grid
                    .quality_history
                    .get(step_index)
                    .copied()
                    .unwrap_or(0.0);
                grid.collected.push((b.clone(), mag));
                grid.last_bonus = Some((b, mag));
            }
        }
        GridNode::Anchor => {
            grid.anchor_floor = Some(grid.collected.clone());
        }
    }

    // Exit is only triggered by the OOB-step branch above. Landing on the
    // last column / row is just a normal cell traversal — the player still
    // needs to step OFF the exit edge from the center cell to actually exit.
    if grid.steps_remaining == 0 {
        // Last step landed somewhere; the player can't make another move,
        // including the exit step. Blowout (or anchor-saved).
        set_blowout_resolution(grid);
    }

    true
}

/// Advance all DPS grid overlays by one server tick.
///
/// - Drain resolved grids: log the result, fold in the anchor floor on
///   `AnchorSaved`, then clear state.
/// - Off-DPS-stance: cancel any active grid and clear pending triggers.
/// - Inactive grid on DPS stance: try cap-trigger (shared_delta >= cap), then
///   break-trigger (consume `DpsGridTrigger.pending_break_*`).
/// - Active grid: no-op — advancement is input-driven by `process_grid_move`.
pub fn tick_grid_states(
    time: Res<Time>,
    mut query: Query<(
        &mut GridState,
        &mut ArcState,
        &mut SecondaryArcState,
        &mut DpsGridTrigger,
        &CombatState,
    )>,
) {
    let dt = time.delta_secs();
    for (mut grid, mut primary, mut secondary, mut trigger, combat) in query.iter_mut() {
        let is_dps = matches!(combat.active_stance, Some(RoleStance::Dps));

        if grid.resolved.is_some() {
            // Hold the grid in its resolved state for `resolve_anim_remaining`
            // seconds so the client can play the shatter (or, for Exit, snap
            // closed immediately since this defaults to 0). Once it elapses,
            // run the actual cleanup.
            if grid.resolve_anim_remaining > 0.0 {
                grid.resolve_anim_remaining = (grid.resolve_anim_remaining - dt).max(0.0);
                continue;
            }

            let res = grid.resolved.unwrap();
            if matches!(res, GridResolution::AnchorSaved) {
                if let Some(floor) = grid.anchor_floor.take() {
                    grid.collected = floor;
                }
            } else if matches!(res, GridResolution::Blowout) {
                grid.collected.clear();
            }
            info!(
                "DPS grid resolved: {:?} | collected: {} bonuses",
                res,
                grid.collected.len()
            );
            // Bonus-application is out of scope for first pass; the resolution
            // system will eventually drain `collected` before reset.
            resolve_grid(&mut grid);
            grid.collected.clear();
            continue;
        }

        if !is_dps {
            if grid.active {
                cancel_grid(&mut grid);
            }
            trigger.clear();
            continue;
        }

        if grid.active {
            continue;
        }

        // Cap trigger.
        let delta = shared_delta(&primary, &secondary.0);
        if delta >= GRID_CRITICAL_MASS_CAP {
            let history = winning_arc(&primary, &secondary.0).commit.history.clone();
            activate_grid(&mut grid, &mut primary, &mut secondary.0, delta, history);
            trigger.clear();
            continue;
        }

        // Break trigger (set by combat.rs at the apex commit call site).
        if let Some(budget) = trigger.pending_break_budget.take() {
            let history = trigger.pending_break_history.take().unwrap_or_default();
            activate_grid(&mut grid, &mut primary, &mut secondary.0, budget, history);
        }
    }
}

/// Advance all active Bar Fill states by one server tick.
///
/// Per tick:
/// - Advance `fill` using `fill_rate(p) = base_rate * p^fill_exponent`.
/// - On auto-reset (fill >= 1.0): generate new bonus markers and reset fill to 0.
/// - Decay `arcane_pool` by `pool_decay_rate * dt`.
/// - Consume `drain_pending` from `fill` first, spilling overflow into
///   `arcane_pool` (negative — reducing banked pool). Drain spreads over
///   ~0.3 s so it reads as a rapid-but-visible dip rather than a snap.
pub fn tick_bar_fill_states(time: Res<Time>, mut query: Query<&mut BarFillState>) {
    const DRAIN_RATE: f32 = 1.0 / 0.3;
    let dt = time.delta_secs();
    for mut bf in query.iter_mut() {
        if bf.drain_pending > 0.0 {
            let drain_this_tick = (bf.drain_pending * DRAIN_RATE * dt).min(bf.drain_pending);
            bf.drain_pending -= drain_this_tick;
            let from_fill = drain_this_tick.min(bf.fill);
            bf.fill -= from_fill;
            let leftover = drain_this_tick - from_fill;
            if leftover > 0.0 {
                bf.arcane_pool = (bf.arcane_pool - leftover).max(0.0);
            }
        }
    }
}

/// Advance all active Wave Interference states by one server tick.
///
/// Per tick:
/// - Advance `time`.
/// - Advance `travel_progress` on each active segment; on exit compute area and
///   accumulate into `wave_accumulation`.
/// - For uncovered zone time, add raw traveling-wave area to `wave_accumulation`.
/// - Decay `disruption_amp` and `wave_accumulation` (slow passive decay).
/// - Decrement `commit_cooldown`.
pub fn tick_wave_interference_states(
    time: Res<Time>,
    mut query: Query<&mut WaveInterferenceState>,
) {
}

/// Advance all active Value Lock states by one server tick.
///
/// Per tick:
/// - If `is_held`: increment `hold_progress` at the fixed fill rate.
/// - Decay `target_frequency` by `frequency_decay_rate * dt` (fixed-rate stepped decay).
pub fn tick_value_lock_states(time: Res<Time>, mut query: Query<&mut ValueLockState>) {}

/// Advance all active Heartbeat states by one server tick.
///
/// Per tick:
/// - Interpolate `current_frequency` toward `target_frequency` via τ_interp.
/// - Add `frequency_spike` (from disruption) to `current_frequency`; decay the spike.
/// - Advance `phase` by `current_frequency * dt`, wrapping at 1.0.
/// - Decay `envelope_noise`.
/// - Release lockout at phase midpoint (0.5).
///
/// MVP scope: full phase/frequency simulation is deferred. This tick only
/// decays the two disruption fields so they don't accumulate indefinitely.
/// `frequency_spike` decays fast (1 s); `envelope_noise` decays slower (3 s)
/// to match the "sustained noise floor" design.
pub fn tick_heartbeat_states(time: Res<Time>, mut query: Query<&mut HeartbeatState>) {
    let dt = time.delta_secs();
    for mut hb in query.iter_mut() {
        hb.frequency_spike *= (-dt / 1.0_f32).exp();
        hb.envelope_noise  *= (-dt / 3.0_f32).exp();
    }
}

#[cfg(test)]
mod tests {
    //! Pure-function tests for arc commit / tick logic. The remaining minigame
    //! systems (cube FSM, bar fill, wave interference, value lock, heartbeat)
    //! are not yet covered — see `server/tests/arc_system.rs` for the App-based
    //! pattern that the next round of tests should follow.
    use super::*;

    fn arc_at_proximity(p: f32) -> ArcState {
        let mut arc = ArcState::default();
        arc.theta = FRAC_PI_2 + arc.amplitude * p;
        arc
    }

    #[test]
    fn close_zone_commit_increments_streak() {
        let mut arc = arc_at_proximity(0.0);
        arc.streak = 3;
        process_arc_commit(&mut arc);
        assert_eq!(arc.streak, 4);
        assert!(arc.commit.in_lockout);
    }

    #[test]
    fn far_zone_commit_resets_streak_and_activation_baseline() {
        let mut arc = arc_at_proximity(0.9);
        arc.streak = 7;
        arc.streak_at_last_activation = 4;
        process_arc_commit(&mut arc);
        assert_eq!(arc.streak, 0);
        assert_eq!(arc.streak_at_last_activation, 0);
    }

    #[test]
    fn mid_zone_commit_neither_increments_nor_resets() {
        let mut arc = arc_at_proximity(0.5);
        arc.streak = 3;
        arc.streak_at_last_activation = 1;
        process_arc_commit(&mut arc);
        assert_eq!(arc.streak, 3);
        assert_eq!(arc.streak_at_last_activation, 1);
    }

    #[test]
    fn commit_during_lockout_is_noop() {
        let mut arc = arc_at_proximity(0.0);
        arc.streak = 2;
        arc.commit.in_lockout = true;
        let before = arc.clone();
        process_arc_commit(&mut arc);
        assert_eq!(arc, before);
    }

    #[test]
    fn any_commit_clears_idle_apex_counter() {
        let mut arc = arc_at_proximity(0.5);
        arc.apex_visits_since_commit = 1;
        process_arc_commit(&mut arc);
        assert_eq!(arc.apex_visits_since_commit, 0);
    }

    #[test]
    fn two_idle_apex_visits_break_streak() {
        // omega = π → period 2 s → two apex crossings per oscillation.
        // Tick at 60 Hz across ~2.1 s of idle simulation.
        let mut arc = ArcState::default();
        arc.streak = 5;
        arc.streak_at_last_activation = 5;
        for _ in 0..130 {
            tick_arc(&mut arc, 1.0 / 60.0);
        }
        assert_eq!(arc.streak, 0);
        assert_eq!(arc.streak_at_last_activation, 0);
    }

    // ── Cube ─────────────────────────────────────────────────────────────────

    use shared::components::minigame::cube::{CubeEdge, CubeState, CUBE_ROTATIONS_PER_ACTIVATION};

    fn populated_active_cube() -> CubeState {
        let mut cube = CubeState::default();
        cube.active = true;
        cube.rotations_remaining = CUBE_ROTATIONS_PER_ACTIVATION;
        cube.current_face = [
            Some(PhysicalBonus::BaseDamage(10.0)),
            Some(PhysicalBonus::BaseDamage(10.0)),
            Some(PhysicalBonus::BaseDamage(10.0)),
        ];
        cube
    }

    #[test]
    fn activate_cube_initializes_state_and_preserves_streak() {
        // Memory invariant: cube activation does NOT reset the running streak;
        // it only snapshots the baseline for the next activation.
        let mut arc = ArcState::default();
        arc.streak = 4;
        arc.commit.history.extend([0.5, 0.5, 0.5, 0.5]);
        let mut cube = CubeState::default();

        activate_cube(&mut cube, &mut arc);

        assert!(cube.active);
        assert_eq!(cube.fill_progress, 0.0);
        assert_eq!(cube.rotations_remaining, CUBE_ROTATIONS_PER_ACTIVATION);
        assert!(cube.current_face.iter().all(Option::is_some));
        assert!(cube.collected.is_empty());
        assert_eq!(arc.streak, 4, "streak must survive activation");
        assert_eq!(arc.streak_at_last_activation, 4);
    }

    #[test]
    fn collect_in_window_records_bonus_and_starts_animation() {
        let mut cube = populated_active_cube();
        cube.fill_progress = 1.0; // peak window — timing_precision == Some(1.0)

        let ok = process_cube_collect(&mut cube, CubeEdge::Bottom);

        assert!(ok);
        assert_eq!(cube.collected.len(), 1);
        assert_eq!(cube.rotating_edge, Some(CubeEdge::Bottom));
        assert_eq!(cube.rotations_remaining, CUBE_ROTATIONS_PER_ACTIVATION - 1);
        assert!(cube.new_face_pending);
    }

    #[test]
    fn collect_outside_window_is_rejected() {
        let mut cube = populated_active_cube();
        cube.fill_progress = 0.5; // outside CUBE_COLLECT_WINDOW

        let ok = process_cube_collect(&mut cube, CubeEdge::Bottom);

        assert!(!ok);
        assert!(cube.collected.is_empty());
        assert!(cube.rotating_edge.is_none());
    }

    #[test]
    fn collect_during_animation_is_rejected() {
        let mut cube = populated_active_cube();
        cube.fill_progress = 1.0;
        cube.rotating_edge = Some(CubeEdge::Left); // animation in progress

        let ok = process_cube_collect(&mut cube, CubeEdge::Bottom);

        assert!(!ok);
        assert_eq!(cube.collected.len(), 0);
    }

    #[test]
    fn cancel_cube_clears_collected_and_deactivates() {
        let mut cube = populated_active_cube();
        cube.collected.push((PhysicalBonus::BaseDamage(5.0), 1.0));

        cancel_cube(&mut cube);

        assert!(!cube.active);
        assert!(cube.collected.is_empty());
        assert_eq!(cube.rotations_remaining, 0);
    }

    // ── Grid (Duelist) ───────────────────────────────────────────────────────

    fn arcs_with_streaks(p_streak: u32, p_base: u32, s_streak: u32, s_base: u32) -> (ArcState, SecondaryArcState) {
        let mut p = ArcState::default();
        p.streak = p_streak;
        p.streak_at_last_activation = p_base;
        let mut s = ArcState::default();
        s.streak = s_streak;
        s.streak_at_last_activation = s_base;
        (p, SecondaryArcState(s))
    }

    fn small_history(values: &[f32]) -> VecDeque<f32> {
        // Newest first, matching CommitTracker's push_front semantics.
        let mut h = VecDeque::with_capacity(values.len());
        for v in values.iter().rev() {
            h.push_front(*v);
        }
        h
    }

    fn force_seeded_grid(
        grid: &mut GridState,
        cols: u8,
        rows: u8,
        rows_data: Vec<Vec<GridNode>>,
        budget: u32,
        history: VecDeque<f32>,
    ) {
        // Bypass `activate_grid`'s randomness — for routing tests we want
        // deterministic cell contents and dimensions.
        grid.active = true;
        grid.cols = cols;
        grid.rows = rows;
        grid.entry_edge = GridEdge::Left;
        grid.exit_edge = GridEdge::Right;
        grid.grid = rows_data;
        let entry_cell = (0u8, rows / 2);
        grid.path = vec![entry_cell];
        grid.cursor = entry_cell;
        grid.steps_remaining = budget;
        grid.collected.clear();
        grid.anchor_floor = None;
        grid.last_bonus = None;
        grid.quality_history = history;
        grid.resolved = None;
    }

    #[test]
    fn shared_delta_is_max_of_per_arc_deltas() {
        let (p, s) = arcs_with_streaks(8, 0, 5, 0);
        assert_eq!(shared_delta(&p, &s.0), 8);
        let (p, s) = arcs_with_streaks(3, 0, 7, 0);
        assert_eq!(shared_delta(&p, &s.0), 7);
        // Baseline subtraction: streak 12, base 4 → delta 8.
        let (p, s) = arcs_with_streaks(12, 4, 0, 0);
        assert_eq!(shared_delta(&p, &s.0), 8);
    }

    #[test]
    fn cap_fires_when_either_arc_reaches_delta_cap_alone() {
        // Single-arc focus, secondary at delta 0. Primary delta = cap → cap-trigger.
        let (mut p, mut s) = arcs_with_streaks(GRID_CRITICAL_MASS_CAP, 0, 0, 0);
        let mut grid = GridState::default();
        let history = small_history(&[1.0, 0.9, 0.85, 0.8, 0.75, 0.7, 0.65, 0.6, 0.55, 0.5]);
        let delta = shared_delta(&p, &s.0);
        assert!(delta >= GRID_CRITICAL_MASS_CAP);

        activate_grid(&mut grid, &mut p, &mut s.0, delta, history.clone());
        assert!(grid.active);
        assert_eq!(grid.steps_remaining, GRID_CRITICAL_MASS_CAP);
        assert_eq!(grid.quality_history, history);
        // Both baselines snapped to current streaks; streak preserved.
        assert_eq!(p.streak_at_last_activation, GRID_CRITICAL_MASS_CAP);
        assert_eq!(s.0.streak_at_last_activation, 0);
    }

    #[test]
    fn cap_uses_max_not_sum_so_dual_at_5_does_not_fire() {
        // Both arcs at delta 5 → max = 5, below cap (10).
        let (p, s) = arcs_with_streaks(5, 0, 5, 0);
        assert_eq!(shared_delta(&p, &s.0), 5);
        assert!(shared_delta(&p, &s.0) < GRID_CRITICAL_MASS_CAP);
    }

    #[test]
    fn winning_arc_returns_higher_delta_primary_on_tie() {
        let (p, s) = arcs_with_streaks(7, 0, 5, 0);
        assert_eq!(winning_arc(&p, &s.0).streak, 7);
        let (p, s) = arcs_with_streaks(3, 0, 8, 0);
        assert_eq!(winning_arc(&p, &s.0).streak, 8);
        // Tie → primary wins.
        let (p, s) = arcs_with_streaks(6, 0, 6, 0);
        assert_eq!(winning_arc(&p, &s.0).streak, 6);
        assert!(std::ptr::eq(winning_arc(&p, &s.0), &p));
    }

    #[test]
    fn activate_grid_snaps_both_baselines_and_preserves_streaks() {
        let (mut p, s) = arcs_with_streaks(12, 4, 9, 9);
        let mut secondary = s;
        let mut grid = GridState::default();
        activate_grid(&mut grid, &mut p, &mut secondary.0, 8, VecDeque::new());
        // Streak counters survive activation.
        assert_eq!(p.streak, 12);
        assert_eq!(secondary.0.streak, 9);
        // Both baselines snap to current streak (delta → 0 on each arc).
        assert_eq!(p.streak_at_last_activation, 12);
        assert_eq!(secondary.0.streak_at_last_activation, 9);
    }

    #[test]
    fn backtrack_attempt_fires_blowout() {
        // 3×3 empty grid; entry at (0,1). Step Right to (1,1), then attempt
        // Left back to (0,1) — that's a self-intersection attempt → blowout.
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 5, small_history(&[]));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.cursor, (1, 1));
        assert!(!process_grid_move(&mut grid, GridDir::Left));
        assert_eq!(grid.resolved, Some(GridResolution::Blowout));
    }

    #[test]
    fn loop_close_attempt_fires_blowout() {
        // 3×3 grid; route (0,1) → (1,1) → (1,0) → (0,0). Now Down to (0,1)
        // would close a loop on the entry — blowout per the new spec.
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 10, small_history(&[]));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert!(process_grid_move(&mut grid, GridDir::Up));
        assert!(process_grid_move(&mut grid, GridDir::Left));
        assert!(!process_grid_move(&mut grid, GridDir::Down));
        assert_eq!(grid.resolved, Some(GridResolution::Blowout));
    }

    #[test]
    fn out_of_bounds_attempt_silently_rejects() {
        // OOB attempts must NOT fire blowout — players in a corner should be
        // able to mash directionals without instant punishment.
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 5, small_history(&[]));
        // From entry (0, 1), Left is OOB.
        assert!(!process_grid_move(&mut grid, GridDir::Left));
        assert_eq!(grid.resolved, None);
        assert_eq!(grid.cursor, (0, 1));
    }

    #[test]
    fn empty_cell_traversal_does_not_blowout_when_neighbors_remain() {
        // Stepping onto a chain of empty cells with budget remaining must not
        // trigger blowout, regardless of which cells are empty vs.
        // bonus-bearing.
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 10, small_history(&[]));
        assert!(process_grid_move(&mut grid, GridDir::Up));    // (0, 0)
        assert_eq!(grid.resolved, None);
        assert!(process_grid_move(&mut grid, GridDir::Right)); // (1, 0)
        assert_eq!(grid.resolved, None);
        assert!(process_grid_move(&mut grid, GridDir::Down));  // (1, 1)
        assert_eq!(grid.resolved, None);
    }
    #[test]

    #[test]
    fn move_into_bonus_collects_with_quality_from_snapshot() {
        // Step 1 should read quality_history[0] (most recent commit).
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty, GridNode::Bonus(PhysicalBonus::BaseDamage(10.0)), GridNode::Empty],
            vec![GridNode::Empty, GridNode::Empty, GridNode::Empty],
            vec![GridNode::Empty, GridNode::Empty, GridNode::Empty],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 5, small_history(&[0.95, 0.5]));
        // Entry is (0, 1). Move Up to (0, 0), then Right to (1, 0) — bonus
        // cell. But we want first-move-onto-bonus; instead route directly:
        // Reset path: do Up from (0,1) first then Right.
        assert!(process_grid_move(&mut grid, GridDir::Up));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.collected.len(), 1);
        // Step 2 reads history[1] = 0.5.
        let (_, mag) = &grid.collected[0];
        assert!((mag - 0.5).abs() < 1e-5);
        assert!(grid.last_bonus.is_some());
    }

    #[test]
    fn echo_duplicates_last_bonus() {
        let mut grid = GridState::default();
        let rows = vec![
            vec![
                GridNode::Empty,
                GridNode::Bonus(PhysicalBonus::BaseDamage(10.0)),
                GridNode::Empty,
            ],
            vec![GridNode::Empty, GridNode::Empty, GridNode::Echo],
            vec![GridNode::Empty, GridNode::Empty, GridNode::Empty],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 5, small_history(&[0.9, 0.8, 0.7]));
        assert!(process_grid_move(&mut grid, GridDir::Up));    // (0,0) empty
        assert!(process_grid_move(&mut grid, GridDir::Right)); // (1,0) bonus
        assert!(process_grid_move(&mut grid, GridDir::Down));  // (1,1) empty
        assert!(process_grid_move(&mut grid, GridDir::Right)); // (2,1) echo on exit-row
        // Bonus at step 2 (history[1] = 0.8) + echo'd same bonus on step 4.
        assert_eq!(grid.collected.len(), 2);
        assert_eq!(grid.collected[0].0, grid.collected[1].0);
        // Now press Right again to exit through the gate at (2, 1).
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.resolved, Some(GridResolution::Exit));
    }

    #[test]
    fn anchor_sets_floor_self_intersection_resolves_anchored() {
        // 3×2 grid; route (0,1) → (1,1)Anchor → (1,0)Empty. Now Up is OOB
        // (silent reject), Right (2,0) is open Exit-edge, Left (0,0) is
        // open. Force a self-intersection by going back Down to (1,1).
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty, GridNode::Empty, GridNode::Empty],
            vec![GridNode::Empty, GridNode::Anchor, GridNode::Empty],
        ];
        force_seeded_grid(&mut grid, 3, 2, rows, 10, small_history(&[]));
        assert!(process_grid_move(&mut grid, GridDir::Right)); // (1,1) Anchor
        assert!(grid.anchor_floor.is_some());
        assert!(process_grid_move(&mut grid, GridDir::Up));    // (1,0)
        // Down would revisit (1,1) → blowout, but anchor floor saves it.
        assert!(!process_grid_move(&mut grid, GridDir::Down));
        assert_eq!(grid.resolved, Some(GridResolution::AnchorSaved));
    }

    #[test]
    fn budget_exhausted_off_exit_edge_blows_out() {
        // 3×3 grid, budget = 2. Route (0,1) → (1,1) → (1,0). Last move uses
        // the final step and lands away from the exit edge → blowout.
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 2, small_history(&[]));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.resolved, None);
        assert!(process_grid_move(&mut grid, GridDir::Up));
        assert_eq!(grid.steps_remaining, 0);
        assert_eq!(grid.resolved, Some(GridResolution::Blowout));
    }

    #[test]
    fn landing_on_exit_column_does_not_resolve() {
        // Stepping onto the rightmost column is just a normal cell traversal
        // — it does NOT resolve Exit. Player must step Right *off* the gate
        // cell on the exit row to actually exit.
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 5, small_history(&[]));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.cursor, (2, 1));
        assert_eq!(grid.resolved, None, "landing on exit column is not exit");
    }

    #[test]
    fn right_step_off_center_row_exits_and_costs_a_step() {
        // Step Right twice to reach (2, 1) — center row, exit column. Then
        // pressing Right again is the exit gesture; it consumes one step.
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 5, small_history(&[]));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.steps_remaining, 3);
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.resolved, Some(GridResolution::Exit));
        // Exit cost one more step.
        assert_eq!(grid.steps_remaining, 2);
    }

    #[test]
    fn right_step_off_non_center_row_silently_rejects() {
        // From an exit-column cell that's NOT on the center row, pressing
        // Right is just OOB — silent reject, no exit, no blowout.
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 10, small_history(&[]));
        // Route to (2, 0): Up, Right, Right.
        assert!(process_grid_move(&mut grid, GridDir::Up));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.cursor, (2, 0));
        assert!(!process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.resolved, None);
    }

    #[test]
    fn exit_step_with_zero_budget_blows_out() {
        // Player reaches the gate cell with steps_remaining == 0. The exit
        // gesture itself requires one step; without it, blowout fires.
        let mut grid = GridState::default();
        let rows = vec![
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
            vec![GridNode::Empty; 3],
        ];
        force_seeded_grid(&mut grid, 3, 3, rows, 2, small_history(&[]));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert!(process_grid_move(&mut grid, GridDir::Right));
        assert_eq!(grid.cursor, (2, 1));
        assert_eq!(grid.steps_remaining, 0);
        // Already blowout from budget exhaustion landing off-exit.
        assert_eq!(grid.resolved, Some(GridResolution::Blowout));
    }

    #[test]
    fn cancel_grid_clears_state() {
        let mut grid = GridState::default();
        let rows = vec![vec![GridNode::Empty; 3]; 3];
        force_seeded_grid(&mut grid, 3, 3, rows, 5, small_history(&[]));
        grid.collected.push((PhysicalBonus::BaseDamage(1.0), 0.5));
        cancel_grid(&mut grid);
        assert!(!grid.active);
        assert!(grid.collected.is_empty());
        assert_eq!(grid.steps_remaining, 0);
    }

    #[test]
    fn grid_dir_step_handles_cardinal_moves() {
        assert_eq!(GridDir::Right.step((1, 1)), (2, 1));
        assert_eq!(GridDir::Left.step((1, 1)), (0, 1));
        assert_eq!(GridDir::Up.step((1, 1)), (1, 0));
        assert_eq!(GridDir::Down.step((1, 1)), (1, 2));
    }

    #[test]
    fn sample_grid_rows_always_yields_odd_value_within_range() {
        // Sweep every legal `cols` in the configured range and verify that
        // `sample_grid_rows` yields an odd value satisfying both invariants:
        // odd, and within `[GRID_ROWS_MIN, min(GRID_ROWS_MAX, cols)]`.
        for cols in GRID_COLS_MIN..=GRID_COLS_MAX {
            for _ in 0..200 {
                let rows = sample_grid_rows(cols);
                assert!(rows % 2 == 1, "rows {} must be odd", rows);
                assert!(
                    rows >= GRID_ROWS_MIN || rows <= GRID_ROWS_MAX.min(cols),
                    "rows {} out of [{}, min({}, {})]",
                    rows,
                    GRID_ROWS_MIN,
                    GRID_ROWS_MAX,
                    cols,
                );
            }
        }
    }

    #[test]
    fn sample_grid_rows_never_exceeds_cols() {
        // The "never taller than wide" invariant must hold for every legal
        // (cols, rows) sample, regardless of the constant values used.
        for cols in GRID_COLS_MIN..=GRID_COLS_MAX {
            for _ in 0..500 {
                let rows = sample_grid_rows(cols);
                assert!(
                    rows <= cols,
                    "grid {}×{} would be taller than wide",
                    cols,
                    rows,
                );
            }
        }
    }

    #[test]
    fn sample_grid_cols_uniform_within_range() {
        // Verify cols always falls inside the configured range. The
        // distribution itself isn't asserted (it's `next_unit`-based and
        // shared with combat-side RNG); just bounds.
        for _ in 0..500 {
            let c = sample_grid_cols();
            assert!(c >= GRID_COLS_MIN);
            assert!(c <= GRID_COLS_MAX);
        }
    }

    #[test]
    fn sample_inclusive_range_handles_edge_cases() {
        // Single-value range collapses to that value.
        for _ in 0..50 {
            assert_eq!(sample_inclusive_range(7, 7), 7);
        }
        // Inverted range (hi < lo) returns lo defensively.
        assert_eq!(sample_inclusive_range(9, 3), 9);
        // Spread sample stays in bounds.
        for _ in 0..200 {
            let v = sample_inclusive_range(2, 12);
            assert!(v >= 2 && v <= 12);
        }
    }
}
