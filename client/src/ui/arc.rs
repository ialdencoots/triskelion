use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    ui::ComputedNode,
};

use shared::components::combat::CombatState;
use shared::components::minigame::arc::{ArcState, SecondaryArcState};
use shared::components::player::{PlayerId, RoleStance};

use crate::plugin::LocalClientId;
use crate::world::players::OwnServerEntity;

use super::hud::minigame_anchor::MinigameRoot;

// ── Material definition ───────────────────────────────────────────────────────

/// GPU parameters for the arc fragment shader.
#[derive(ShaderType, Clone, Debug, Default)]
pub struct ArcParams {
    /// x = theta, y = amplitude, z = in_lockout (0/1 as f32), w = ghost_count
    pub core: Vec4,
    /// Ghost theta values for indices 0–3.
    pub ghost_a: Vec4,
    /// Ghost theta values for indices 4–5 in x, y; z and w unused.
    pub ghost_b: Vec4,
    /// x = node_width_px, y = node_height_px, z = time_secs, w = unused
    pub dimensions: Vec4,
    /// x = commit_pulse (1→0 over ~0.5 s), y = physical theta of last commit
    pub commit: Vec4,
    /// x = scroll_carry (accumulated un-finished scroll from interrupted animations)
    pub extra: Vec4,
}

#[derive(AsBindGroup, Asset, TypePath, Debug, Clone, Default)]
pub struct ArcMaterial {
    #[uniform(0)]
    pub params: ArcParams,
}

impl UiMaterial for ArcMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/arc.wgsl".into()
    }
}

// ── Client-local ghost history ────────────────────────────────────────────────

pub const MAX_GHOST_ENTRIES: usize = 6;
/// Maximum ghost entries for the unified DPS central stack.
const MAX_CENTRAL_GHOST_ENTRIES: usize = 8;

/// Rotation angle (radians) for the 22.5° DPS arc tilt. Applied with opposite
/// signs on primary (−) and secondary (+) so they V toward the center.
const DPS_TILT: f32 = std::f32::consts::FRAC_PI_3; // π/3 = 60°

/// sin(ARC_THETA_MIN) = sin(π/8) — mirrors the shader constant for computing offsets.
const SIN_ARC_THETA_MIN: f32 = 0.38268;

/// Decay rate for ghost commit pulse animations (1.0 → 0.0 per second).
const GHOST_PULSE_DECAY: f32 = 2.0;

/// Arc depth as a fraction of node width — mirrors the shader's `DEPTH` ratio.
const ARC_DEPTH_RATIO: f32 = 0.310;
/// Arc half-width as a fraction of node width — mirrors the shader's `HALF_W` ratio.
const ARC_HALF_W_RATIO: f32 = 0.452;
/// Horizontal distance from a side arc's center to the central node's center,
/// as a fraction of the central node width. Drives ghost[0] fly-in animation.
const SIDE_ARC_CX_OFFSET_RATIO: f32 = 0.24;

/// Client-local record of recent commit positions. Not replicated — maintained
/// by detecting `in_lockout` false→true transitions in the render system.
#[derive(Component)]
pub struct GhostArcHistory {
    /// Theta at each recent commit; index 0 = most recent.
    pub entries: Vec<f32>,
    /// Previous `in_lockout` value — used for edge detection.
    pub prev_in_lockout: bool,
    /// Previous stance — used to detect stance exit and clear history.
    pub prev_stance: Option<RoleStance>,
    /// Physical theta of the last commit — drives commit color in the shader.
    pub commit_theta: f32,
    /// Decays from 1.0 → 0.0 after a commit; drives arc pulse and dot color.
    pub commit_pulse: f32,
}

impl Default for GhostArcHistory {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            prev_in_lockout: false,
            prev_stance: None,
            commit_theta: std::f32::consts::FRAC_PI_2,
            commit_pulse: 0.0,
        }
    }
}

// ── Node markers ─────────────────────────────────────────────────────────────

/// Marks the primary arc `MaterialNode` child of `MinigameRoot`.
#[derive(Component)]
pub struct PrimaryArcNode;

/// Marks the secondary arc `MaterialNode` child of `MinigameRoot` (DPS only).
#[derive(Component)]
pub struct SecondaryArcNode;

/// Marks the central ghost-stack `MaterialNode` child of `MinigameRoot` (DPS only).
#[derive(Component)]
pub struct CentralGhostNode;

/// Client-local ghost history for the secondary (left) arc in DPS stance.
#[derive(Component, Default)]
pub struct SecondaryGhostArcHistory(pub GhostArcHistory);

/// Combined ghost history for DPS stance — aggregates commits from both arcs
/// in recency order so the central ghost stack shows a unified trail.
#[derive(Component)]
pub struct CentralGhostHistory {
    pub entries: Vec<f32>,
    pub prev_stance: Option<RoleStance>,
    /// 1.0→0.0 animation timer for ghost[0] flying into the stack.
    pub commit_pulse: f32,
    /// Source tilt (±DPS_TILT) of the most recent commit, drives animation start.
    pub last_commit_tilt: f32,
    /// Remaining scroll displacement (in STACK_OFFSET units) carried over from an
    /// animation that was interrupted by a new commit. Prevents teleporting.
    pub scroll_carry: f32,
}

impl Default for CentralGhostHistory {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            prev_stance: None,
            commit_pulse: 0.0,
            last_commit_tilt: 0.0,
            scroll_carry: 0.0,
        }
    }
}

// ── Ghost history setup ───────────────────────────────────────────────────────

pub fn on_arc_state_added(trigger: On<Add, ArcState>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .insert((GhostArcHistory::default(), CentralGhostHistory::default()));
}

pub fn on_secondary_arc_state_added(trigger: On<Add, SecondaryArcState>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .insert(SecondaryGhostArcHistory::default());
}

// ── Spawn ─────────────────────────────────────────────────────────────────────

pub fn spawn_arc_overlay(
    mut commands: Commands,
    mut materials: ResMut<Assets<ArcMaterial>>,
    root_q: Query<Entity, With<MinigameRoot>>,
) {
    let Ok(root) = root_q.single() else { return };

    let primary_mat = materials.add(ArcMaterial::default());
    let secondary_mat = materials.add(ArcMaterial::default());
    let central_mat = materials.add(ArcMaterial::default());

    commands.entity(root).with_children(|parent| {
        // Primary arc — full width by default; shrinks to half in DPS stance.
        parent.spawn((
            PrimaryArcNode,
            MaterialNode(primary_mat),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                ..default()
            },
            Visibility::Hidden,
        ));

        // Secondary arc — left half, only visible in DPS stance.
        parent.spawn((
            SecondaryArcNode,
            MaterialNode(secondary_mat),
            Node {
                width: Val::Percent(50.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Percent(13.0),
                top: Val::Px(0.0),
                ..default()
            },
            Visibility::Hidden,
        ));

        // Central ghost stack — centered, DPS only. Renders unified commit history
        // from both arcs as horizontal ghost arcs; the live arc is hidden.
        parent.spawn((
            CentralGhostNode,
            MaterialNode(central_mat),
            Node {
                width: Val::Percent(50.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Percent(25.0),
                top: Val::Px(0.0),
                ..default()
            },
            Visibility::Hidden,
        ));
    });
}

// ── Ghost history helpers ─────────────────────────────────────────────────────

/// Detects a lockout rising edge, records the commit in `ghost` (and optionally
/// the central history), then decays the commit pulse.
///
/// `push_to_ghost_trail` — true for the primary arc (which shows its own trail
/// in Tank/Heal stance), false for the secondary arc (central-only history).
fn tick_ghost_history(
    ghost: &mut GhostArcHistory,
    in_lockout: bool,
    commit_theta: f32,
    push_to_ghost_trail: bool,
    ghost_max: usize,
    central: Option<&mut CentralGhostHistory>,
    tilt: f32,
    dt: f32,
) {
    if !ghost.prev_in_lockout && in_lockout {
        if push_to_ghost_trail {
            ghost.entries.insert(0, commit_theta);
            ghost.entries.truncate(ghost_max);
        }
        ghost.commit_theta = commit_theta;
        ghost.commit_pulse = 1.0;
        if let Some(c) = central {
            let old_scroll_t = (c.commit_pulse * 2.0 - 1.0).clamp(0.0, 1.0);
            c.scroll_carry = old_scroll_t * (1.0 + c.scroll_carry);
            c.entries.insert(0, commit_theta);
            c.entries.truncate(MAX_CENTRAL_GHOST_ENTRIES);
            c.commit_pulse = 1.0;
            c.last_commit_tilt = tilt;
        }
    }
    ghost.prev_in_lockout = in_lockout;
    ghost.commit_pulse = (ghost.commit_pulse - dt * GHOST_PULSE_DECAY).max(0.0);
}

/// Computes the vertical ghost stack offset and horizontal source-cx offset (px)
/// for the central ghost node, based on the DPS arc geometry.
fn compute_central_ghost_layout(node_w: f32, last_commit_tilt: f32) -> (f32, f32) {
    let depth_px  = node_w * ARC_DEPTH_RATIO;
    let half_w_px = node_w * ARC_HALF_W_RATIO;
    let t_sin = DPS_TILT.sin();
    let t_cos = DPS_TILT.cos();
    let arc_y_off = (half_w_px * t_sin - depth_px * SIN_ARC_THETA_MIN * t_cos).max(0.0);
    let ghost_y_offset = arc_y_off + t_cos * depth_px + 1.0;
    // Primary  left=37% → center 62% → +SIDE_ARC_CX_OFFSET_RATIO of central node width
    // Secondary left=13% → center 38% → -SIDE_ARC_CX_OFFSET_RATIO
    let source_cx_offset = if last_commit_tilt < 0.0 {
        node_w * SIDE_ARC_CX_OFFSET_RATIO
    } else {
        -node_w * SIDE_ARC_CX_OFFSET_RATIO
    };
    (ghost_y_offset, source_cx_offset)
}

// ── Render ────────────────────────────────────────────────────────────────────

pub fn render_arc(
    time: Res<Time>,
    local_id: Res<LocalClientId>,
    own_entity: Option<Res<OwnServerEntity>>,
    mut server_q: Query<(
        &PlayerId,
        &CombatState,
        Option<&ArcState>,
        Option<&SecondaryArcState>,
        Option<&mut GhostArcHistory>,
        Option<&mut SecondaryGhostArcHistory>,
        Option<&mut CentralGhostHistory>,
    )>,
    mut primary_q: Query<
        (&mut Visibility, &mut Node, &MaterialNode<ArcMaterial>, &ComputedNode),
        (With<PrimaryArcNode>, Without<SecondaryArcNode>, Without<CentralGhostNode>),
    >,
    mut secondary_q: Query<
        (&mut Visibility, &MaterialNode<ArcMaterial>, &ComputedNode),
        (With<SecondaryArcNode>, Without<PrimaryArcNode>, Without<CentralGhostNode>),
    >,
    mut central_q: Query<
        (&mut Visibility, &MaterialNode<ArcMaterial>, &ComputedNode),
        (With<CentralGhostNode>, Without<PrimaryArcNode>, Without<SecondaryArcNode>),
    >,
    mut arc_materials: ResMut<Assets<ArcMaterial>>,
) {
    let Some(own) = own_entity else { return };
    let Ok((_pid, combat, arc_opt, secondary_opt, ghost_opt, mut sec_ghost_opt, mut central_opt)) =
        server_q.get_mut(own.0)
    else {
        return;
    };

    let in_stance = combat.active_stance.is_some();
    let in_dps = combat.active_stance == Some(RoleStance::Dps);
    let t = time.elapsed_secs();

    if let (Some(arc), Some(mut ghost)) = (arc_opt, ghost_opt) {
        // Clear all histories on any stance change.
        if ghost.prev_stance != combat.active_stance {
            ghost.entries.clear();
            ghost.prev_in_lockout = false;
            ghost.commit_pulse = 0.0;
            ghost.commit_theta = std::f32::consts::FRAC_PI_2;
            if let Some(ref mut central) = central_opt {
                central.entries.clear();
                central.commit_pulse = 0.0;
                central.scroll_carry = 0.0;
            }
        }
        ghost.prev_stance = combat.active_stance;
        if let Some(ref mut central) = central_opt {
            central.prev_stance = combat.active_stance;
        }

        // Primary commit detection — push to ghost trail and central history.
        tick_ghost_history(
            &mut ghost,
            arc.in_lockout,
            arc.last_commit_theta,
            true,
            MAX_GHOST_ENTRIES,
            central_opt.as_deref_mut(),
            -DPS_TILT,
            time.delta_secs(),
        );

        // Primary arc — in DPS pass a sparse ghost (commit pulse/theta only, no trail).
        if let Ok((mut vis, mut node, mat_handle, computed)) = primary_q.single_mut() {
            *vis = if in_stance { Visibility::Visible } else { Visibility::Hidden };
            // Tank/Heal: centered at 50% width, matching one DPS arc.
            // DPS: primary (right) at left=37%; secondary lives at left=13%.
            node.width = Val::Percent(50.0);
            node.left  = if in_dps { Val::Percent(37.0) } else { Val::Percent(25.0) };

            if let Some(mat) = arc_materials.get_mut(mat_handle.id()) {
                let size = computed.size();
                let tilt = if in_dps { -DPS_TILT } else { 0.0 };
                let sparse;
                let ghost_ref: &GhostArcHistory = if in_dps {
                    sparse = GhostArcHistory {
                        commit_pulse: ghost.commit_pulse,
                        commit_theta: ghost.commit_theta,
                        ..GhostArcHistory::default()
                    };
                    &sparse
                } else {
                    &*ghost
                };
                mat.params = arc_to_params(arc, ghost_ref, size.x, size.y, t, tilt);
            }
        }

        // Clear secondary ghost on any stance change.
        if let Some(ref mut sec_ghost) = sec_ghost_opt {
            if sec_ghost.0.prev_stance != combat.active_stance {
                sec_ghost.0.prev_in_lockout = false;
                sec_ghost.0.commit_pulse = 0.0;
                sec_ghost.0.commit_theta = std::f32::consts::FRAC_PI_2;
            }
            sec_ghost.0.prev_stance = combat.active_stance;
        }

        // Secondary arc.
        if let Ok((mut vis, mat_handle, computed)) = secondary_q.single_mut() {
            if in_dps && in_stance {
                *vis = Visibility::Visible;
                if let Some(secondary) = secondary_opt {
                    // Secondary commit detection — push to central history only.
                    if let Some(ref mut sec_ghost) = sec_ghost_opt {
                        tick_ghost_history(
                            &mut sec_ghost.0,
                            secondary.0.in_lockout,
                            secondary.0.last_commit_theta,
                            false,
                            MAX_GHOST_ENTRIES,
                            central_opt.as_deref_mut(),
                            DPS_TILT,
                            time.delta_secs(),
                        );
                    }

                    if let Some(mat) = arc_materials.get_mut(mat_handle.id()) {
                        let size = computed.size();
                        let commit_pulse =
                            sec_ghost_opt.as_ref().map_or(0.0, |sg| sg.0.commit_pulse);
                        let commit_theta = sec_ghost_opt
                            .as_ref()
                            .map_or(std::f32::consts::FRAC_PI_2, |sg| sg.0.commit_theta);
                        let sec_sparse = GhostArcHistory {
                            commit_pulse,
                            commit_theta,
                            ..GhostArcHistory::default()
                        };
                        mat.params =
                            arc_to_params(&secondary.0, &sec_sparse, size.x, size.y, t, DPS_TILT);
                    }
                }
            } else {
                *vis = Visibility::Hidden;
            }
        }

        // Decay central animation pulse.
        if let Some(ref mut central) = central_opt {
            central.commit_pulse = (central.commit_pulse - time.delta_secs() * GHOST_PULSE_DECAY).max(0.0);
        }

        // Central ghost stack — visible in DPS only.
        if let Ok((mut vis, mat_handle, computed)) = central_q.single_mut() {
            if in_dps && in_stance {
                *vis = Visibility::Visible;
                if let Some(mat) = arc_materials.get_mut(mat_handle.id()) {
                    let size = computed.size();
                    let entries =
                        central_opt.as_ref().map_or(&[][..], |c| c.entries.as_slice());
                    let commit_pulse =
                        central_opt.as_ref().map_or(0.0, |c| c.commit_pulse);
                    let last_commit_tilt =
                        central_opt.as_ref().map_or(0.0, |c| c.last_commit_tilt);
                    let scroll_carry =
                        central_opt.as_ref().map_or(0.0, |c| c.scroll_carry);
                    let (ghost_y_offset, source_cx_offset) =
                        compute_central_ghost_layout(size.x, last_commit_tilt);
                    mat.params = central_ghost_params(
                        entries, arc.amplitude, size.x, size.y, t,
                        ghost_y_offset, commit_pulse, last_commit_tilt,
                        source_cx_offset, scroll_carry,
                    );
                }
            } else {
                *vis = Visibility::Hidden;
            }
        }
    } else {
        if let Ok((mut vis, _, _, _)) = primary_q.single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Ok((mut vis, _, _)) = secondary_q.single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Ok((mut vis, _, _)) = central_q.single_mut() {
            *vis = Visibility::Hidden;
        }
    }
}

/// Builds ArcParams for the central ghost-only node. Sets `commit.z = 1.0` so
/// the shader skips the main arc and dot, showing only the ghost stack.
/// `ghost_y_offset` (pixels, stored in `commit.w`) pushes the stack below the
/// tilted main arcs so they don't visually overlap.
fn central_ghost_params(
    entries: &[f32],
    amplitude: f32,
    w: f32,
    h: f32,
    t: f32,
    ghost_y_offset: f32,
    commit_pulse: f32,
    last_commit_tilt: f32,
    source_cx_offset: f32,
    scroll_carry: f32,
) -> ArcParams {
    let ghost_count = entries.len().min(MAX_CENTRAL_GHOST_ENTRIES) as f32;
    let g = |i: usize| entries.get(i).copied().unwrap_or(0.0);
    ArcParams {
        core: Vec4::new(std::f32::consts::FRAC_PI_2, amplitude, 0.0, ghost_count),
        ghost_a: Vec4::new(g(0), g(1), g(2), g(3)),
        ghost_b: Vec4::new(g(4), g(5), g(6), g(7)),
        // w repurposed as source_cx_offset for ghost[0] animation (tilt always 0 for ghosts).
        dimensions: Vec4::new(w, h, t, source_cx_offset),
        // x=commit_pulse, y=source tilt, z=hide_main, w=ghost_y_offset
        commit: Vec4::new(commit_pulse, last_commit_tilt, 1.0, ghost_y_offset),
        // x=scroll_carry: unfinished scroll from interrupted animation
        extra: Vec4::new(scroll_carry, 0.0, 0.0, 0.0),
    }
}

fn arc_to_params(arc: &ArcState, ghost: &GhostArcHistory, w: f32, h: f32, t: f32, tilt: f32) -> ArcParams {
    let ghost_count = ghost.entries.len().min(MAX_GHOST_ENTRIES) as f32;
    let g = |i: usize| ghost.entries.get(i).copied().unwrap_or(0.0);
    ArcParams {
        core: Vec4::new(
            arc.theta,
            arc.amplitude,
            if arc.in_lockout { 1.0 } else { 0.0 },
            ghost_count,
        ),
        ghost_a: Vec4::new(g(0), g(1), g(2), g(3)),
        ghost_b: Vec4::new(g(4), g(5), g(6), g(7)),
        dimensions: Vec4::new(w, h, t, tilt),
        commit: Vec4::new(ghost.commit_pulse, ghost.commit_theta, 0.0, 0.0),
        extra: Vec4::ZERO,
    }
}
