use bevy::prelude::*;
use bevy_tnua::builtins::TnuaBuiltinWalk;
use bevy_tnua::TnuaController;

use super::camera::OrbitState;
use super::terrain::PlayerMarker;
use super::ControlScheme;

pub fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    orbit: Res<OrbitState>,
    mut player_query: Query<(&mut TnuaController<ControlScheme>, &mut Transform), With<PlayerMarker>>,
) {
    let Ok((mut controller, mut transform)) = player_query.single_mut() else { return };

    let both_mouse = mouse_buttons.pressed(MouseButton::Left) && mouse_buttons.pressed(MouseButton::Right);

    let yaw_rot = Quat::from_rotation_y(orbit.yaw);
    let cam_forward = yaw_rot * Vec3::NEG_Z;
    let cam_right = yaw_rot * Vec3::X;

    let move_dir = if both_mouse {
        cam_forward
    } else {
        let mut dir = Vec3::ZERO;
        if keyboard.pressed(KeyCode::KeyE) { dir += cam_forward; }
        if keyboard.pressed(KeyCode::KeyD) { dir -= cam_forward; }
        if keyboard.pressed(KeyCode::KeyS) { dir -= cam_right; }
        if keyboard.pressed(KeyCode::KeyF) { dir += cam_right; }
        if dir.length_squared() > 0.0 { dir.normalize() } else { Vec3::ZERO }
    };

    // Rotate character to face movement direction
    if move_dir.length_squared() > 0.01 {
        let target_yaw = move_dir.x.atan2(move_dir.z);
        transform.rotation = Quat::from_rotation_y(target_yaw);
    }

    controller.basis = TnuaBuiltinWalk {
        desired_motion: move_dir.into(),
        desired_forward: Dir3::new(move_dir).ok(),
    };

    // initiate_action_feeding must be called every frame before any action() calls
    controller.initiate_action_feeding();

    if keyboard.pressed(KeyCode::Space) {
        controller.action(ControlScheme::Jump(Default::default()));
    }
}
