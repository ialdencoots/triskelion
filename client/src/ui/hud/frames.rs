use bevy::prelude::*;

use shared::components::combat::Health;
use shared::components::enemy::EnemyName;
use shared::components::player::PlayerName;

use crate::world::players::{OwnServerEntity, RemotePlayerMarker};
use crate::world::selection::SelectedTarget;

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

/// Marker for the green fill inside a health bar.
#[derive(Component)]
pub struct HealthFill;

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

/// Marks the avatar background inside the *target* frame so its color can be
/// updated to match the selected entity's group-frame avatar.
#[derive(Component)]
pub struct TargetAvatarBg;

// ── Spawn ────────────────────────────────────────────────────────────────────

pub fn spawn_frames(mut commands: Commands) {
    // Player frame: left side of screen center, horizontally offset by CHAR_OFFSET + FRAME_W.
    commands.spawn((PlayerFrameRoot, frame_node(-(FRAME_W + CHAR_OFFSET)), Visibility::Inherited))
        .with_children(|p| {
            spawn_frame_contents(p, Color::srgb(0.25, 0.55, 0.25), "Player", false);
        });

    // Target frame: right side of screen center, offset by CHAR_OFFSET.
    commands.spawn((TargetFrameRoot, frame_node(CHAR_OFFSET), Visibility::Hidden))
        .with_children(|p| {
            spawn_frame_contents(p, Color::srgb(0.65, 0.20, 0.20), "", true);
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
    // Avatar square — tagged with TargetAvatarBg on the target frame so the
    // update system can recolor it to match the selected entity's group-frame avatar.
    let mut avatar_cmd = parent.spawn((
        Node {
            width: Val::Px(AVATAR_SIZE),
            height: Val::Px(AVATAR_SIZE),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(accent.with_alpha(0.5)),
        BorderColor {
            top:    Color::srgba(0.4, 0.4, 0.5, 0.6),
            bottom: Color::srgba(0.4, 0.4, 0.5, 0.6),
            left:   Color::srgba(0.4, 0.4, 0.5, 0.6),
            right:  Color::srgba(0.4, 0.4, 0.5, 0.6),
        },
    ));
    if is_target {
        avatar_cmd.insert(TargetAvatarBg);
    }

    // Health bar column
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                row_gap: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.85)),
            BorderColor {
                top:    Color::srgba(0.4, 0.4, 0.5, 0.6),
                bottom: Color::srgba(0.4, 0.4, 0.5, 0.6),
                left:   Color::srgba(0.4, 0.4, 0.5, 0.6),
                right:  Color::srgba(0.4, 0.4, 0.5, 0.6),
            },
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

            col.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(10.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.15, 0.05, 0.05, 0.8)),
            ))
            .with_children(|bar| {
                let mut fill_cmd = bar.spawn((
                    HealthFill,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.20, 0.72, 0.20)),
                ));
                if is_target {
                    fill_cmd.insert(TargetHealthFill);
                }
            });
        });
}

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
        None => Color::srgba(0.65, 0.20, 0.20, 0.5),
        Some(e) if own_entity.as_ref().map(|r| r.0 == e).unwrap_or(false) => {
            Color::srgb(0.25, 0.55, 0.25).with_alpha(0.5) // self = green
        }
        Some(e) if remote_q.contains(e) => {
            Color::srgb(0.20, 0.40, 0.60).with_alpha(0.5) // party member = blue
        }
        _ => Color::srgb(0.65, 0.20, 0.20).with_alpha(0.5), // enemy = red
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
        Some(e) => health_query
            .get(e)
            .map(|h| (h.current / h.max * 100.0).clamp(0.0, 100.0))
            .unwrap_or(100.0),
        None => 100.0,
    };

    node.width = Val::Percent(pct);
}
