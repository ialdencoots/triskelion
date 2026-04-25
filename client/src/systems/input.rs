use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::components::enemy::EnemyMarker;
use shared::components::player::{PlayerId, RoleStance, SelectedMobOrPlayer};
use shared::inputs::{AbilityInput, MinigameInput, PlayerInput};

use super::keybindings::{ActionBarBindings, ActionSlot};
use shared::instances::{find_def, terrain_surface_y};
use shared::messages::SelectTargetMsg;
use shared::settings::{AIRBORNE_HEIGHT_THRESHOLD, AIRBORNE_VY_THRESHOLD, PLAYER_FLOAT_HEIGHT};

use crate::ui::hud::action_bar::SlotClickPulse;
use crate::ui::hud::chat::ChatInputState;
use crate::world::camera::OrbitState;
use crate::world::controller::world_move_dir;
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
    chat_state: Res<ChatInputState>,
    mut pulse: ResMut<SlotClickPulse>,
    player_query: Query<(&Transform, &LinearVelocity), With<PlayerMarker>>,
    mut sender_query: Query<&mut MessageSender<PlayerInput>>,
    mut char_facing: Local<f32>,
) {
    let Ok(mut sender) = sender_query.single_mut() else { return };

    // While the chat input is focused, suppress keyboard-driven actions:
    // typed characters would otherwise also fire abilities / toggle stances.
    // Click-pulses from the action bar still flow through so mouse-driven
    // combat stays responsive.
    let keyboard_suppressed = chat_state.focused;

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

    let move_3d = if keyboard_suppressed {
        Vec3::ZERO
    } else {
        world_move_dir(&keyboard, both_mouse, fwd_axis, right_axis)
    };

    // TnuaController floats the physics capsule at ~1.95 m above the terrain using
    // a spring-damper, which produces small Y oscillations even at rest.  Relaying
    // those oscillations to other clients causes visible up/down jitter.
    //
    // Fix: only send the actual physics Y when the player is clearly airborne.
    // Detection combines two signals so the full jump arc is captured:
    //   • |vy| > AIRBORNE_VY_THRESHOLD — catches takeoff (vy ≈ +5.4) and landing (vy < 0)
    //   • height_above > AIRBORNE_HEIGHT_THRESHOLD — catches the jump apex where vy ≈ 0
    // When grounded, we send the canonical terrain + PLAYER_FLOAT_HEIGHT with vy = 0.
    let def = find_def(terrain.kind);
    let (y, vy, player_yaw) = player_query
        .single()
        .map(|(tf, lv)| {
            let terrain_y = terrain_surface_y(&terrain.noise, tf.translation.x, tf.translation.z, def);
            let height_above = tf.translation.y - terrain_y;
            let is_airborne =
                lv.y.abs() > AIRBORNE_VY_THRESHOLD || height_above > AIRBORNE_HEIGHT_THRESHOLD;
            let (y, vy) = if is_airborne {
                (tf.translation.y, lv.y)
            } else {
                (terrain_y + PLAYER_FLOAT_HEIGHT, 0.0)
            };
            let fwd = tf.rotation * Vec3::NEG_Z;
            let yaw = (-fwd.x).atan2(-fwd.z);
            (y, vy, yaw)
        })
        .unwrap_or((0.0, 0.0, *char_facing));

    // A slot fires this frame if either its bound key was just pressed OR its
    // on-screen button was just clicked (SlotClickPulse). Keyboard is
    // ignored while the chat input is focused so typing doesn't cast.
    let slot_fired = |slot: ActionSlot| -> bool {
        let i = slot.index();
        let key_hit = !keyboard_suppressed
            && bindings.0.get(i).map(|&k| keyboard.just_pressed(k)).unwrap_or(false);
        let click_hit = pulse.0.get(i).copied().unwrap_or(false);
        key_hit || click_hit
    };

    // Stance slots enter Tank/DPS/Heal; Escape exits any active stance.
    let enter_stance = if slot_fired(ActionSlot::Stance1) {
        Some(RoleStance::Tank)
    } else if slot_fired(ActionSlot::Stance2) {
        Some(RoleStance::Dps)
    } else if slot_fired(ActionSlot::Stance3) {
        Some(RoleStance::Heal)
    } else {
        None
    };
    // Escape while focused cancels chat input (handled in chat.rs); don't
    // also use it to exit stance on the same frame.
    let exit_stance = !keyboard_suppressed && keyboard.just_pressed(KeyCode::Escape);

    let (x, z) = player_query
        .single()
        .map(|(tf, _)| (tf.translation.x, tf.translation.z))
        .unwrap_or((0.0, 0.0));

    // Right mouse locks facing to camera; otherwise track the player's actual rotation.
    *char_facing = if right_mouse { orbit.yaw } else { player_yaw };

    let primary_commit   = slot_fired(ActionSlot::Primary1);
    let secondary_commit = slot_fired(ActionSlot::Primary2);
    let cube_left   = slot_fired(ActionSlot::SecondaryLeft);
    let cube_bottom = slot_fired(ActionSlot::SecondaryDown);
    let cube_right  = slot_fired(ActionSlot::SecondaryRight);
    let secondary_up = slot_fired(ActionSlot::SecondaryUp);

    // Clear the click pulse now that this frame's input has been read — next
    // frame a click must press again to fire.
    pulse.0 = [false; ActionSlot::COUNT];

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
            action_6: secondary_up,
        },
        facing_yaw: *char_facing,
    });
}
