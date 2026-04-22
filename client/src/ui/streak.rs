use bevy::math::{Rot2, Vec2};
use bevy::prelude::*;
use bevy::ui::{UiTransform, Val2};

use shared::components::combat::CombatState;
use shared::components::minigame::arc::{ArcState, SecondaryArcState};
use shared::components::player::RoleStance;

use crate::plugin::LocalClientId;
use crate::world::players::OwnServerEntity;

use super::hud::minigame_anchor::MinigameRoot;
use super::theme::NADIR_GREEN;

/// Which arc a counter reads and where it renders.
///
/// `TankHeal` uses the primary `ArcState` and centers horizontally. The two DPS
/// variants read their respective arcs and sit beneath each tilted arc.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StreakCounterKind {
    TankHeal,
    DpsPrimary,
    DpsSecondary,
}

/// One streak-counter text node. Visibility is gated by stance; animation state
/// lives here so the system is stateless beyond reading `ArcState`.
#[derive(Component)]
pub struct StreakCounter {
    pub kind: StreakCounterKind,
    /// Streak value observed last frame — used for edge detection.
    pub prev_streak: u32,
    /// `in_lockout` observed last frame — drives commit edge detection so the
    /// counter can react to mid-zone commits that don't change `streak`.
    pub prev_in_lockout: bool,
    /// Seconds remaining in the current animation (0 = idle).
    pub anim_remaining: f32,
    /// Total duration of the current animation, for progress calc.
    pub anim_total: f32,
    pub anim: StreakAnim,
    /// The streak value at the moment the current Break animation began —
    /// displayed on the fragments so the visible shards match what the player
    /// was looking at just before the break.
    pub shattered_value: u32,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StreakAnim {
    None,
    /// Streak went up (nadir-zone commit) — green flash fading back to white,
    /// bigger pop.
    Increment,
    /// Commit landed but the streak didn't change (mid-zone) — gentle white
    /// pop, no color shift.
    Neutral,
    /// Streak dropped to 0 — main text hides, fragments fly apart.
    Break,
}

impl StreakCounter {
    fn new(kind: StreakCounterKind) -> Self {
        Self {
            kind,
            prev_streak: 0,
            prev_in_lockout: false,
            anim_remaining: 0.0,
            anim_total: 0.0,
            anim: StreakAnim::None,
            shattered_value: 0,
        }
    }
}

/// The main streak digit. Separate marker so fragment queries don't match it
/// (both share the same `Text` component type on the counter subtree).
#[derive(Component)]
pub struct StreakMainText;

/// One shard of the shatter animation. Indices 0..NUM_FRAGMENTS drive a fixed
/// velocity/rotation pattern so shards spread in distinct directions.
#[derive(Component)]
pub struct StreakFragment {
    pub index: u8,
    pub parent_counter: Entity,
}

const NUM_FRAGMENTS: u8 = 4;

/// Nadir-zone commit: green flash with a firm pop.
const INCREMENT_SECS: f32 = 0.28;
/// Mid-zone commit: soft white pop, no color shift.
const NEUTRAL_SECS: f32 = 0.14;
/// Shatter animation length. Long enough for the shards to clearly fly out
/// and fade; short enough not to linger after the streak is gone.
const BREAK_SECS: f32 = 0.55;
const POP_PEAK_SCALE: f32 = 1.4;
/// Softer pop peak used by the neutral (mid-zone) commit animation.
const NEUTRAL_POP_PEAK_SCALE: f32 = 1.14;
const FONT_SIZE: f32 = 28.0;
/// Font size of shatter fragments — smaller than the main digit so they read
/// as shards rather than duplicate numbers.
const FRAGMENT_FONT_SIZE: f32 = FONT_SIZE * 0.7;
/// Gravity applied to shatter fragments (px/s² in UI coords — y grows downward).
const FRAGMENT_GRAVITY_PX_S2: f32 = 520.0;

/// Y offset of the Tank/Heal counter. Sits inside the bottom-anchored cube
/// overlay (y = 80..290) but above the arc's nadir (y ≈ 165) so the streak
/// number reads against the cube's rotation-gradient backdrop.
const TANK_HEAL_COUNTER_TOP_PX: f32 = 96.0;
/// Y offset of the DPS counters. Sits in the upper portion of the panel, inside
/// the envelope formed by the two tilted arcs (which converge near the top
/// center) and above the central ghost stack (first ghost at y ≈ 104).
const DPS_COUNTER_TOP_PX: f32 = 46.0;
const COUNTER_WIDTH_PX: f32 = 80.0;
/// Horizontal center of each counter as a fraction of panel width.
/// Tank/Heal: centered on the single arc.
/// DPS counters sit inside the neck area between the two tilted arcs —
/// pulled toward the midline so they're nestled between the arcs rather
/// than floating over the outer ends.
const TANK_HEAL_CENTER_PCT: f32 = 50.0;
const DPS_PRIMARY_CENTER_PCT: f32 = 66.0;
const DPS_SECONDARY_CENTER_PCT: f32 = 33.0;

const GREY_IDLE: Color = Color::srgba(0.50, 0.52, 0.58, 0.85);
const WHITE_LIVE: Color = Color::srgba(1.0, 1.0, 1.0, 0.95);
/// Shatter fragment color — a hot red-orange that fades its alpha during the break.
const FRAGMENT_COLOR: Color = Color::srgba(1.00, 0.30, 0.18, 1.0);

/// Spawn three counter nodes as children of `MinigameRoot`. Each counter has
/// one main text digit and `NUM_FRAGMENTS` shatter shards (hidden at rest).
pub fn spawn_streak_counters(mut commands: Commands, root_q: Query<Entity, With<MinigameRoot>>) {
    let Ok(root) = root_q.single() else { return };

    for (kind, center_pct, top_px) in [
        (StreakCounterKind::TankHeal, TANK_HEAL_CENTER_PCT, TANK_HEAL_COUNTER_TOP_PX),
        (StreakCounterKind::DpsPrimary, DPS_PRIMARY_CENTER_PCT, DPS_COUNTER_TOP_PX),
        (StreakCounterKind::DpsSecondary, DPS_SECONDARY_CENTER_PCT, DPS_COUNTER_TOP_PX),
    ] {
        let counter_entity = commands
            .spawn((
                StreakCounter::new(kind),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(top_px),
                    left: Val::Percent(center_pct),
                    width: Val::Px(COUNTER_WIDTH_PX),
                    margin: UiRect::left(Val::Px(-COUNTER_WIDTH_PX * 0.5)),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Visibility::Hidden,
                UiTransform {
                    translation: Val2::ZERO,
                    scale: Vec2::ONE,
                    rotation: Rot2::IDENTITY,
                },
            ))
            .id();

        commands.entity(root).add_child(counter_entity);

        commands.entity(counter_entity).with_children(|wrap| {
            wrap.spawn((
                StreakMainText,
                Text::new("0"),
                TextFont {
                    font_size: FONT_SIZE,
                    ..default()
                },
                TextColor(GREY_IDLE),
            ));
            for i in 0..NUM_FRAGMENTS {
                wrap.spawn((
                    StreakFragment {
                        index: i,
                        parent_counter: counter_entity,
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(50.0),
                        top: Val::Px(0.0),
                        margin: UiRect::left(Val::Px(-FRAGMENT_FONT_SIZE * 0.3)),
                        ..default()
                    },
                    Text::new("0"),
                    TextFont {
                        font_size: FRAGMENT_FONT_SIZE,
                        ..default()
                    },
                    TextColor(Color::srgba(
                        FRAGMENT_COLOR.to_srgba().red,
                        FRAGMENT_COLOR.to_srgba().green,
                        FRAGMENT_COLOR.to_srgba().blue,
                        0.0,
                    )),
                    UiTransform {
                        translation: Val2::ZERO,
                        scale: Vec2::ONE,
                        rotation: Rot2::IDENTITY,
                    },
                    Visibility::Hidden,
                ));
            }
        });
    }
}

/// Drive each counter from the local player's arc state. Reads `ArcState.streak`
/// (for `TankHeal` / `DpsPrimary`) or `SecondaryArcState.0.streak` (for
/// `DpsSecondary`); fires pop / green-flash / shatter on transitions.
pub fn render_streak_counters(
    time: Res<Time>,
    local_id: Res<LocalClientId>,
    own_entity: Option<Res<OwnServerEntity>>,
    server_q: Query<(&CombatState, Option<&ArcState>, Option<&SecondaryArcState>)>,
    mut counter_q: Query<(&mut StreakCounter, &mut Visibility, &mut UiTransform)>,
    mut main_text_q: Query<
        (&mut Text, &mut TextColor, &mut Visibility, &ChildOf),
        (With<StreakMainText>, Without<StreakCounter>),
    >,
) {
    let _ = local_id;

    let Some(own) = own_entity else {
        for (_, mut vis, _) in counter_q.iter_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    };
    let Ok((combat, arc_opt, secondary_opt)) = server_q.get(own.0) else {
        for (_, mut vis, _) in counter_q.iter_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    };

    let dt = time.delta_secs();
    let stance = combat.active_stance;

    for (mut counter, mut vis, mut xform) in counter_q.iter_mut() {
        // Pull the streak + lockout bits for this counter's arc. `None` if the
        // stance doesn't match or the required component is missing.
        let (stance_match, streak_opt, lockout_opt) = match counter.kind {
            StreakCounterKind::TankHeal => {
                let show = matches!(stance, Some(RoleStance::Tank) | Some(RoleStance::Heal));
                let (s, l) = if show {
                    (arc_opt.map(|a| a.streak), arc_opt.map(|a| a.in_lockout))
                } else {
                    (None, None)
                };
                (show && s.is_some(), s, l)
            }
            StreakCounterKind::DpsPrimary => {
                let show = stance == Some(RoleStance::Dps);
                let (s, l) = if show {
                    (arc_opt.map(|a| a.streak), arc_opt.map(|a| a.in_lockout))
                } else {
                    (None, None)
                };
                (show && s.is_some(), s, l)
            }
            StreakCounterKind::DpsSecondary => {
                let show = stance == Some(RoleStance::Dps);
                let (s, l) = if show {
                    (
                        secondary_opt.map(|s| s.0.streak),
                        secondary_opt.map(|s| s.0.in_lockout),
                    )
                } else {
                    (None, None)
                };
                (show && s.is_some(), s, l)
            }
        };

        if !stance_match {
            *vis = Visibility::Hidden;
            // Reset baselines so re-entering the stance doesn't misfire an
            // animation based on pre-exit state.
            counter.prev_streak = streak_opt.unwrap_or(0);
            counter.prev_in_lockout = lockout_opt.unwrap_or(false);
            counter.anim = StreakAnim::None;
            counter.anim_remaining = 0.0;
            continue;
        }

        let streak = streak_opt.unwrap_or(0);
        let in_lockout = lockout_opt.unwrap_or(false);
        let commit_edge = in_lockout && !counter.prev_in_lockout;

        // ── Pick an animation ────────────────────────────────────────────────
        // Priority: break > increment > neutral. A break trumps an incoming
        // nadir commit (e.g. if the server processes an apex commit that both
        // pushes a lockout edge AND zeroes the streak in the same tick).
        if streak == 0 && counter.prev_streak > 0 {
            counter.anim = StreakAnim::Break;
            counter.anim_total = BREAK_SECS;
            counter.anim_remaining = BREAK_SECS;
            counter.shattered_value = counter.prev_streak;
        } else if commit_edge {
            if streak > counter.prev_streak {
                counter.anim = StreakAnim::Increment;
                counter.anim_total = INCREMENT_SECS;
                counter.anim_remaining = INCREMENT_SECS;
            } else {
                // Commit landed but streak didn't change (mid-zone commit).
                counter.anim = StreakAnim::Neutral;
                counter.anim_total = NEUTRAL_SECS;
                counter.anim_remaining = NEUTRAL_SECS;
            }
        }
        counter.prev_streak = streak;
        counter.prev_in_lockout = in_lockout;

        // ── Animation advance ────────────────────────────────────────────────
        if counter.anim_remaining > 0.0 {
            counter.anim_remaining = (counter.anim_remaining - dt).max(0.0);
            if counter.anim_remaining == 0.0 {
                counter.anim = StreakAnim::None;
            }
        }

        // ── Parent node visibility + scale ───────────────────────────────────
        // Show the counter while there's a live streak OR while the shatter is
        // still playing (so the fragments can animate out). Otherwise hide.
        let should_show = streak > 0 || counter.anim == StreakAnim::Break;
        *vis = if should_show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        let t = if counter.anim_total > 0.0 {
            1.0 - (counter.anim_remaining / counter.anim_total).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let scale = match counter.anim {
            StreakAnim::Increment => pop_scale(t, POP_PEAK_SCALE),
            StreakAnim::Neutral => pop_scale(t, NEUTRAL_POP_PEAK_SCALE),
            StreakAnim::Break | StreakAnim::None => 1.0,
        };
        xform.scale = Vec2::splat(scale);
    }

    // ── Main digit text ──────────────────────────────────────────────────────
    // Updated in a second pass using ChildOf to find the owning counter. During
    // the Break animation the main digit is hidden (fragments take over);
    // otherwise it shows the current streak in the state-appropriate color.
    for (mut text, mut color, mut mvis, child_of) in main_text_q.iter_mut() {
        let Ok((counter, _, _)) = counter_q.get(child_of.parent()) else {
            continue;
        };
        let streak = counter.prev_streak;
        let anim_t = if counter.anim_total > 0.0 {
            1.0 - (counter.anim_remaining / counter.anim_total).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let (text_color, visible) = match counter.anim {
            // Shatter animation: hide main digit, fragments take over.
            StreakAnim::Break => (WHITE_LIVE, false),
            // Nadir commit: flash green, fade back to white over the animation.
            StreakAnim::Increment => (mix_colors(NADIR_GREEN, WHITE_LIVE, anim_t), true),
            // Mid-zone commit: stays white — the gentle pop carries the feedback.
            StreakAnim::Neutral => (WHITE_LIVE, true),
            StreakAnim::None => {
                let c = if streak == 0 { GREY_IDLE } else { WHITE_LIVE };
                (c, streak > 0)
            }
        };

        *mvis = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        let new_str = streak.to_string();
        if text.0 != new_str {
            text.0 = new_str;
        }
        *color = TextColor(text_color);
    }
}

/// Drive shatter fragments. Only visible during `StreakAnim::Break`. Each shard
/// uses a fixed velocity + rotation pattern keyed off its index; gravity
/// arcs them downward; alpha fades to 0 as the break animation progresses.
pub fn render_streak_fragments(
    counter_q: Query<&StreakCounter>,
    mut fragment_q: Query<(
        &StreakFragment,
        &mut Visibility,
        &mut UiTransform,
        &mut Text,
        &mut TextColor,
    )>,
) {
    for (frag, mut vis, mut xform, mut text, mut color) in fragment_q.iter_mut() {
        let Ok(counter) = counter_q.get(frag.parent_counter) else {
            *vis = Visibility::Hidden;
            continue;
        };

        if counter.anim != StreakAnim::Break {
            *vis = Visibility::Hidden;
            continue;
        }

        *vis = Visibility::Inherited;

        // Time in seconds since the shatter started.
        let sim_t = (counter.anim_total - counter.anim_remaining).max(0.0);
        let progress = if counter.anim_total > 0.0 {
            (sim_t / counter.anim_total).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let (vx, vy, vrot) = fragment_velocity(frag.index);
        let x = vx * sim_t;
        let y = vy * sim_t + 0.5 * FRAGMENT_GRAVITY_PX_S2 * sim_t * sim_t;
        let rot = vrot * sim_t;

        xform.translation = Val2::new(Val::Px(x), Val::Px(y));
        xform.rotation = Rot2::radians(rot);

        let alpha = (1.0 - progress).clamp(0.0, 1.0);
        let base = FRAGMENT_COLOR.to_srgba();
        *color = TextColor(Color::srgba(base.red, base.green, base.blue, alpha));

        // Each shard shows the number it broke from so the shatter reads as
        // fragments of the previous streak value rather than arbitrary debris.
        let new_str = counter.shattered_value.to_string();
        if text.0 != new_str {
            text.0 = new_str;
        }
    }
}

/// Per-index velocity + rotation seed for the four shatter shards. Indices map
/// to four quadrants so the shards spread outward; gravity pulls them down.
fn fragment_velocity(index: u8) -> (f32, f32, f32) {
    // (vx px/s, vy px/s, vrot rad/s). UI y grows downward.
    match index {
        0 => (110.0, -140.0, 7.5),   // up-right, CW
        1 => (-110.0, -140.0, -7.5), // up-left, CCW
        2 => (85.0, -40.0, 5.0),     // right, gentle CW
        3 => (-85.0, -40.0, -5.0),   // left, gentle CCW
        _ => (0.0, 0.0, 0.0),
    }
}

/// Ease-out pop: 1.0 → `peak` at t=0.5 → 1.0.
fn pop_scale(t: f32, peak: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    // Parabolic envelope: 4 * t * (1 - t) peaks at 1.0 when t = 0.5.
    let env = 4.0 * t * (1.0 - t);
    1.0 + (peak - 1.0) * env
}

/// Lerp between two sRGB colors at factor `t` ∈ [0, 1].
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
