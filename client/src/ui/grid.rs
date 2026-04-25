use bevy::math::{Rot2, Vec2};
use bevy::prelude::*;
use bevy::ui::{UiTransform, Val2};

use shared::components::minigame::cube::PhysicalBonus;
use shared::components::minigame::grid::{
    GridEdge, GridNode, GridResolution, GridState, GRID_BLOWOUT_ANIM_SECS, GRID_COLS_MAX,
    GRID_ROWS_MAX,
};

use crate::world::players::OwnServerEntity;

use super::hud::minigame_anchor::MinigameRoot;
use super::theme::{APEX_RED, NADIR_GREEN};

/// Pixel dimensions of one grid cell. Sized so the max-grid (`GRID_COLS_MAX`
/// × `GRID_ROWS_MAX`) fits inside the 420×290 minigame panel with the
/// `GRID_GAP_PX` between cells.
const GRID_CELL_PX: f32 = 40.0;
const GRID_GAP_PX: f32 = 6.0;
/// Maximum dimensions we pre-spawn. The render system hides cells outside
/// the active `(cols, rows)` per activation. These mirror the shared cap
/// constants so increasing the activation upper bound automatically grows
/// the pre-spawn pool.
const GRID_MAX_COLS: u8 = GRID_COLS_MAX;
const GRID_MAX_ROWS: u8 = GRID_ROWS_MAX;
/// Header height (steps + collected counts).
const GRID_HEADER_H_PX: f32 = 22.0;
/// Number of pre-spawned path connectors. Sized to the max possible path
/// edges, which equals the max budget the player can be granted. Budget is
/// at most `GRID_CRITICAL_MASS_CAP`, plus headroom for the exit step itself.
const GRID_MAX_CONNECTORS: usize = 16;
/// Thickness of the path connector bar. The cell edges are 1 px borders, so a
/// 6 px line reads as a deliberate trail rather than a UI artifact.
const PATH_CONNECTOR_THICK_PX: f32 = 6.0;

/// Tag for the grid overlay's root container. Visibility is flipped by
/// `render_grid` based on the local player's `GridState.active`.
#[derive(Component)]
pub struct GridRoot;

/// Tag for the header text node ("Steps: N  Collected: M").
#[derive(Component)]
pub struct GridHeader;

/// One cell in the pre-spawned max-grid. The render system reads
/// `GridState.grid[row][col]` and `path` / `cursor` to set color and label.
#[derive(Component, Clone, Copy)]
pub struct GridCell {
    pub col: u8,
    pub row: u8,
}

/// Tag for the text node inside a `GridCell`.
#[derive(Component, Clone, Copy)]
pub struct GridCellText {
    pub col: u8,
    pub row: u8,
}

/// One pre-spawned path connector bar. `index` matches `path.windows(2)`
/// position; the render system positions/sizes it to bridge consecutive
/// cells, or hides it when the path is shorter than the index.
#[derive(Component, Clone, Copy)]
pub struct PathConnector {
    pub index: usize,
}

/// Marker on the entry edge indicating where the player enters from.
#[derive(Component)]
pub struct GridEntryIndicator;

/// Marker on the exit edge indicating the gate the player must step through
/// to exit. Positioned dynamically each frame on the center cell of the
/// active grid's exit edge.
#[derive(Component)]
pub struct GridExitIndicator;

// ── Spawn ────────────────────────────────────────────────────────────────────

/// Spawns the grid overlay nodes once. Cells use `Visibility::Inherited` so
/// they automatically follow the root's visibility — when the run resolves
/// and `render_grid` flips the root to Hidden, every cell goes hidden too.
pub fn spawn_grid_overlay(mut commands: Commands, root_q: Query<Entity, With<MinigameRoot>>) {
    let Ok(root) = root_q.single() else { return };

    let total_w = GRID_MAX_COLS as f32 * GRID_CELL_PX
        + (GRID_MAX_COLS as f32 - 1.0) * GRID_GAP_PX;
    let total_h = GRID_MAX_ROWS as f32 * GRID_CELL_PX
        + (GRID_MAX_ROWS as f32 - 1.0) * GRID_GAP_PX
        + GRID_HEADER_H_PX
        + 4.0;
    // Pad horizontally so entry/exit arrow indicators fit just outside the
    // grid cells without being clipped by the root width.
    let arrow_pad = 18.0;
    let root_w = total_w + arrow_pad * 2.0;

    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                GridRoot,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    bottom: Val::Px(0.0),
                    width: Val::Px(root_w),
                    height: Val::Px(total_h),
                    margin: UiRect::left(Val::Px(-root_w / 2.0)),
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|grid| {
                grid.spawn((
                    GridHeader,
                    Text::new(""),
                    TextFont { font_size: 12.0, ..default() },
                    TextColor(Color::srgba(0.85, 0.85, 0.95, 0.95)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(arrow_pad),
                        top: Val::Px(0.0),
                        width: Val::Px(total_w),
                        height: Val::Px(GRID_HEADER_H_PX),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ));

                // Entry / exit arrow indicators. Positioned dynamically each
                // frame in `render_grid` based on the active grid dimensions
                // and entry/exit edges.
                grid.spawn((
                    GridEntryIndicator,
                    Text::new("▶"),
                    TextFont { font_size: 18.0, ..default() },
                    TextColor(Color::srgba(0.55, 0.85, 0.55, 0.9)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        ..default()
                    },
                    Visibility::Inherited,
                ));
                grid.spawn((
                    GridExitIndicator,
                    Text::new("▶"),
                    TextFont { font_size: 18.0, ..default() },
                    TextColor(Color::srgba(1.0, 0.85, 0.35, 0.95)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        ..default()
                    },
                    Visibility::Inherited,
                ));

                // Pre-spawn connectors *before* cells so the cells render on
                // top — the trail sits behind the cell faces, only filling
                // the gap strips.
                for i in 0..GRID_MAX_CONNECTORS {
                    grid.spawn((
                        PathConnector { index: i },
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            width: Val::Px(0.0),
                            height: Val::Px(0.0),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        // Inherited so connectors hide when the root hides
                        // (no need to clear them on resolution).
                        Visibility::Inherited,
                    ));
                }

                for r in 0..GRID_MAX_ROWS {
                    for c in 0..GRID_MAX_COLS {
                        let left = arrow_pad + c as f32 * (GRID_CELL_PX + GRID_GAP_PX);
                        let top = GRID_HEADER_H_PX
                            + 4.0
                            + r as f32 * (GRID_CELL_PX + GRID_GAP_PX);
                        grid.spawn((
                            GridCell { col: c, row: r },
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(left),
                                top: Val::Px(top),
                                width: Val::Px(GRID_CELL_PX),
                                height: Val::Px(GRID_CELL_PX),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                            BorderColor {
                                top: Color::NONE,
                                bottom: Color::NONE,
                                left: Color::NONE,
                                right: Color::NONE,
                            },
                            // Inherited (not Visible) so the root's Hidden
                            // state propagates and clears the grid display
                            // when the run resolves.
                            Visibility::Inherited,
                            UiTransform {
                                translation: Val2::ZERO,
                                scale: Vec2::ONE,
                                rotation: Rot2::IDENTITY,
                            },
                        ))
                        .with_children(|cell| {
                            cell.spawn((
                                GridCellText { col: c, row: r },
                                Text::new(""),
                                TextFont { font_size: 10.0, ..default() },
                                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.95)),
                            ));
                        });
                    }
                }
            });
    });
}

// ── Render ───────────────────────────────────────────────────────────────────

/// Drive the grid overlay each frame from the local player's `GridState`. When
/// inactive: hides the root (which propagates to all children via Inherited).
/// When active: shows in-bounds cells, sets per-cell background by state
/// (cursor / path / bonus / Echo / Mimic / Anchor / empty), positions the
/// path connectors between consecutive visited cells, updates the header.
/// When a blowout/anchor-saved resolution is mid-animation
/// (`resolve_anim_remaining > 0`), the cells fly outward and shrink for the
/// duration before the server clears state.
pub fn render_grid(
    own_entity: Option<Res<OwnServerEntity>>,
    server_q: Query<&GridState>,
    mut root_q: Query<&mut Visibility, With<GridRoot>>,
    mut header_q: Query<
        (&mut Text, &mut TextColor),
        (
            With<GridHeader>,
            Without<GridCellText>,
            Without<GridEntryIndicator>,
            Without<GridExitIndicator>,
        ),
    >,
    mut cell_q: Query<
        (
            &GridCell,
            &mut Visibility,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut UiTransform,
        ),
        (
            Without<GridRoot>,
            Without<GridHeader>,
            Without<GridCellText>,
            Without<PathConnector>,
            Without<GridEntryIndicator>,
            Without<GridExitIndicator>,
        ),
    >,
    mut cell_text_q: Query<(&GridCellText, &mut Text), Without<GridHeader>>,
    mut connector_q: Query<
        (&PathConnector, &mut Node, &mut BackgroundColor),
        (
            Without<GridCell>,
            Without<GridRoot>,
            Without<GridHeader>,
            Without<GridEntryIndicator>,
            Without<GridExitIndicator>,
        ),
    >,
    mut entry_q: Query<
        (&mut Node, &mut Visibility),
        (
            With<GridEntryIndicator>,
            Without<GridExitIndicator>,
            Without<GridRoot>,
            Without<GridHeader>,
            Without<GridCell>,
            Without<PathConnector>,
        ),
    >,
    mut exit_q: Query<
        (&mut Node, &mut Visibility),
        (
            With<GridExitIndicator>,
            Without<GridEntryIndicator>,
            Without<GridRoot>,
            Without<GridHeader>,
            Without<GridCell>,
            Without<PathConnector>,
        ),
    >,
) {
    let grid = own_entity.as_ref().and_then(|own| server_q.get(own.0).ok());
    let Ok(mut root_vis) = root_q.single_mut() else { return };

    let Some(grid) = grid else {
        *root_vis = Visibility::Hidden;
        return;
    };
    if !grid.active {
        *root_vis = Visibility::Hidden;
        return;
    }
    *root_vis = Visibility::Visible;

    // Animation progress for blowout/anchor-saved shatter, in [0, 1].
    let shatter_progress = match grid.resolved {
        Some(GridResolution::Blowout) | Some(GridResolution::AnchorSaved)
            if GRID_BLOWOUT_ANIM_SECS > 0.0 =>
        {
            (1.0 - grid.resolve_anim_remaining / GRID_BLOWOUT_ANIM_SECS).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };
    let is_shattering = shatter_progress > 0.0;

    if let Ok((mut header, mut color)) = header_q.single_mut() {
        let new = format!(
            "Steps: {}   Collected: {}",
            grid.steps_remaining,
            grid.collected.len()
        );
        if header.0.as_str() != new {
            header.0 = new;
        }
        // Fade the header during shatter so the eye is drawn to the cells.
        let alpha = if is_shattering {
            (1.0 - shatter_progress).max(0.0) * 0.95
        } else {
            0.95
        };
        *color = TextColor(Color::srgba(0.85, 0.85, 0.95, alpha));
    }

    for (tag, mut vis, mut bg, mut border, mut xform) in cell_q.iter_mut() {
        if tag.col >= grid.cols || tag.row >= grid.rows {
            *vis = Visibility::Hidden;
            xform.translation = Val2::ZERO;
            xform.scale = Vec2::ONE;
            xform.rotation = Rot2::IDENTITY;
            continue;
        }
        // Inherited: follow the root's visibility, so when the run resolves
        // the cell hides automatically with the root.
        *vis = Visibility::Inherited;

        let cell_pos = (tag.col, tag.row);
        let is_cursor = grid.cursor == cell_pos;
        let in_path = grid.path.contains(&cell_pos);
        let node = &grid.grid[tag.row as usize][tag.col as usize];

        let (fill, edge) = cell_palette(node, is_cursor, in_path);
        *bg = BackgroundColor(fill);
        *border = BorderColor {
            top: edge,
            bottom: edge,
            left: edge,
            right: edge,
        };

        if is_shattering {
            let (dx, dy, rot, scale) = shatter_offset(
                tag.col,
                tag.row,
                grid.cols,
                grid.rows,
                shatter_progress,
            );
            xform.translation = Val2::new(Val::Px(dx), Val::Px(dy));
            xform.rotation = Rot2::radians(rot);
            xform.scale = Vec2::splat(scale);
        } else {
            xform.translation = Val2::ZERO;
            xform.rotation = Rot2::IDENTITY;
            xform.scale = Vec2::ONE;
        }
    }

    for (tag, mut text) in cell_text_q.iter_mut() {
        let label = if tag.col >= grid.cols || tag.row >= grid.rows {
            ""
        } else {
            cell_label(&grid.grid[tag.row as usize][tag.col as usize])
        };
        if text.0.as_str() != label {
            text.0 = label.to_string();
        }
    }

    if is_shattering {
        // Connectors are part of the live path; hide them while the cells
        // are flying apart so the trail doesn't render as floating bars.
        for (_, mut node, mut bg) in connector_q.iter_mut() {
            node.width = Val::Px(0.0);
            node.height = Val::Px(0.0);
            *bg = BackgroundColor(Color::NONE);
        }
    } else {
        update_path_connectors(grid, &mut connector_q);
    }

    update_indicators(grid, &mut entry_q, &mut exit_q, is_shattering);
}

/// Position the path-trace connectors. Each `path[i] → path[i+1]` segment
/// gets one rectangular bar bridging the gap between the two cells. Unused
/// connectors are zero-sized and transparent.
fn update_path_connectors(
    grid: &GridState,
    connector_q: &mut Query<
        (&PathConnector, &mut Node, &mut BackgroundColor),
        (
            Without<GridCell>,
            Without<GridRoot>,
            Without<GridHeader>,
            Without<GridEntryIndicator>,
            Without<GridExitIndicator>,
        ),
    >,
) {
    let edges: Vec<((u8, u8), (u8, u8))> = grid
        .path
        .windows(2)
        .map(|w| (w[0], w[1]))
        .collect();

    for (tag, mut node, mut bg) in connector_q.iter_mut() {
        if let Some(&(a, b)) = edges.get(tag.index) {
            let (left, top, w, h) = connector_geometry(a, b);
            node.left = Val::Px(left);
            node.top = Val::Px(top);
            node.width = Val::Px(w);
            node.height = Val::Px(h);
            // Trail color matches cursor highlight family but dimmer — reads
            // as "where I've been" without overpowering the bonus tints.
            *bg = BackgroundColor(Color::srgba(0.95, 0.85, 0.30, 0.90));
        } else {
            node.width = Val::Px(0.0);
            node.height = Val::Px(0.0);
            *bg = BackgroundColor(Color::NONE);
        }
    }
}

fn update_indicators(
    grid: &GridState,
    entry_q: &mut Query<
        (&mut Node, &mut Visibility),
        (
            With<GridEntryIndicator>,
            Without<GridExitIndicator>,
            Without<GridRoot>,
            Without<GridHeader>,
            Without<GridCell>,
            Without<PathConnector>,
        ),
    >,
    exit_q: &mut Query<
        (&mut Node, &mut Visibility),
        (
            With<GridExitIndicator>,
            Without<GridEntryIndicator>,
            Without<GridRoot>,
            Without<GridHeader>,
            Without<GridCell>,
            Without<PathConnector>,
        ),
    >,
    is_shattering: bool,
) {
    if is_shattering {
        if let Ok((_, mut vis)) = entry_q.single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Ok((_, mut vis)) = exit_q.single_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    }
    if let Ok((mut node, mut vis)) = entry_q.single_mut() {
        let (left, top) = indicator_position(grid, grid.entry_edge);
        node.left = Val::Px(left);
        node.top = Val::Px(top);
        *vis = Visibility::Inherited;
    }
    if let Ok((mut node, mut vis)) = exit_q.single_mut() {
        let (left, top) = indicator_position(grid, grid.exit_edge);
        node.left = Val::Px(left);
        node.top = Val::Px(top);
        *vis = Visibility::Inherited;
    }
}

/// Return `(left_px, top_px)` for an entry/exit arrow positioned just outside
/// the active grid on the center cell of `edge`. Both arrows always point
/// rightward (▶) — the entry one as "you came in here" and the exit as
/// "step this way out."
fn indicator_position(grid: &GridState, edge: GridEdge) -> (f32, f32) {
    let arrow_pad = 18.0;
    let cells_h_origin = arrow_pad;
    let cells_v_origin = GRID_HEADER_H_PX + 4.0;
    let cell_pitch_x = GRID_CELL_PX + GRID_GAP_PX;
    let cell_pitch_y = GRID_CELL_PX + GRID_GAP_PX;
    let mid_col = grid.cols / 2;
    let mid_row = grid.rows / 2;

    // Approximate visual centering for the 18 px arrow glyph.
    let glyph_w_off = 6.0;
    let glyph_v_off = 12.0;

    match edge {
        GridEdge::Left => {
            let cell_top = cells_v_origin + mid_row as f32 * cell_pitch_y;
            let top = cell_top + GRID_CELL_PX * 0.5 - glyph_v_off;
            let left = cells_h_origin - 14.0;
            (left, top)
        }
        GridEdge::Right => {
            let cell_top = cells_v_origin + mid_row as f32 * cell_pitch_y;
            let top = cell_top + GRID_CELL_PX * 0.5 - glyph_v_off;
            let cells_right_edge = cells_h_origin
                + grid.cols as f32 * GRID_CELL_PX
                + (grid.cols.saturating_sub(1)) as f32 * GRID_GAP_PX;
            let left = cells_right_edge + 4.0;
            (left, top)
        }
        GridEdge::Top => {
            let cell_left = cells_h_origin + mid_col as f32 * cell_pitch_x;
            let left = cell_left + GRID_CELL_PX * 0.5 - glyph_w_off;
            let top = cells_v_origin - 16.0;
            (left, top)
        }
        GridEdge::Bottom => {
            let cell_left = cells_h_origin + mid_col as f32 * cell_pitch_x;
            let left = cell_left + GRID_CELL_PX * 0.5 - glyph_w_off;
            let cells_bottom_edge = cells_v_origin
                + grid.rows as f32 * GRID_CELL_PX
                + (grid.rows.saturating_sub(1)) as f32 * GRID_GAP_PX;
            let top = cells_bottom_edge + 2.0;
            (left, top)
        }
    }
}

fn cell_origin(col: u8, row: u8) -> (f32, f32) {
    let arrow_pad = 18.0;
    let left = arrow_pad + col as f32 * (GRID_CELL_PX + GRID_GAP_PX);
    let top = GRID_HEADER_H_PX + 4.0 + row as f32 * (GRID_CELL_PX + GRID_GAP_PX);
    (left, top)
}

/// Return `(left, top, width, height)` for the connector bar bridging cells
/// `a` and `b`. Assumes the cells are 4-cardinal-adjacent.
fn connector_geometry(a: (u8, u8), b: (u8, u8)) -> (f32, f32, f32, f32) {
    let (a_left, a_top) = cell_origin(a.0, a.1);
    let (b_left, b_top) = cell_origin(b.0, b.1);
    let half_thick = PATH_CONNECTOR_THICK_PX * 0.5;

    if a.1 == b.1 {
        // Horizontal connector: spans the cell-gap strip in the row.
        let x_lo = a_left.min(b_left) + GRID_CELL_PX;
        let y_center = a_top + GRID_CELL_PX * 0.5;
        (x_lo, y_center - half_thick, GRID_GAP_PX, PATH_CONNECTOR_THICK_PX)
    } else {
        // Vertical connector.
        let y_lo = a_top.min(b_top) + GRID_CELL_PX;
        let x_center = a_left + GRID_CELL_PX * 0.5;
        (x_center - half_thick, y_lo, PATH_CONNECTOR_THICK_PX, GRID_GAP_PX)
    }
}

/// Per-cell shatter offset at progress `t ∈ [0, 1]`. Cells fly outward from
/// the grid center, with deterministic per-cell jitter so the cloud doesn't
/// move as a single rigid block. Gravity pulls them downward; rotation
/// accumulates linearly; scale shrinks to ~0 by `t = 1`.
fn shatter_offset(col: u8, row: u8, cols: u8, rows: u8, t: f32) -> (f32, f32, f32, f32) {
    let cx = (cols as f32 - 1.0) * 0.5;
    let cy = (rows as f32 - 1.0) * 0.5;
    let rel_x = col as f32 - cx;
    let rel_y = row as f32 - cy;
    let dist = (rel_x * rel_x + rel_y * rel_y).sqrt().max(0.001);

    let radial_speed = 220.0;
    let mut vx = rel_x / dist * radial_speed;
    let mut vy = rel_y / dist * radial_speed - 80.0; // slight upward bias

    // Per-cell jitter so the cloud doesn't move uniformly.
    let seed = (col as i32 * 37 + row as i32 * 71) as f32;
    vx += (seed * 0.7).sin() * 80.0;
    vy += (seed * 0.9).cos() * 50.0;

    let gravity = 480.0;
    let dx = vx * t;
    let dy = vy * t + 0.5 * gravity * t * t;
    let rot = (seed * 0.13).sin() * 4.5 * t;
    let scale = (1.0 - t * 0.9).max(0.0);

    (dx, dy, rot, scale)
}

fn cell_palette(node: &GridNode, is_cursor: bool, in_path: bool) -> (Color, Color) {
    if is_cursor {
        return (Color::srgba(0.95, 0.85, 0.30, 0.85), NADIR_GREEN);
    }
    if in_path {
        return (
            Color::srgba(0.20, 0.32, 0.20, 0.65),
            Color::srgba(0.55, 0.75, 0.55, 0.6),
        );
    }
    match node {
        GridNode::Empty => (
            Color::srgba(0.10, 0.10, 0.16, 0.55),
            Color::srgba(0.35, 0.35, 0.50, 0.5),
        ),
        GridNode::Bonus(b) => bonus_palette(b),
        GridNode::Echo => (
            Color::srgba(0.15, 0.20, 0.40, 0.75),
            Color::srgba(0.60, 0.70, 1.00, 0.8),
        ),
        GridNode::Mimic => (
            Color::srgba(0.30, 0.15, 0.40, 0.75),
            Color::srgba(0.85, 0.55, 1.00, 0.8),
        ),
        GridNode::Anchor => (
            Color::srgba(0.40, 0.32, 0.10, 0.75),
            Color::srgba(1.00, 0.85, 0.40, 0.85),
        ),
    }
}

fn bonus_palette(b: &PhysicalBonus) -> (Color, Color) {
    match b {
        PhysicalBonus::BaseDamage(_) => (
            Color::srgba(0.40, 0.18, 0.18, 0.80),
            APEX_RED,
        ),
        PhysicalBonus::DamageOverTime { .. } => (
            Color::srgba(0.35, 0.22, 0.10, 0.80),
            Color::srgba(0.95, 0.55, 0.20, 0.85),
        ),
        PhysicalBonus::StunOnHit { .. } => (
            Color::srgba(0.25, 0.25, 0.40, 0.80),
            Color::srgba(0.80, 0.80, 1.00, 0.85),
        ),
        PhysicalBonus::CooldownReduction(_) => (
            Color::srgba(0.18, 0.30, 0.30, 0.80),
            Color::srgba(0.55, 0.85, 0.85, 0.85),
        ),
        PhysicalBonus::Healing(_) => (
            Color::srgba(0.18, 0.36, 0.20, 0.80),
            NADIR_GREEN,
        ),
        PhysicalBonus::AggroBonus(_) => (
            Color::srgba(0.36, 0.30, 0.18, 0.80),
            Color::srgba(0.95, 0.80, 0.50, 0.85),
        ),
    }
}

fn cell_label(node: &GridNode) -> &'static str {
    match node {
        GridNode::Empty => "",
        GridNode::Bonus(b) => b.short_label(),
        GridNode::Echo => "ECHO",
        GridNode::Mimic => "MIMIC",
        GridNode::Anchor => "ANCH",
    }
}
