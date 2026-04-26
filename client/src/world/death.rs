use avian3d::prelude::{LockedAxes, Position, Rotation};
use bevy::prelude::*;
use bevy_tnua::TnuaController;

use shared::components::combat::Dead;
use shared::components::enemy::BossMarker;
use shared::instances::{find_def, terrain_surface_y};

use super::instance::CurrentInstanceTerrain;
use super::players::OwnServerEntity;
use super::terrain::PlayerMarker;
use super::{ControlScheme, DeadReckoning};

/// Capsule radius for non-boss entities (matches the spawn dimensions in
/// `client::world::players` and `client::world::enemies`). When a capsule is
/// lying on its side, this is the distance from its center to the ground —
/// i.e. the Y offset that puts the side of the capsule flush with terrain.
const NORMAL_CAPSULE_RADIUS: f32 = 0.4;
/// Capsule radius for boss entities (matches the boss spawn in
/// `client::world::enemies`).
const BOSS_CAPSULE_RADIUS: f32 = 1.0;

/// Reacts to the server inserting a `Dead` marker on any entity.
///
/// For remote players and enemies, the trigger entity is the same one that
/// holds the rendered capsule, so we tip its `Transform` directly and drop
/// the Y onto the terrain (the dead-reckoning sync had been holding it at
/// standing-float height — fine while the capsule was upright, but a tipped
/// capsule needs to sit at `terrain + radius` instead). Removing
/// `DeadReckoning` after the rotation freezes the position at its last
/// server snapshot rather than continuing to extrapolate or get re-synced
/// to standing height.
///
/// The local player branch lives in [`on_dead_added_local`]: the trigger is
/// the server-replicated entity (held in `OwnServerEntity`), which carries
/// no mesh — the rendered capsule is a separate `PlayerMarker` entity owned
/// by Avian/Tnua and needs the controller torn down before any rotation
/// sticks.
pub fn on_dead_added(
    trigger: On<Add, Dead>,
    own: Option<Res<OwnServerEntity>>,
    terrain: Option<Res<CurrentInstanceTerrain>>,
    boss_q: Query<(), With<BossMarker>>,
    mut commands: Commands,
    mut tf_q: Query<&mut Transform, Without<PlayerMarker>>,
) {
    let entity = trigger.event_target();
    if let Some(own) = own {
        if entity == own.0 {
            return;
        }
    }
    if let Ok(mut tf) = tf_q.get_mut(entity) {
        let local_x = tf.local_x();
        tf.rotation = Quat::from_axis_angle(*local_x, std::f32::consts::FRAC_PI_2) * tf.rotation;

        if let Some(terrain) = terrain.as_deref() {
            let radius = if boss_q.contains(entity) {
                BOSS_CAPSULE_RADIUS
            } else {
                NORMAL_CAPSULE_RADIUS
            };
            let def = find_def(terrain.kind);
            let ground = terrain_surface_y(&terrain.noise, tf.translation.x, tf.translation.z, def);
            tf.translation.y = ground + radius;
        }
    }
    commands.entity(entity).remove::<DeadReckoning>();
}

/// Handles the local-player branch of death: when `Dead` arrives on the
/// server-replicated `OwnServerEntity`, the rendered capsule on the
/// `PlayerMarker` entity needs to topple. Tnua + Avian fight a manual rotation
/// otherwise — Tnua keeps the capsule upright via its walk basis and Avian's
/// `LockedAxes::ROTATION_LOCKED` prevents physics torques from rotating it —
/// so we strip both before tipping. Position is also dropped to lie on the
/// ground; without that the capsule would float at its last standing Y until
/// gravity settled it (and Tnua's float force would also have been fighting
/// to keep it raised before we tore the controller off).
pub fn on_dead_added_local(
    trigger: On<Add, Dead>,
    own: Option<Res<OwnServerEntity>>,
    terrain: Option<Res<CurrentInstanceTerrain>>,
    mut commands: Commands,
    mut player_q: Query<
        (Entity, &mut Transform, Option<&mut Rotation>, Option<&mut Position>),
        With<PlayerMarker>,
    >,
) {
    let Some(own) = own else { return };
    if trigger.event_target() != own.0 {
        return;
    }
    let Ok((entity, mut tf, rotation_opt, position_opt)) = player_q.single_mut() else { return };

    let local_x = tf.local_x();
    let tip = Quat::from_axis_angle(*local_x, std::f32::consts::FRAC_PI_2) * tf.rotation;
    tf.rotation = tip;
    if let Some(mut rot) = rotation_opt {
        rot.0 = tip;
    }

    if let Some(terrain) = terrain.as_deref() {
        let def = find_def(terrain.kind);
        let ground = terrain_surface_y(&terrain.noise, tf.translation.x, tf.translation.z, def);
        let target_y = ground + NORMAL_CAPSULE_RADIUS;
        tf.translation.y = target_y;
        if let Some(mut pos) = position_opt {
            pos.0.y = target_y;
        }
    }

    commands
        .entity(entity)
        .remove::<TnuaController<ControlScheme>>()
        .remove::<LockedAxes>();
}
