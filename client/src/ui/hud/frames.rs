use bevy::prelude::*;

use crate::world::camera::OrbitCamera;
use crate::world::selection::SelectedTarget;
use crate::world::terrain::PlayerMarker;

const FRAME_W: f32 = 220.0;
const FRAME_H: f32 = 56.0;
const AVATAR_SIZE: f32 = 48.0;
const CHAR_OFFSET: f32 = 80.0; // px gap between frame edge and character screen center

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
    commands.spawn((PlayerFrameRoot, frame_node(), Visibility::Inherited))
        .with_children(|p| {
            spawn_frame_contents(p, Color::srgb(0.25, 0.55, 0.25), "Player");
        });

    // Target frame starts hidden — shown only when an enemy is selected.
    commands.spawn((TargetFrameRoot, frame_node(), Visibility::Hidden))
        .with_children(|p| {
            spawn_frame_contents(p, Color::srgb(0.65, 0.20, 0.20), "");
        });
}

fn frame_node() -> impl Bundle {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        top: Val::Px(0.0),
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

pub fn anchor_frames_to_character(
    player_query: Query<&GlobalTransform, With<PlayerMarker>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    mut player_frame_q: Query<&mut Node, (With<PlayerFrameRoot>, Without<TargetFrameRoot>)>,
    mut target_frame_q: Query<&mut Node, (With<TargetFrameRoot>, Without<PlayerFrameRoot>)>,
) {
    let Ok(player_tf) = player_query.single() else { return };
    let Ok((camera, cam_tf)) = camera_query.single() else { return };
    let Ok(screen) = camera.world_to_viewport(cam_tf, player_tf.translation()) else { return };

    if let Ok(mut node) = player_frame_q.single_mut() {
        node.left = Val::Px(screen.x - FRAME_W - CHAR_OFFSET);
        node.top  = Val::Px(screen.y - FRAME_H / 2.0);
    }

    if let Ok(mut node) = target_frame_q.single_mut() {
        node.left = Val::Px(screen.x + CHAR_OFFSET);
        node.top  = Val::Px(screen.y - FRAME_H / 2.0);
    }
}

/// Shows the target frame when an enemy is selected; hides it otherwise.
pub fn update_target_frame_visibility(
    selected: Res<SelectedTarget>,
    mut target_frame_q: Query<&mut Visibility, With<TargetFrameRoot>>,
) {
    if !selected.is_changed() { return; }
    let Ok(mut vis) = target_frame_q.single_mut() else { return };
    *vis = if selected.0.is_some() { Visibility::Inherited } else { Visibility::Hidden };
}
