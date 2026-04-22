use bevy::math::{Rot2, Vec2};
use bevy::prelude::*;
use bevy::ui::{
    BackgroundGradient, BorderRadius, ColorStop, Gradient, LinearGradient, UiTransform, Val2,
};

use shared::components::minigame::cube::{
    CubeEdge, CubeState, PhysicalBonus, CUBE_COLLECT_WINDOW, CUBE_ROTATIONS_PER_ACTIVATION,
};

use super::theme::{APEX_RED, NADIR_GREEN};

/// How big the landed marker gets at peak pop.
const POP_PEAK_SCALE: f32 = 1.45;
/// Multiplier applied to the popped marker's palette intensity at peak pop.
const POP_PEAK_BRIGHTNESS: f32 = 1.6;

use crate::plugin::LocalClientId;
use crate::world::players::OwnServerEntity;

use super::hud::minigame_anchor::MinigameRoot;

// ── Layout constants ──────────────────────────────────────────────────────────

/// Edge length of the square cube overlay, px. Chosen to match a 50%-wide arc
/// node in the 420×290 minigame panel so the arc sits centered inside the cube.
/// A true square matters because the user reads it as a 3-D cube when it rotates.
const CUBE_SIZE_PX: f32 = 210.0;
/// Horizontal inset of the cube in the minigame panel (center-aligns the cube).
const CUBE_LEFT_PX: f32 = 105.0;

/// Thickness of the invisible cube's edge-fill line, in px.
const EDGE_THICK_PX: f32 = 3.0;
/// Bonus marker width (horizontal pill), px.
const MARKER_W_PX: f32 = 56.0;
/// Bonus marker height, px.
const MARKER_H_PX: f32 = 24.0;

/// Cube edge color — green at full rotations, red when depleted. Replaces the
/// tier-based palette so the cube visually communicates rotations remaining
/// (and the "Rotations: N" text that used to live on the cube).
fn rotations_gradient(remaining: u32) -> Color {
    let total = CUBE_ROTATIONS_PER_ACTIVATION.max(1);
    let consumed = (total.saturating_sub(remaining) as f32 / total as f32).clamp(0.0, 1.0);
    mix_colors(NADIR_GREEN, APEX_RED, consumed)
}

/// Shared dim/bright/hot palette derived from the edge colour. Used by both
/// the wireframe and the corner fills so the fill phase and rotation phase
/// read as the same lit object.
struct EdgePalette {
    dim: Color,
    bright: Color,
    hot: Color,
}

impl EdgePalette {
    fn from_base(base: Color, intensity: f32) -> Self {
        let hot = mix_colors(base, Color::WHITE, 0.55);
        Self {
            dim: scale_color(base, 0.35 * intensity),
            bright: scale_color(base, 1.10 * intensity),
            hot: scale_color(hot, intensity),
        }
    }
}

/// Neutral marker background — stays tier-agnostic so the edge-color gradient
/// reads as the dominant state signal. Brightens when the edge is "lit"
/// (inside the collect window or during the hit-pop).
fn marker_fill(lit: bool) -> Color {
    if lit {
        Color::srgba(0.28, 0.28, 0.36, 1.0)
    } else {
        Color::srgba(0.12, 0.12, 0.18, 0.85)
    }
}

// ── Node markers ──────────────────────────────────────────────────────────────

/// Root node of the cube overlay, sibling of the arc inside `MinigameRoot`.
#[derive(Component)]
pub struct CubeRoot;

/// A corner-fill segment that grows from a corner toward the edge-center marker.
/// Anchor-side is baked into the node at spawn; render-time only updates size.
/// `from_start` = true means the fill is anchored at the "start" corner of its
/// edge (top for L/R, left for Bottom) — this drives the gradient direction so
/// the dim end always sits at the corner and the hot end at the marker.
#[derive(Component, Clone, Copy)]
pub struct CubeFill {
    pub edge: CubeEdge,
    pub from_start: bool,
}

/// Container wrapping a single bonus marker: the coloured background plus a
/// child text label. Positioned at the center of its edge.
#[derive(Component, Clone, Copy)]
pub struct CubeMarker(pub CubeEdge);

/// Text label inside a marker. Updated with the bonus's short label on face swap.
#[derive(Component, Clone, Copy)]
pub struct CubeMarkerText(pub CubeEdge);

/// One of the 12 wireframe edges of the 3-D cube. Drawn only during a rotation
/// animation — the 8 cube vertices are rotated + projected each frame and each
/// edge is a thin UI rectangle positioned/rotated to span its two vertices.
#[derive(Component, Clone, Copy)]
pub struct CubeWireEdge(pub usize);

// ── Wireframe geometry ────────────────────────────────────────────────────────

/// 8 corners of a unit cube centered at origin. Axes are model-space:
/// +x right, +y up, +z toward viewer. Projected orthographically (drop z).
const CUBE_VERTS: [Vec3; 8] = [
    Vec3::new( 1.0,  1.0,  1.0), // 0 top-right-front
    Vec3::new(-1.0,  1.0,  1.0), // 1 top-left-front
    Vec3::new( 1.0, -1.0,  1.0), // 2 bottom-right-front
    Vec3::new(-1.0, -1.0,  1.0), // 3 bottom-left-front
    Vec3::new( 1.0,  1.0, -1.0), // 4 top-right-back
    Vec3::new(-1.0,  1.0, -1.0), // 5 top-left-back
    Vec3::new( 1.0, -1.0, -1.0), // 6 bottom-right-back
    Vec3::new(-1.0, -1.0, -1.0), // 7 bottom-left-back
];

/// 12 edges of the cube as pairs of indices into `CUBE_VERTS`.
const CUBE_EDGES: [(usize, usize); 12] = [
    // Front face
    (0, 1), (1, 3), (3, 2), (2, 0),
    // Back face
    (4, 5), (5, 7), (7, 6), (6, 4),
    // Depth (front-to-back)
    (0, 4), (1, 5), (2, 6), (3, 7),
];

/// Thickness of each wireframe edge in px. A little heavier than the fill
/// bars so the rounded caps and gradient read clearly.
const WIRE_THICK_PX: f32 = 3.5;
/// Corner radius on wireframe segments; at half-thickness the ends become
/// capsule caps which soften the look considerably.
const WIRE_RADIUS_PX: f32 = WIRE_THICK_PX * 0.5;

// ── Spawn ─────────────────────────────────────────────────────────────────────

/// Spawns the cube overlay nodes once, hidden by default. `render_cube` flips
/// visibility and drives per-frame layout from `CubeState`.
pub fn spawn_cube_overlay(mut commands: Commands, root_q: Query<Entity, With<MinigameRoot>>) {
    let Ok(root) = root_q.single() else { return };

    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                CubeRoot,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(CUBE_LEFT_PX),
                    bottom: Val::Px(0.0),
                    width: Val::Px(CUBE_SIZE_PX),
                    height: Val::Px(CUBE_SIZE_PX),
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|cube| {
                for edge in CubeEdge::ALL {
                    spawn_edge(cube, edge);
                }

                // 12 wireframe edges — invisible at rest, laid out each frame
                // during rotation via `render_cube`. Rounded caps + gradient give
                // the edges a soft, lit appearance instead of hard rectangles.
                for i in 0..CUBE_EDGES.len() {
                    cube.spawn((
                        CubeWireEdge(i),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            width: Val::Px(0.0),
                            height: Val::Px(WIRE_THICK_PX),
                            border_radius: BorderRadius::all(Val::Px(WIRE_RADIUS_PX)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        BackgroundGradient::default(),
                        UiTransform {
                            translation: Val2::ZERO,
                            scale: Vec2::ONE,
                            rotation: Rot2::IDENTITY,
                        },
                    ));
                }
            });
    });
}

fn spawn_edge(cube: &mut ChildSpawnerCommands, edge: CubeEdge) {
    // Each edge has two corner fills (grown from each corner toward the center)
    // plus a marker at the edge center.
    for from_start in [true, false] {
        cube.spawn((
            CubeFill { edge, from_start },
            Node {
                position_type: PositionType::Absolute,
                border_radius: BorderRadius::all(Val::Px(EDGE_THICK_PX * 0.5)),
                ..edge_fill_base_node(edge, from_start)
            },
            BackgroundColor(Color::NONE),
            BackgroundGradient::default(),
        ));
    }

    // Marker container: positioned on the edge, centered along it.
    cube.spawn((
        CubeMarker(edge),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(MARKER_W_PX),
            height: Val::Px(MARKER_H_PX),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.5)),
            ..marker_offset_node(edge)
        },
        BackgroundColor(Color::NONE),
        BorderColor {
            top: Color::NONE,
            bottom: Color::NONE,
            left: Color::NONE,
            right: Color::NONE,
        },
        // Driven during the pop phase — identity at rest.
        UiTransform {
            translation: Val2::ZERO,
            scale: Vec2::ONE,
            rotation: Rot2::IDENTITY,
        },
    ))
    .with_children(|m| {
        m.spawn((
            CubeMarkerText(edge),
            Text::new(""),
            TextFont {
                font_size: 10.0,
                ..default()
            },
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.95)),
        ));
    });
}

/// Per-edge base fields that position a fill near the correct corner. The
/// dimension that grows with `fill_progress` is filled in each frame by
/// `render_cube`; the other axis is fixed here.
fn edge_fill_base_node(edge: CubeEdge, from_start: bool) -> Node {
    match edge {
        CubeEdge::Left => Node {
            left: Val::Px(0.0),
            top: if from_start { Val::Px(0.0) } else { Val::Auto },
            bottom: if from_start { Val::Auto } else { Val::Px(0.0) },
            width: Val::Px(EDGE_THICK_PX),
            ..default()
        },
        CubeEdge::Right => Node {
            right: Val::Px(0.0),
            top: if from_start { Val::Px(0.0) } else { Val::Auto },
            bottom: if from_start { Val::Auto } else { Val::Px(0.0) },
            width: Val::Px(EDGE_THICK_PX),
            ..default()
        },
        CubeEdge::Bottom => Node {
            bottom: Val::Px(0.0),
            left: if from_start { Val::Px(0.0) } else { Val::Auto },
            right: if from_start { Val::Auto } else { Val::Px(0.0) },
            height: Val::Px(EDGE_THICK_PX),
            ..default()
        },
    }
}

/// Positions a marker container so its center sits on the midpoint of `edge`,
/// half inside and half outside the cube (the "invisible cube" straddle).
fn marker_offset_node(edge: CubeEdge) -> Node {
    let half_w = MARKER_W_PX * 0.5;
    let half_h = MARKER_H_PX * 0.5;
    match edge {
        CubeEdge::Left => Node {
            left: Val::Px(-half_w),
            top: Val::Percent(50.0),
            margin: UiRect {
                top: Val::Px(-half_h),
                ..default()
            },
            ..default()
        },
        CubeEdge::Right => Node {
            right: Val::Px(-half_w),
            top: Val::Percent(50.0),
            margin: UiRect {
                top: Val::Px(-half_h),
                ..default()
            },
            ..default()
        },
        CubeEdge::Bottom => Node {
            bottom: Val::Px(-half_h),
            left: Val::Percent(50.0),
            margin: UiRect {
                left: Val::Px(-half_w),
                ..default()
            },
            ..default()
        },
    }
}

// ── Render ────────────────────────────────────────────────────────────────────

/// Drives the cube overlay each frame from the local player's `CubeState`.
///
/// When `cube.active == false`: hides all overlay nodes.
/// When active: reveals the current face's markers, animates the corner fills
/// from their corners toward the edge-center marker, and lights the markers
/// while the collect window is open.
pub fn render_cube(
    local_id: Res<LocalClientId>,
    own_entity: Option<Res<OwnServerEntity>>,
    server_q: Query<&CubeState>,
    node_size_q: Query<&ComputedNode, With<CubeRoot>>,
    mut root_q: Query<&mut Visibility, With<CubeRoot>>,
    mut fill_q: Query<
        (&CubeFill, &mut Node, &mut BackgroundGradient),
        (Without<CubeMarker>, Without<CubeWireEdge>),
    >,
    mut marker_q: Query<
        (
            &CubeMarker,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut UiTransform,
        ),
        (Without<CubeFill>, Without<CubeWireEdge>, Without<CubeRoot>),
    >,
    mut marker_text_q: Query<(&CubeMarkerText, &mut Text)>,
    mut wire_q: Query<
        (
            &CubeWireEdge,
            &mut Node,
            &mut BackgroundGradient,
            &mut UiTransform,
        ),
        (Without<CubeFill>, Without<CubeMarker>, Without<CubeRoot>),
    >,
) {
    let _ = local_id;

    let cube = own_entity
        .as_ref()
        .and_then(|own| server_q.get(own.0).ok());

    let Ok(mut root_vis) = root_q.single_mut() else { return };

    let Some(cube) = cube else {
        *root_vis = Visibility::Hidden;
        return;
    };

    if !cube.active {
        *root_vis = Visibility::Hidden;
        return;
    }
    *root_vis = Visibility::Visible;

    // Pixel dimensions of the cube (needed to compute fill max-lengths).
    let (cube_w, cube_h) = node_size_q
        .single()
        .map(|c| {
            let s = c.size();
            (s.x, s.y)
        })
        .unwrap_or((0.0, 0.0));

    let edge_colour = rotations_gradient(cube.rotations_remaining);
    let in_window = (cube.fill_progress - 1.0).abs() <= CUBE_COLLECT_WINDOW;

    // Phase detection. `rotating_edge` is `Some` through the whole post-collect
    // sequence (pop → rotate → hold). Only the pop sub-phase should still draw
    // fills + markers; the rotate/hold sub-phases are wireframe-only.
    let landed = cube.rotating_edge;
    let popping = landed.is_some() && cube.pop_progress < 1.0;
    let rotating_or_holding = landed.is_some() && !popping;
    let pop_t = pop_curve(cube.pop_progress);

    // ── Fills ────────────────────────────────────────────────────────────────
    // Fills stay drawn during the pop so the hit visually freezes a beat; they
    // hide once the wireframe takes over for rotate + hold.
    let fill_palette = EdgePalette::from_base(edge_colour, 1.0);
    for (fill, mut node, mut bg) in fill_q.iter_mut() {
        let max_len = max_fill_length(fill.edge, cube_w, cube_h);
        let t = if rotating_or_holding {
            0.0
        } else {
            cube.fill_progress.min(1.0).max(0.0)
        };
        let len = (max_len * t).max(0.0);

        match fill.edge {
            CubeEdge::Left | CubeEdge::Right => {
                node.height = Val::Px(len);
                node.width = Val::Px(EDGE_THICK_PX);
            }
            CubeEdge::Bottom => {
                node.width = Val::Px(len);
                node.height = Val::Px(EDGE_THICK_PX);
            }
        }

        *bg = if rotating_or_holding {
            BackgroundGradient::default()
        } else {
            BackgroundGradient(vec![fill_gradient(
                &fill_palette,
                fill.edge,
                fill.from_start,
            )])
        };
    }

    // ── Markers ──────────────────────────────────────────────────────────────
    // Visible during fill + pop; hidden while the wireframe is on. During the
    // pop phase the landed marker scales up and brightens for hit feedback.
    for (marker, mut bg, mut border, mut xform) in marker_q.iter_mut() {
        let has_bonus = cube.current_face[marker.0.index()].is_some();
        if rotating_or_holding || !has_bonus {
            *bg = BackgroundColor(Color::NONE);
            *border = BorderColor {
                top: Color::NONE,
                bottom: Color::NONE,
                left: Color::NONE,
                right: Color::NONE,
            };
            xform.scale = Vec2::ONE;
            continue;
        }

        let is_landed = popping && landed == Some(marker.0);
        let scale = if is_landed {
            1.0 + (POP_PEAK_SCALE - 1.0) * pop_t
        } else {
            1.0
        };
        xform.scale = Vec2::splat(scale);

        // During the pop, brighten the landed marker's fill toward "hot".
        let base_fill = marker_fill(in_window || popping);
        let fill_col = if is_landed {
            scale_color(base_fill, 1.0 + (POP_PEAK_BRIGHTNESS - 1.0) * pop_t)
        } else {
            base_fill
        };
        *bg = BackgroundColor(fill_col);
        let b = edge_colour;
        *border = BorderColor {
            top: b,
            bottom: b,
            left: b,
            right: b,
        };
    }

    // ── Wireframe (rotate + hold only) ───────────────────────────────────────
    update_wireframe(cube, &mut wire_q, edge_colour, rotating_or_holding);

    // ── Marker text ──────────────────────────────────────────────────────────
    for (marker_text, mut text) in marker_text_q.iter_mut() {
        let label = cube.current_face[marker_text.0.index()]
            .as_ref()
            .map(PhysicalBonus::short_label)
            .unwrap_or("");
        if text.0.as_str() != label {
            text.0 = label.to_string();
        }
    }
}

/// Maximum length (px) from a corner to the edge midpoint. At `fill_progress = 1.0`
/// the two corner fills meet under the marker, so the whole edge reads as filled.
fn max_fill_length(edge: CubeEdge, cube_w: f32, cube_h: f32) -> f32 {
    match edge {
        CubeEdge::Left | CubeEdge::Right => (cube_h * 0.5).max(0.0),
        CubeEdge::Bottom => (cube_w * 0.5).max(0.0),
    }
}

// ── Wireframe rendering ──────────────────────────────────────────────────────

/// Rotate a model-space vertex around the axis appropriate for `edge`, by
/// angle θ. Left/Right pivot around the vertical (Y) axis so the named face
/// swings to the front; Bottom pivots around the horizontal (X) axis so the
/// bottom face lifts toward the viewer.
fn rotate_vertex(v: Vec3, edge: CubeEdge, theta: f32) -> Vec3 {
    let c = theta.cos();
    let s = theta.sin();
    match edge {
        CubeEdge::Left => Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c),
        CubeEdge::Right => Vec3::new(v.x * c - v.z * s, v.y, v.x * s + v.z * c),
        CubeEdge::Bottom => Vec3::new(v.x, v.y * c + v.z * s, -v.y * s + v.z * c),
    }
}

/// Orthographic projection of a rotated unit-cube vertex into CubeRoot-local
/// pixel coordinates. Model y is up; UI y is down, so y is flipped.
fn project_to_ui(v: Vec3, half: f32) -> Vec2 {
    Vec2::new(half + v.x * half, half - v.y * half)
}

fn update_wireframe(
    cube: &CubeState,
    wire_q: &mut Query<
        (
            &CubeWireEdge,
            &mut Node,
            &mut BackgroundGradient,
            &mut UiTransform,
        ),
        (Without<CubeFill>, Without<CubeMarker>, Without<CubeRoot>),
    >,
    edge_colour: Color,
    active: bool,
) {
    let Some(rot_edge) = cube.rotating_edge else {
        for (_, _, mut bg, _) in wire_q.iter_mut() {
            *bg = BackgroundGradient::default();
        }
        return;
    };
    if !active {
        // Pop phase: `rotating_edge` is set but wireframe isn't shown yet.
        for (_, _, mut bg, _) in wire_q.iter_mut() {
            *bg = BackgroundGradient::default();
        }
        return;
    }

    // A full 90° swing over the rotation animation brings the landed face to
    // the front position — exactly one quarter-turn per collect. During the
    // hold phase `rotation_progress` stays at 1.0 and θ stays at π/2.
    let theta = cube.rotation_progress.min(1.0) * std::f32::consts::FRAC_PI_2;
    let half = CUBE_SIZE_PX * 0.5;
    let rotated: [Vec3; 8] =
        std::array::from_fn(|i| rotate_vertex(CUBE_VERTS[i], rot_edge, theta));
    let projected: [Vec2; 8] = std::array::from_fn(|i| project_to_ui(rotated[i], half));

    // Traveling highlight sweep position along the edge (0..1 then wraps),
    // driven by rotation progress so the "glint" moves while the cube turns.
    let glint_t = (cube.rotation_progress * 1.6).fract();

    for (we, mut node, mut bg, mut xform) in wire_q.iter_mut() {
        let (i1, i2) = CUBE_EDGES[we.0];
        let p1 = projected[i1];
        let p2 = projected[i2];
        let diff = p2 - p1;
        let length = diff.length();
        let mid = 0.5 * (p1 + p2);
        // atan2 on UI coords: y grows downward, so angle is the on-screen
        // rotation of the line (CCW in screen orientation == CW geometrically).
        let angle = if length > 0.01 { diff.y.atan2(diff.x) } else { 0.0 };

        node.left = Val::Px(mid.x - length * 0.5);
        node.top = Val::Px(mid.y - WIRE_THICK_PX * 0.5);
        node.width = Val::Px(length);
        node.height = Val::Px(WIRE_THICK_PX);
        xform.rotation = Rot2::radians(angle);
        xform.scale = Vec2::ONE;
        xform.translation = Val2::ZERO;

        // Depth cue: edges whose projected midpoint sits further from the
        // viewer (smaller z') render darker. z' ∈ [-1, +1]; remap to [0.45, 1.1].
        let avg_z = 0.5 * (rotated[i1].z + rotated[i2].z);
        let depth = (avg_z + 1.0) * 0.5; // 0 (back) .. 1 (front)
        let intensity = 0.45 + 0.65 * depth;

        let palette = EdgePalette::from_base(edge_colour, intensity);
        *bg = BackgroundGradient(vec![wire_gradient(&palette, glint_t)]);
    }
}

/// Build a single wireframe edge's gradient: dim at the endpoints, bright in
/// the middle, plus a narrow traveling highlight that sweeps along the edge.
fn wire_gradient(palette: &EdgePalette, glint_t: f32) -> Gradient {
    // Glint window: 10% wide band centered on glint_t. Clamp so the stops
    // stay strictly increasing (gradient systems dislike out-of-order stops).
    const GLINT_HALF: f32 = 0.06;
    let g_lo = (glint_t - GLINT_HALF).clamp(0.02, 0.98);
    let g_hi = (glint_t + GLINT_HALF).clamp(0.02, 0.98);
    let g_mid = ((g_lo + g_hi) * 0.5).clamp(0.02, 0.98);

    // Use `TO_RIGHT` so the gradient runs along the edge's length (node x-axis).
    LinearGradient::new(
        LinearGradient::TO_RIGHT,
        vec![
            ColorStop::new(palette.dim, Val::Percent(0.0)),
            ColorStop::new(palette.bright, Val::Percent(g_lo * 100.0 - 8.0)),
            ColorStop::new(palette.hot, Val::Percent(g_mid * 100.0)),
            ColorStop::new(palette.bright, Val::Percent(g_hi * 100.0 + 8.0)),
            ColorStop::new(palette.dim, Val::Percent(100.0)),
        ],
    )
    .into()
}

/// Gradient for a single corner-fill: runs from the corner (dim) to the edge
/// midpoint (hot), so the two mirrored fills of one edge combine into the same
/// dim→bright→hot→bright→dim sweep the wireframe draws. Direction depends on
/// which corner the fill is anchored to.
fn fill_gradient(palette: &EdgePalette, edge: CubeEdge, from_start: bool) -> Gradient {
    let angle = match edge {
        // L/R fills are vertical; Bottom fill is horizontal.
        CubeEdge::Left | CubeEdge::Right => {
            if from_start {
                LinearGradient::TO_BOTTOM
            } else {
                LinearGradient::TO_TOP
            }
        }
        CubeEdge::Bottom => {
            if from_start {
                LinearGradient::TO_RIGHT
            } else {
                LinearGradient::TO_LEFT
            }
        }
    };
    LinearGradient::new(
        angle,
        vec![
            ColorStop::new(palette.dim, Val::Percent(0.0)),
            ColorStop::new(palette.bright, Val::Percent(70.0)),
            ColorStop::new(palette.hot, Val::Percent(100.0)),
        ],
    )
    .into()
}

/// Multiply a colour's RGB channels by `factor`, clamping to a safe range.
/// Alpha is preserved.
fn scale_color(c: Color, factor: f32) -> Color {
    let s = c.to_srgba();
    Color::srgba(
        (s.red * factor).clamp(0.0, 1.5),
        (s.green * factor).clamp(0.0, 1.5),
        (s.blue * factor).clamp(0.0, 1.5),
        s.alpha,
    )
}

/// Ease-out-quad curve for the pop animation: accelerates early, settles at
/// peak near `progress = 1`. Returns a value in [0, 1].
fn pop_curve(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    let inv = 1.0 - p;
    1.0 - inv * inv
}

/// Linear interpolation between two sRGB colours at blend factor `t` ∈ [0, 1].
fn mix_colors(a: Color, b: Color, t: f32) -> Color {
    let a = a.to_srgba();
    let b = b.to_srgba();
    let t = t.clamp(0.0, 1.0);
    Color::srgba(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
        a.alpha + (b.alpha - a.alpha) * t,
    )
}
