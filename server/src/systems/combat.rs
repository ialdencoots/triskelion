use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::{AbilityCooldowns, CombatState};
use shared::components::instance::InstanceId;
use shared::components::player::{PlayerId, PlayerPosition, PlayerVelocity};
use shared::inputs::PlayerInput;
use shared::instances::{find_def, sample_height};

use super::connection::PlayerEntityLink;
use super::instances::InstanceRegistry;

const PLAYER_SPEED: f32 = 6.0;

/// Read buffered `PlayerInput` messages from all connected clients and apply
/// movement and stance changes to their server-side components.
pub fn process_player_inputs(
    time: Res<Time>,
    reg: Res<InstanceRegistry>,
    mut link_query: Query<(&PlayerEntityLink, &mut MessageReceiver<PlayerInput>)>,
    mut player_query: Query<(&mut PlayerPosition, &mut PlayerVelocity, &mut CombatState, Option<&InstanceId>), With<PlayerId>>,
) {
    let dt = time.delta_secs();

    for (link, mut receiver) in link_query.iter_mut() {
        // Use the most recent input in the buffer; ignore stale ones.
        let last_input = receiver.receive().last();
        let Ok((mut pos, mut vel, mut combat, iid_opt)) = player_query.get_mut(link.0) else { continue };

        if let Some(input) = last_input {
            // ── Movement ───────────────────────────────────────────────────────
            // XYZ position comes directly from the client's physics body so the
            // server position stays in sync with what the client actually simulates.
            // Velocity is derived from the input direction for dead-reckoning by
            // other clients; it is not used to integrate position server-side.
            let raw = Vec3::new(input.movement.x, 0.0, -input.movement.y);
            let dir = if raw.length_squared() > 0.01 { raw.normalize() } else { Vec3::ZERO };

            vel.vx = dir.x * PLAYER_SPEED;
            vel.vz = dir.z * PLAYER_SPEED;
            vel.vy = input.vy;
            pos.x = input.x;
            pos.z = input.z;
            pos.y = input.y;

            // ── Stance transitions ─────────────────────────────────────────────
            if input.abilities.exit_stance {
                combat.active_stance = None;
            } else if let Some(stance) = input.abilities.enter_stance {
                // Pressing the active stance's key again toggles back to neutral.
                combat.active_stance = if combat.active_stance == Some(stance) {
                    None
                } else {
                    Some(stance)
                };
            }
        } else {
            // No input this tick — zero XZ motion.
            vel.vx = 0.0;
            vel.vz = 0.0;
            vel.vy = 0.0;
            let floor_y = if let Some(iid) = iid_opt {
                if let Some(live) = reg.instances.get(&iid.0) {
                    let def = find_def(live.kind);
                    sample_height(&live.noise, pos.x, pos.z, &def.terrain) + 1.1
                } else {
                    pos.y
                }
            } else {
                pos.y
            };
            if pos.y <= floor_y + 0.1 {
                pos.y = floor_y;
            }
        }
    }
}

/// Decrement all ability cooldowns by the elapsed fixed-timestep delta.
pub fn tick_ability_cooldowns(time: Res<Time>, mut query: Query<&mut AbilityCooldowns>) {
    let dt = time.delta_secs();
    for mut cd in query.iter_mut() {
        cd.mobility_cd = (cd.mobility_cd - dt).max(0.0);
        cd.cc_cd = (cd.cc_cd - dt).max(0.0);
        cd.taunt_cd = (cd.taunt_cd - dt).max(0.0);
        cd.interrupt_cd = (cd.interrupt_cd - dt).max(0.0);
        cd.stance_cd = (cd.stance_cd - dt).max(0.0);
    }
}
