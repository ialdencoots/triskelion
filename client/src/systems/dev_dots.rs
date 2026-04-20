// ═════════════════════════════════════════════════════════════════════════════
// DEV-ONLY — REMOVE BEFORE SHIP
// Keys 4/5/6 apply a Physical/Arcane/Nature DoT on the currently selected mob.
// To remove: delete this file, unregister the systems in client/src/plugin.rs,
// drop `dev_dots` from systems/mod.rs, and remove the matching server + shared
// message bits (grep for DEV-ONLY).
// ═════════════════════════════════════════════════════════════════════════════
use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::components::combat::DamageType;
use shared::messages::DevApplyDotMsg;

/// On `just_pressed` for 4/5/6, fire a `DevApplyDotMsg` with the matching type.
pub fn send_dev_dot_requests(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut sender_q: Query<&mut MessageSender<DevApplyDotMsg>>,
) {
    let Ok(mut sender) = sender_q.single_mut() else { return };

    let ty = if keyboard.just_pressed(KeyCode::Digit4) {
        Some(DamageType::Physical)
    } else if keyboard.just_pressed(KeyCode::Digit5) {
        Some(DamageType::Arcane)
    } else if keyboard.just_pressed(KeyCode::Digit6) {
        Some(DamageType::Nature)
    } else {
        None
    };

    if let Some(ty) = ty {
        sender.send::<GameChannel>(DevApplyDotMsg { ty });
    }
}
