use bevy::prelude::*;

use crate::world::camera::OrbitCamera;
use crate::world::terrain::PlayerMarker;

const MINIGAME_W: f32 = 420.0;
const MINIGAME_H: f32 = 180.0;
/// Distance above the action bar (slot height 64 + bottom pad 18 + gap 10).
const ABOVE_ACTION_BAR: f32 = 64.0 + 18.0 + 10.0;

#[derive(Component)]
pub struct MinigameRoot;

/// Spawned lazily by `anchor_minigame_to_character` the first time the
/// character has a valid screen position, so it never flashes at (0,0).
pub fn anchor_minigame_to_character(
    mut commands: Commands,
    player_query: Query<&GlobalTransform, With<PlayerMarker>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    mut root_query: Query<(Entity, &mut Node), With<MinigameRoot>>,
) {
    let Ok(player_tf) = player_query.single() else { return };
    let Ok((camera, cam_tf)) = camera_query.single() else { return };
    let Ok(screen) = camera.world_to_viewport(cam_tf, player_tf.translation()) else { return };

    let left = screen.x - MINIGAME_W / 2.0;

    if let Ok((_entity, mut node)) = root_query.single_mut() {
        node.left = Val::Px(left);
    } else {
        // First time we have a valid screen position — spawn the container.
        commands.spawn((
            MinigameRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(left),
                bottom: Val::Px(ABOVE_ACTION_BAR),
                width: Val::Px(MINIGAME_W),
                height: Val::Px(MINIGAME_H),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.03, 0.08, 0.75)),
            BorderColor {
                top:    Color::srgba(0.35, 0.35, 0.50, 0.5),
                bottom: Color::srgba(0.35, 0.35, 0.50, 0.5),
                left:   Color::srgba(0.35, 0.35, 0.50, 0.5),
                right:  Color::srgba(0.35, 0.35, 0.50, 0.5),
            },
        ));
    }
}
