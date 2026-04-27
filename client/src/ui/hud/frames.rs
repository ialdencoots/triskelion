use bevy::prelude::*;

use shared::components::combat::{Health, ReplicatedMitigationPool};
use shared::components::enemy::EnemyName;
use shared::components::player::PlayerName;

use crate::ui::theme;
use crate::world::players::{OwnServerEntity, RemotePlayerMarker};
use crate::world::selection::SelectedTarget;

use super::health_bar;

pub const FRAME_W: f32 = 220.0;
pub const FRAME_H: f32 = 56.0;
const AVATAR_SIZE: f32 = 48.0;
/// Horizontal gap from screen center to the near edge of each frame.
pub const CHAR_OFFSET: f32 = 80.0;
/// Vertical position: percentage of window height where the frame top sits.
/// ~55 % places the frames slightly below center, near where the player appears.
pub const FRAME_TOP_PCT: f32 = 55.0;

// ── Marker components ───────────────────────────────────────────────────────

#[derive(Component)]
pub struct PlayerFrameRoot;

#[derive(Component)]
pub struct TargetFrameRoot;

/// Marks the health fill inside the *player* frame.
#[derive(Component)]
pub struct PlayerHealthFill;

/// Marks the health fill inside the *target* frame.
#[derive(Component)]
pub struct TargetHealthFill;

/// Marks the name text inside the *target* frame.
#[derive(Component)]
pub struct TargetNameText;

/// Marks the name text inside the *player* frame so it can be driven by the
/// local player's actual `PlayerName`.
#[derive(Component)]
pub struct PlayerNameText;

/// Marks the mitigation-pool fill inside the *player* frame.
#[derive(Component)]
pub struct PlayerMitigationFill;

/// Soft cap used to normalize the mitigation pool bar — the pool is uncapped
/// in gameplay terms, but the visual needs a reference value. Each full bar
/// "tank" represents this much pool; overflow past it adds tier markers
/// below.
const MITIGATION_DISPLAY_MAX: f32 = 100.0;
/// Bar fill color when the pool is normal. Cool teal so it reads distinctly
/// from the green health bar.
const MITIGATION_FILL: Color = Color::srgb(0.20, 0.65, 0.85);
/// Bar fill color while invuln is active. Gold for a "shielded" feel.
const MITIGATION_INVULN_FILL: Color = Color::srgb(0.95, 0.80, 0.20);
/// Background for the empty portion of the pool bar.
const MITIGATION_BAR_BG: Color = Color::srgba(0.05, 0.10, 0.15, 0.8);
/// Side length of one tier marker.
const MITIGATION_MARKER_SIZE: f32 = 5.0;
/// Horizontal gap between adjacent tier markers.
const MITIGATION_MARKER_GAP: f32 = 3.0;
/// Display cap — at most this many markers ever appear. The pool itself is
/// uncapped, but the visual stops growing past this point.
const MITIGATION_MAX_MARKERS: usize = 8;

/// Marks the avatar background inside the *target* frame so its color can be
/// updated to match the selected entity's group-frame avatar.
#[derive(Component)]
pub struct TargetAvatarBg;

// ── Spawn ────────────────────────────────────────────────────────────────────

pub fn spawn_frames(mut commands: Commands) {
    commands.spawn((PlayerFrameRoot, frame_node(-(FRAME_W + CHAR_OFFSET)), Visibility::Inherited))
        .with_children(|p| {
            spawn_frame_contents(p, theme::AVATAR_SELF, "Player", false);
        });

    commands.spawn((TargetFrameRoot, frame_node(CHAR_OFFSET), Visibility::Hidden))
        .with_children(|p| {
            spawn_frame_contents(p, theme::AVATAR_ENEMY, "", true);
        });
}

/// Builds a frame node centered on screen.
/// `margin_left_px` shifts the left edge relative to the horizontal midpoint.
fn frame_node(margin_left_px: f32) -> impl Bundle {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        top: Val::Percent(FRAME_TOP_PCT),
        margin: UiRect::left(Val::Px(margin_left_px)),
        width: Val::Px(FRAME_W),
        height: Val::Px(FRAME_H),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(6.0),
        padding: UiRect::all(Val::Px(4.0)),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

fn spawn_frame_contents(parent: &mut ChildSpawnerCommands, accent: Color, name: &str, is_target: bool) {
    let mut avatar_cmd = parent.spawn((
        Node {
            width: Val::Px(AVATAR_SIZE),
            height: Val::Px(AVATAR_SIZE),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(accent),
        theme::uniform_border(),
    ));
    if is_target {
        avatar_cmd.insert(TargetAvatarBg);
    }

    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                row_gap: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG),
            theme::uniform_border(),
        ))
        .with_children(|col| {
            let mut text_cmd = col.spawn((
                Text::new(name),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
            ));
            if is_target {
                text_cmd.insert(TargetNameText);
            } else {
                // Player frame — name is driven dynamically from PlayerName.
                text_cmd.insert(PlayerNameText);
            }

            col.spawn(health_bar::bar_bundle(10.0))
                .with_children(|bar| {
                    let mut fill_cmd = bar.spawn(health_bar::fill_bundle());
                    if is_target {
                        fill_cmd.insert(TargetHealthFill);
                    } else {
                        fill_cmd.insert(PlayerHealthFill);
                    }
                });

            // Player frame only: a thin secondary bar below health for the
            // healer's mitigation pool. Hidden by default; the update system
            // shows it whenever the pool has any amount or invuln is active.
            if !is_target {
                col.spawn((
                    health_bar::bar_bundle_styled(6.0, MITIGATION_BAR_BG),
                    Visibility::Hidden,
                    PlayerMitigationBar,
                ))
                    .with_children(|bar| {
                        bar.spawn((
                            health_bar::fill_bundle_styled(MITIGATION_FILL),
                            PlayerMitigationFill,
                        ));
                    });
                // Tier markers — each one represents a full bar's worth of
                // accumulated pool. Container is empty until the first
                // overflow lands, then markers are spawned/despawned by
                // `update_player_mitigation_markers`.
                col.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(MITIGATION_MARKER_GAP),
                        ..default()
                    },
                    PlayerMitigationMarkers,
                ));
            }
        });
}

/// Marks the bar background so we can toggle visibility based on whether
/// the local player has any pool or active invuln.
#[derive(Component)]
pub struct PlayerMitigationBar;

/// Marks the container row that holds tier markers below the mitigation bar.
#[derive(Component)]
pub struct PlayerMitigationMarkers;

// ── Systems ───────────────────────────────────────────────────────────────────

/// Drives the player frame's name text from the local player's `PlayerName`.
/// Runs every frame but only writes when the text differs, so it is effectively
/// free after the name arrives.
pub fn update_player_name(
    own_entity: Option<Res<OwnServerEntity>>,
    player_names: Query<&PlayerName>,
    mut text_q: Query<&mut Text, With<PlayerNameText>>,
) {
    let Some(own) = own_entity else { return };
    let Ok(name) = player_names.get(own.0) else { return };
    let Ok(mut text) = text_q.single_mut() else { return };
    if text.0 != name.0 {
        text.0 = name.0.clone();
    }
}

/// Recolors the target frame's avatar to match the selected entity's
/// group-frame avatar:
///   • own player  → green   (matches the self row in the group frame)
///   • party member → blue   (matches remote-player rows)
///   • enemy / other → red   (the original target-frame accent)
pub fn update_target_avatar(
    selected: Res<SelectedTarget>,
    own_entity: Option<Res<OwnServerEntity>>,
    remote_q: Query<(), With<RemotePlayerMarker>>,
    mut avatar_q: Query<&mut BackgroundColor, With<TargetAvatarBg>>,
) {
    if !selected.is_changed() { return; }
    let Ok(mut bg) = avatar_q.single_mut() else { return };
    bg.0 = match selected.0 {
        None => theme::AVATAR_ENEMY,
        Some(e) if own_entity.as_ref().map(|r| r.0 == e).unwrap_or(false) => theme::AVATAR_SELF,
        Some(e) if remote_q.contains(e) => theme::AVATAR_PARTY,
        _ => theme::AVATAR_ENEMY,
    };
}

/// Updates the name text inside the target frame when the selection changes.
/// Checks `EnemyName` first, then `PlayerName` as a fallback.
pub fn update_target_name(
    selected: Res<SelectedTarget>,
    enemy_names: Query<&EnemyName>,
    player_names: Query<&PlayerName>,
    mut name_text_q: Query<&mut Text, With<TargetNameText>>,
) {
    if !selected.is_changed() {
        return;
    }
    let Ok(mut text) = name_text_q.single_mut() else { return };
    text.0 = match selected.0 {
        Some(e) => {
            if let Ok(n) = enemy_names.get(e) {
                n.0.clone()
            } else if let Ok(n) = player_names.get(e) {
                n.0.clone()
            } else {
                String::new()
            }
        }
        None => String::new(),
    };
}

/// Shows the target frame when something is selected; hides it otherwise.
pub fn update_target_frame_visibility(
    selected: Res<SelectedTarget>,
    mut target_frame_q: Query<&mut Visibility, With<TargetFrameRoot>>,
) {
    if !selected.is_changed() { return; }
    let Ok(mut vis) = target_frame_q.single_mut() else { return };
    *vis = if selected.0.is_some() { Visibility::Inherited } else { Visibility::Hidden };
}

/// Drives the target frame's health fill width from the selected entity's `Health`.
/// Falls back to 100% when the entity has no `Health` (e.g. enemies without HP tracking).
pub fn update_target_health_fill(
    selected: Res<SelectedTarget>,
    health_query: Query<&Health>,
    mut fill_query: Query<&mut Node, With<TargetHealthFill>>,
) {
    let Ok(mut node) = fill_query.single_mut() else { return };

    let pct = match selected.0 {
        Some(e) => health_query.get(e).map(health_bar::percent).unwrap_or(100.0),
        None => 100.0,
    };

    node.width = Val::Percent(pct);
}

/// Drives the player frame's health fill width from the local player's `Health`.
pub fn update_player_health_fill(
    own_entity: Option<Res<OwnServerEntity>>,
    health_query: Query<&Health>,
    mut fill_query: Query<&mut Node, With<PlayerHealthFill>>,
) {
    let Ok(mut node) = fill_query.single_mut() else { return };
    let pct = own_entity
        .as_ref()
        .and_then(|own| health_query.get(own.0).ok())
        .map(health_bar::percent)
        .unwrap_or(100.0);
    node.width = Val::Percent(pct);
}

/// Drives the player frame's mitigation pool bar from the local player's
/// `ReplicatedMitigationPool`. The bar shows only the *overflow* past the
/// current tier — full bars wrap around to 0% and add a tier marker below
/// (see `update_player_mitigation_markers`). The bar is hidden when the
/// pool is empty and invuln is inactive.
pub fn update_player_mitigation_bar(
    own_entity: Option<Res<OwnServerEntity>>,
    pool_query: Query<&ReplicatedMitigationPool>,
    mut fill_query: Query<
        (&mut Node, &mut BackgroundColor),
        (With<PlayerMitigationFill>, Without<PlayerMitigationBar>),
    >,
    mut bar_query: Query<&mut Visibility, With<PlayerMitigationBar>>,
) {
    let pool = own_entity
        .as_ref()
        .and_then(|own| pool_query.get(own.0).ok());

    let Ok((mut fill_node, mut fill_color)) = fill_query.single_mut() else { return };
    let Ok(mut vis) = bar_query.single_mut() else { return };

    match pool {
        Some(p) if p.amount > 0.0 || p.invuln_active => {
            *vis = Visibility::Inherited;
            let pct = if p.invuln_active {
                100.0
            } else {
                // Show only the overflow above the current tier — each full
                // bar resets to 0% and is recorded as a marker below.
                let overflow = p.amount.rem_euclid(MITIGATION_DISPLAY_MAX);
                (overflow / MITIGATION_DISPLAY_MAX * 100.0).clamp(0.0, 100.0)
            };
            fill_node.width = Val::Percent(pct);
            let want = if p.invuln_active { MITIGATION_INVULN_FILL } else { MITIGATION_FILL };
            if fill_color.0 != want {
                fill_color.0 = want;
            }
        }
        _ => {
            *vis = Visibility::Hidden;
        }
    }
}

/// Spawns / despawns tier markers below the mitigation bar to reflect the
/// number of full `MITIGATION_DISPLAY_MAX` chunks currently in the pool. A
/// pool of `2.5 × MITIGATION_DISPLAY_MAX` shows 2 markers below; the bar
/// itself shows the trailing 0.5 as 50% fill.
pub fn update_player_mitigation_markers(
    mut commands: Commands,
    own_entity: Option<Res<OwnServerEntity>>,
    pool_query: Query<&ReplicatedMitigationPool>,
    container_query: Query<(Entity, Option<&Children>), With<PlayerMitigationMarkers>>,
) {
    let Ok((container, children)) = container_query.single() else { return };

    let target = own_entity
        .as_ref()
        .and_then(|own| pool_query.get(own.0).ok())
        .map(|p| ((p.amount / MITIGATION_DISPLAY_MAX) as usize).min(MITIGATION_MAX_MARKERS))
        .unwrap_or(0);

    let current = children.map(|c| c.len()).unwrap_or(0);
    if target > current {
        for _ in current..target {
            let marker = commands
                .spawn((
                    Node {
                        width: Val::Px(MITIGATION_MARKER_SIZE),
                        height: Val::Px(MITIGATION_MARKER_SIZE),
                        ..default()
                    },
                    BackgroundColor(MITIGATION_FILL),
                ))
                .id();
            commands.entity(container).add_child(marker);
        }
    } else if target < current {
        if let Some(children) = children {
            // Despawn the trailing markers first — visually unwinds from the
            // right as the pool drops past each tier boundary.
            for child in children.iter().rev().take(current - target) {
                commands.entity(child).despawn();
            }
        }
    }
}
