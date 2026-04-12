use bevy::prelude::*;

use crate::world::camera::OrbitCamera;
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
    // Both frames start at (0,0) and are moved every frame by the anchor system.
    commands.spawn((PlayerFrameRoot, frame_node())).with_children(|p| {
        spawn_frame_contents(p, Color::srgb(0.25, 0.55, 0.25)); // green tint for self
    });

    commands.spawn((TargetFrameRoot, frame_node())).with_children(|p| {
        spawn_frame_contents(p, Color::srgb(0.65, 0.20, 0.20)); // red tint for target
    });
}

fn frame_node() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(-9999.0), // hidden until first anchor update
            top: Val::Px(0.0),
            width: Val::Px(FRAME_W),
            height: Val::Px(FRAME_H),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(4.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.85)),
        BorderColor {
            top: Color::srgba(0.4, 0.4, 0.5, 0.6),
            bottom: Color::srgba(0.4, 0.4, 0.5, 0.6),
            left: Color::srgba(0.4, 0.4, 0.5, 0.6),
            right: Color::srgba(0.4, 0.4, 0.5, 0.6),
        },
    )
}

fn spawn_frame_contents(parent: &mut ChildSpawnerCommands, accent: Color) {
    // Avatar square
    parent.spawn((
        Node {
            width: Val::Px(AVATAR_SIZE),
            height: Val::Px(AVATAR_SIZE),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(accent.with_alpha(0.5)),
    ));

    // Health bar column
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            row_gap: Val::Px(3.0),
            ..default()
        })
        .with_children(|col| {
            // Name placeholder
            col.spawn((
                Text::new("Player"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
            ));

            // HP bar container
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
                // Fill — width driven by health percentage
                bar.spawn((
                    HealthFill,
                    Node {
                        width: Val::Percent(100.0), // placeholder: full health
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.20, 0.72, 0.20)),
                ));
            });
        });
}

// ── Anchor system ─────────────────────────────────────────────────────────────

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
        node.top = Val::Px(screen.y - FRAME_H / 2.0);
    }

    if let Ok(mut node) = target_frame_q.single_mut() {
        node.left = Val::Px(screen.x + CHAR_OFFSET);
        node.top = Val::Px(screen.y - FRAME_H / 2.0);
    }
}
