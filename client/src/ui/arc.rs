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

/// Rotation angle (radians) for the 22.5° DPS arc tilt. Applied with opposite
/// signs on primary (−) and secondary (+) so they V toward the center.
const DPS_TILT: f32 = std::f32::consts::FRAC_PI_8; // π/8 = 22.5°

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

/// Client-local ghost history for the secondary (left) arc in DPS stance.
#[derive(Component, Default)]
pub struct SecondaryGhostArcHistory(pub GhostArcHistory);

// ── Ghost history setup ───────────────────────────────────────────────────────

pub fn on_arc_state_added(trigger: On<Add, ArcState>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .insert(GhostArcHistory::default());
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
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                ..default()
            },
            Visibility::Hidden,
        ));
    });
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
    )>,
    mut primary_q: Query<
        (&mut Visibility, &mut Node, &MaterialNode<ArcMaterial>, &ComputedNode),
        (With<PrimaryArcNode>, Without<SecondaryArcNode>),
    >,
    mut secondary_q: Query<
        (&mut Visibility, &MaterialNode<ArcMaterial>, &ComputedNode),
        (With<SecondaryArcNode>, Without<PrimaryArcNode>),
    >,
    mut arc_materials: ResMut<Assets<ArcMaterial>>,
) {
    let Some(own) = own_entity else { return };
    let Ok((_pid, combat, arc_opt, secondary_opt, ghost_opt, mut sec_ghost_opt)) = server_q.get_mut(own.0) else {
        return;
    };

    let in_stance = combat.active_stance.is_some();
    let in_dps = combat.active_stance == Some(RoleStance::Dps);
    let t = time.elapsed_secs();

    if let (Some(arc), Some(mut ghost)) = (arc_opt, ghost_opt) {
        // Clear history on any stance change (exit or switch).
        if ghost.prev_stance != combat.active_stance {
            ghost.entries.clear();
            ghost.prev_in_lockout = false;
            ghost.commit_pulse = 0.0;
            ghost.commit_theta = std::f32::consts::FRAC_PI_2;
        }
        ghost.prev_stance = combat.active_stance;

        // Ghost history edge detection: in_lockout false→true = new commit.
        if !ghost.prev_in_lockout && arc.in_lockout {
            ghost.entries.insert(0, arc.last_commit_theta);
            ghost.entries.truncate(MAX_GHOST_ENTRIES);
            ghost.commit_theta = arc.last_commit_theta;
            ghost.commit_pulse = 1.0;
        }
        ghost.prev_in_lockout = arc.in_lockout;

        // Decay commit pulse (~0.5 s duration).
        ghost.commit_pulse = (ghost.commit_pulse - time.delta_secs() * 2.0).max(0.0);

        if let Ok((mut vis, mut node, mat_handle, computed)) = primary_q.single_mut() {
            *vis = if in_stance { Visibility::Visible } else { Visibility::Hidden };
            node.width = if in_dps { Val::Percent(50.0) } else { Val::Percent(100.0) };
            node.left  = if in_dps { Val::Percent(50.0) } else { Val::Px(0.0) };

            if let Some(mat) = arc_materials.get_mut(mat_handle.id()) {
                let size = computed.size();
                let tilt = if in_dps { -DPS_TILT } else { 0.0 };
                mat.params = arc_to_params(arc, &ghost, size.x, size.y, t, tilt);
            }
        }

        // Clear secondary ghost on any stance change, mirroring primary ghost behavior.
        if let Some(ref mut sec_ghost) = sec_ghost_opt {
            if sec_ghost.0.prev_stance != combat.active_stance {
                sec_ghost.0.entries.clear();
                sec_ghost.0.prev_in_lockout = false;
                sec_ghost.0.commit_pulse = 0.0;
                sec_ghost.0.commit_theta = std::f32::consts::FRAC_PI_2;
            }
            sec_ghost.0.prev_stance = combat.active_stance;
        }

        if let Ok((mut vis, mat_handle, computed)) = secondary_q.single_mut() {
            if in_dps && in_stance {
                *vis = Visibility::Visible;
                if let Some(secondary) = secondary_opt {
                    if let Some(ref mut sec_ghost) = sec_ghost_opt {
                        if !sec_ghost.0.prev_in_lockout && secondary.0.in_lockout {
                            sec_ghost.0.entries.insert(0, secondary.0.last_commit_theta);
                            sec_ghost.0.entries.truncate(MAX_GHOST_ENTRIES);
                            sec_ghost.0.commit_theta = secondary.0.last_commit_theta;
                            sec_ghost.0.commit_pulse = 1.0;
                        }
                        sec_ghost.0.prev_in_lockout = secondary.0.in_lockout;
                        sec_ghost.0.commit_pulse = (sec_ghost.0.commit_pulse - time.delta_secs() * 2.0).max(0.0);
                    }

                    // Always update the material — use ghost if present, empty otherwise.
                    if let Some(mat) = arc_materials.get_mut(mat_handle.id()) {
                        let size = computed.size();
                        let empty = GhostArcHistory::default();
                        let ghost_ref = match sec_ghost_opt {
                            Some(ref sg) => &sg.0,
                            None => &empty,
                        };
                        mat.params = arc_to_params(&secondary.0, ghost_ref, size.x, size.y, t, DPS_TILT);
                    }
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
        ghost_b: Vec4::new(g(4), g(5), 0.0, 0.0),
        dimensions: Vec4::new(w, h, t, tilt),
        commit: Vec4::new(ghost.commit_pulse, ghost.commit_theta, 0.0, 0.0),
    }
}
