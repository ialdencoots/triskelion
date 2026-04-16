use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::{AbilityCooldowns, CombatState};
use shared::components::player::{PlayerId, PlayerPosition, PlayerVelocity};
use shared::inputs::PlayerInput;
use shared::terrain;

use super::connection::PlayerEntityLink;

const PLAYER_SPEED: f32 = 6.0;

/// Read buffered `PlayerInput` messages from all connected clients and apply
/// movement to their server-side `PlayerPosition` / `PlayerVelocity`.
pub fn process_player_inputs(
    time: Res<Time>,
    mut link_query: Query<(&PlayerEntityLink, &mut MessageReceiver<PlayerInput>)>,
    mut player_query: Query<(&mut PlayerPosition, &mut PlayerVelocity), With<PlayerId>>,
) {
    let dt = time.delta_secs();

    for (link, mut receiver) in link_query.iter_mut() {
        // Use the most recent input in the buffer; ignore stale ones.
        let last_input = receiver.receive().last();
        let Ok((mut pos, mut vel)) = player_query.get_mut(link.0) else { continue };

        if let Some(input) = last_input {
            // The client encodes world-space XZ as (movement.x, -movement.z).
            let raw = Vec3::new(input.movement.x, 0.0, -input.movement.y);
            let dir = if raw.length_squared() > 0.01 { raw.normalize() } else { Vec3::ZERO };

            vel.vx = dir.x * PLAYER_SPEED;
            vel.vz = dir.z * PLAYER_SPEED;
            // Relay the client's physics Y and vertical velocity so other clients
            // can see jumps via dead-reckoning.
            vel.vy = input.vy;
            pos.x += vel.vx * dt;
            pos.z += vel.vz * dt;
            pos.y = input.y;
        } else {
            // No input this tick — zero XZ motion.
            vel.vx = 0.0;
            vel.vz = 0.0;
            vel.vy = 0.0;
            // Only snap to terrain when the player is already at (or below) ground
            // level.  If they are clearly airborne, hold the last replicated Y so
            // a single dropped packet mid-jump does not teleport them to the ground.
            let floor_y = terrain::height_at(pos.x, pos.z) + 1.1;
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
