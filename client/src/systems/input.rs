use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::inputs::{AbilityInput, MinigameInput, PlayerInput};
use shared::terrain;

use crate::world::camera::OrbitState;
use crate::world::terrain::PlayerMarker;

/// Reads keyboard/gamepad state each frame, constructs a `PlayerInput`, and
/// sends it to the server as a Lightyear message.
///
/// The movement vector encodes world-space XZ as (x, -z) so the server can
/// reconstruct the 3-D direction with `Vec3::new(m.x, 0, -m.y)`.
///
/// `y` and `vy` carry the client physics body's vertical position and velocity
/// so the server can relay jumps to all other connected clients.
pub fn gather_and_send_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    orbit: Res<OrbitState>,
    player_query: Query<(&Transform, &LinearVelocity), With<PlayerMarker>>,
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

    // TnuaController floats the physics capsule at ~1.95 m above the terrain using
    // a spring-damper, which produces small Y oscillations even at rest.  Relaying
    // those oscillations to other clients causes visible up/down jitter.
    //
    // Fix: only send the actual physics Y when the player is clearly airborne.
    // Detection combines two signals so the full jump arc is captured:
    //   • |vy| > 1.0 m/s  — catches takeoff (vy ≈ +5.4) and landing (vy negative)
    //   • height_above > 2.2 m — catches the apex where vy ≈ 0 but we're clearly up
    // The physics resting height is ~1.95 m above the terrain noise floor, so the
    // 2.2 m threshold only fires once the player is genuinely above the ground.
    // When grounded, we send the canonical terrain + 1.1 with vy = 0.
    let (y, vy) = player_query
        .single()
        .map(|(tf, lv)| {
            let terrain_y = terrain::height_at(tf.translation.x, tf.translation.z);
            let height_above = tf.translation.y - terrain_y;
            let is_airborne = lv.y.abs() > 1.0 || height_above > 2.2;
            if is_airborne {
                (tf.translation.y, lv.y)
            } else {
                (terrain_y + 1.1, 0.0)
            }
        })
        .unwrap_or((0.0, 0.0));

    sender.send::<GameChannel>(PlayerInput {
        movement: Vec2::new(move_3d.x, -move_3d.z),
        y,
        vy,
        abilities: AbilityInput::default(),
        minigame: MinigameInput::default(),
    });
}
