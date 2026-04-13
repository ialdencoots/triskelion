use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::inputs::{AbilityInput, MinigameInput, PlayerInput};

use crate::world::camera::OrbitState;

/// Reads keyboard/gamepad state each frame, constructs a `PlayerInput`, and
/// sends it to the server as a Lightyear message.
///
/// The movement vector encodes world-space XZ as (x, -z) so the server can
/// reconstruct the 3-D direction with `Vec3::new(m.x, 0, -m.y)`.
pub fn gather_and_send_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    orbit: Res<OrbitState>,
    mut sender_query: Query<&mut MessageSender<PlayerInput>>,
) {
    let Ok(mut sender) = sender_query.single_mut() else { return };

    let yaw_rot = Quat::from_rotation_y(orbit.yaw);
    let cam_forward = yaw_rot * Vec3::NEG_Z;
    let cam_right = yaw_rot * Vec3::X;

    let both_mouse =
        mouse_buttons.pressed(MouseButton::Left) && mouse_buttons.pressed(MouseButton::Right);

    let move_3d = if both_mouse {
        cam_forward
    } else {
        let mut dir = Vec3::ZERO;
        if keyboard.pressed(KeyCode::KeyE) { dir += cam_forward; }
        if keyboard.pressed(KeyCode::KeyD) { dir -= cam_forward; }
        if keyboard.pressed(KeyCode::KeyS) { dir -= cam_right; }
        if keyboard.pressed(KeyCode::KeyF) { dir += cam_right; }
        if dir.length_squared() > 0.0 { dir.normalize() } else { Vec3::ZERO }
    };

    sender.send::<GameChannel>(PlayerInput {
        movement: Vec2::new(move_3d.x, -move_3d.z),
        abilities: AbilityInput::default(),
        minigame: MinigameInput::default(),
    });
}
