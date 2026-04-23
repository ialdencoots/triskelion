use bevy::prelude::*;
use bevy_tnua::builtins::TnuaBuiltinWalk;
use bevy_tnua::TnuaController;

use super::camera::OrbitState;
use super::terrain::PlayerMarker;
use super::ControlScheme;

/// Build the normalized XZ move direction from WASD/EDSF + mouse chords.
///
/// `fwd_axis` / `right_axis` already encode whose frame movement is relative to
/// (camera when right-mouse is held, character facing otherwise) — the caller
/// picks one because Tnua drives rotation from `Transform` while the networked
/// input path uses the locally cached facing from the previous tick.
pub fn world_move_dir(
    keyboard: &ButtonInput<KeyCode>,
    both_mouse: bool,
    fwd_axis: Vec3,
    right_axis: Vec3,
) -> Vec3 {
    let dir = if both_mouse {
        // Auto-forward cancellable by D; S/F strafe.
        let forward = if keyboard.pressed(KeyCode::KeyD) { Vec3::ZERO } else { fwd_axis };
        let mut dir = forward;
        if keyboard.pressed(KeyCode::KeyS) { dir -= right_axis; }
        if keyboard.pressed(KeyCode::KeyF) { dir += right_axis; }
        dir
    } else {
        let mut dir = Vec3::ZERO;
        if keyboard.pressed(KeyCode::KeyE) { dir += fwd_axis; }
        if keyboard.pressed(KeyCode::KeyD) { dir -= fwd_axis; }
        if keyboard.pressed(KeyCode::KeyS) { dir -= right_axis; }
        if keyboard.pressed(KeyCode::KeyF) { dir += right_axis; }
        dir
    };
    if dir.length_squared() > 0.0 { dir.normalize() } else { Vec3::ZERO }
}

pub fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    orbit: Res<OrbitState>,
    mut player_query: Query<(&mut TnuaController<ControlScheme>, &mut Transform), With<PlayerMarker>>,
) {
    let Ok((mut controller, mut transform)) = player_query.single_mut() else { return };

    let right_mouse = mouse_buttons.pressed(MouseButton::Right);
    let both_mouse = mouse_buttons.pressed(MouseButton::Left) && right_mouse;

    let yaw_rot = Quat::from_rotation_y(orbit.yaw);
    let cam_forward = yaw_rot * Vec3::NEG_Z;
    let cam_right = yaw_rot * Vec3::X;

    // When right mouse is held, movement axes and facing track the camera.
    // Otherwise, movement is relative to the player's own facing and rotation is unchanged.
    let player_forward = transform.rotation * Vec3::NEG_Z;
    let player_right   = transform.rotation * Vec3::X;
    let (fwd_axis, right_axis, face_dir) = if right_mouse {
        (cam_forward, cam_right, cam_forward)
    } else {
        (player_forward, player_right, player_forward)
    };

    let move_dir = world_move_dir(&keyboard, both_mouse, fwd_axis, right_axis);

    if face_dir.length_squared() > 0.01 {
        let target_yaw = (-face_dir.x).atan2(-face_dir.z);
        transform.rotation = Quat::from_rotation_y(target_yaw);
    }

    controller.basis = TnuaBuiltinWalk {
        desired_motion: move_dir.into(),
        desired_forward: Dir3::new(face_dir).ok(),
    };

    // initiate_action_feeding must be called every frame before any action() calls
    controller.initiate_action_feeding();

    if keyboard.pressed(KeyCode::Space) {
        controller.action(ControlScheme::Jump(Default::default()));
    }
}
