use bevy::prelude::*;

use crate::world::selection::SelectedTarget;

const FRAME_W: f32 = 220.0;
const FRAME_H: f32 = 56.0;
const AVATAR_SIZE: f32 = 48.0;
/// Horizontal gap from screen center to the near edge of each frame.
const CHAR_OFFSET: f32 = 80.0;
/// Vertical position: percentage of window height where the frame top sits.
/// ~55 % places the frames slightly below center, near where the player appears.
const FRAME_TOP_PCT: f32 = 55.0;

// ── Marker components ───────────────────────────────────────────────────────

#[derive(Component)]
pub struct PlayerFrameRoot;

#[derive(Component)]
pub struct TargetFrameRoot;

/// Marker for the green fill inside a health bar.
#[derive(Component)]
pub struct HealthFill;

// ── Spawn ────────────────────────────────────────────────────────────────────

pub fn spawn_frames(mut commands: Commands) {
    // Player frame: left side of screen center, horizontally offset by CHAR_OFFSET + FRAME_W.
    commands.spawn((PlayerFrameRoot, frame_node(-(FRAME_W + CHAR_OFFSET)), Visibility::Inherited))
        .with_children(|p| {
            spawn_frame_contents(p, Color::srgb(0.25, 0.55, 0.25), "Player");
        });

    // Target frame: right side of screen center, offset by CHAR_OFFSET.
    commands.spawn((TargetFrameRoot, frame_node(CHAR_OFFSET), Visibility::Hidden))
        .with_children(|p| {
            spawn_frame_contents(p, Color::srgb(0.65, 0.20, 0.20), "");
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

fn spawn_frame_contents(parent: &mut ChildSpawnerCommands, accent: Color, name: &str) {
    // Avatar square
    parent.spawn((
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
            col.spawn((
                Text::new(name),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
            ));

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
                bar.spawn((
                    HealthFill,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.20, 0.72, 0.20)),
                ));
            });
        });
}

// ── Systems ───────────────────────────────────────────────────────────────────

/// Shows the target frame when an enemy is selected; hides it otherwise.
pub fn update_target_frame_visibility(
    selected: Res<SelectedTarget>,
    mut target_frame_q: Query<&mut Visibility, With<TargetFrameRoot>>,
) {
    if !selected.is_changed() { return; }
    let Ok(mut vis) = target_frame_q.single_mut() else { return };
    *vis = if selected.0.is_some() { Visibility::Inherited } else { Visibility::Hidden };
}
