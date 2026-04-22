use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::components::enemy::EnemyMarker;
use shared::components::player::{PlayerId, RoleStance, SelectedMobOrPlayer};
use shared::inputs::{AbilityInput, MinigameInput, PlayerInput};

use super::keybindings::ActionBarBindings;
use shared::instances::sample_height;
use shared::messages::SelectTargetMsg;

use crate::world::camera::OrbitState;
use crate::world::instance::CurrentInstanceTerrain;
use crate::world::selection::SelectedTarget;
use crate::world::terrain::PlayerMarker;

/// Watches `SelectedTarget` for changes and notifies the server when the local
/// player selects or deselects an enemy mob.
pub fn send_target_selection(
    selected: Res<SelectedTarget>,
    enemy_q: Query<(), With<EnemyMarker>>,
    player_id_q: Query<&PlayerId>,
    mut sender_q: Query<&mut MessageSender<SelectTargetMsg>>,
) {
    if !selected.is_changed() { return; }
    let Ok(mut sender) = sender_q.single_mut() else { return };
    let payload = match selected.0 {
        None => None,
        Some(e) if enemy_q.contains(e) => Some(SelectedMobOrPlayer::Mob(e)),
        Some(e) => player_id_q.get(e).ok().map(|p| SelectedMobOrPlayer::Player(p.0)),
    };
    sender.send::<GameChannel>(SelectTargetMsg(payload));
}

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
    terrain: Res<CurrentInstanceTerrain>,
    bindings: Res<ActionBarBindings>,
    player_query: Query<(&Transform, &LinearVelocity), With<PlayerMarker>>,
    mut sender_query: Query<&mut MessageSender<PlayerInput>>,
    mut char_facing: Local<f32>,
) {
    let Ok(mut sender) = sender_query.single_mut() else { return };

    let yaw_rot = Quat::from_rotation_y(orbit.yaw);
    let cam_forward = yaw_rot * Vec3::NEG_Z;
    let cam_right = yaw_rot * Vec3::X;

    let right_mouse = mouse_buttons.pressed(MouseButton::Right);
    let both_mouse = mouse_buttons.pressed(MouseButton::Left) && right_mouse;

    let player_yaw_rot = Quat::from_rotation_y(*char_facing);
    let player_forward = player_yaw_rot * Vec3::NEG_Z;
    let player_right   = player_yaw_rot * Vec3::X;
    let (fwd_axis, right_axis) = if right_mouse {
        (cam_forward, cam_right)
    } else {
        (player_forward, player_right)
    };

    let move_3d = if both_mouse {
        let forward = if keyboard.pressed(KeyCode::KeyD) { Vec3::ZERO } else { fwd_axis };
        let mut dir = forward;
        if keyboard.pressed(KeyCode::KeyS) { dir -= right_axis; }
        if keyboard.pressed(KeyCode::KeyF) { dir += right_axis; }
        if dir.length_squared() > 0.0 { dir.normalize() } else { Vec3::ZERO }
    } else {
        let mut dir = Vec3::ZERO;
        if keyboard.pressed(KeyCode::KeyE) { dir += fwd_axis; }
        if keyboard.pressed(KeyCode::KeyD) { dir -= fwd_axis; }
        if keyboard.pressed(KeyCode::KeyS) { dir -= right_axis; }
        if keyboard.pressed(KeyCode::KeyF) { dir += right_axis; }
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
    let (y, vy, player_yaw) = player_query
        .single()
        .map(|(tf, lv)| {
            let terrain_y = sample_height(&terrain.noise, tf.translation.x, tf.translation.z, &terrain.cfg);
            let height_above = tf.translation.y - terrain_y;
            let is_airborne = lv.y.abs() > 1.0 || height_above > 2.2;
            let (y, vy) = if is_airborne {
                (tf.translation.y, lv.y)
            } else {
                (terrain_y + 1.1, 0.0)
            };
            let fwd = tf.rotation * Vec3::NEG_Z;
            let yaw = (-fwd.x).atan2(-fwd.z);
            (y, vy, yaw)
        })
        .unwrap_or((0.0, 0.0, *char_facing));

    // 1/2/3 enter Tank/DPS/Heal; Escape exits any active stance.
    let enter_stance = if keyboard.just_pressed(KeyCode::Digit1) {
        Some(RoleStance::Tank)
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        Some(RoleStance::Dps)
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        Some(RoleStance::Heal)
    } else {
        None
    };
    let exit_stance = keyboard.just_pressed(KeyCode::Escape);

    let (x, z) = player_query
        .single()
        .map(|(tf, _)| (tf.translation.x, tf.translation.z))
        .unwrap_or((0.0, 0.0));

    // Right mouse locks facing to camera; otherwise track the player's actual rotation.
    *char_facing = if right_mouse { orbit.yaw } else { player_yaw };

    let primary_commit   = bindings.0.get(4).map(|&k| keyboard.just_pressed(k)).unwrap_or(false);
    let secondary_commit = bindings.0.get(3).map(|&k| keyboard.just_pressed(k)).unwrap_or(false);
    let cube_left   = bindings.0.get(5).map(|&k| keyboard.just_pressed(k)).unwrap_or(false);
    let cube_bottom = bindings.0.get(6).map(|&k| keyboard.just_pressed(k)).unwrap_or(false);
    let cube_right  = bindings.0.get(7).map(|&k| keyboard.just_pressed(k)).unwrap_or(false);

    sender.send::<GameChannel>(PlayerInput {
        movement: Vec2::new(move_3d.x, -move_3d.z),
        x,
        z,
        y,
        vy,
        abilities: AbilityInput { enter_stance, exit_stance, ..default() },
        minigame: MinigameInput {
            action_1: primary_commit,
            action_2: secondary_commit,
            action_3: cube_left,
            action_4: cube_bottom,
            action_5: cube_right,
        },
        facing_yaw: *char_facing,
    });
}
