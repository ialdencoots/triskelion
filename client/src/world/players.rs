use avian3d::prelude::{Collider, Position, Sensor};
use bevy::prelude::*;

use shared::components::player::{PlayerId, PlayerPosition, PlayerVelocity};
use shared::instances::sample_height;

use super::instance::CurrentInstanceTerrain;

use crate::plugin::LocalClientId;

use super::terrain::PlayerMarker;

/// Marks a client-side entity that renders a remote (non-local) player.
#[derive(Component)]
pub struct RemotePlayerMarker;

/// Client-only dead-reckoning state for remote players.
/// Identical in purpose and structure to `EnemyDeadReckoning`; kept separate
/// so queries can distinguish enemies from players unambiguously.
#[derive(Component)]
pub struct PlayerDeadReckoning {
    /// Authoritative position at the time of the last server update.
    pub base_pos: Vec3,
    /// XZ velocity received from the server at that same update.
    pub vel: Vec2,
    /// Vertical velocity received from the server; used to extrapolate Y between
    /// updates so jumps appear smooth rather than stepping every 100 ms.
    pub vel_y: f32,
    /// `Time::elapsed_secs()` (client wall clock) when the update was applied.
    pub base_time: f32,
}

/// The server-replicated entity that corresponds to our own local player.
/// Set once in `on_remote_player_replicated` when `PlayerId` matches our client ID.
/// Used by `correct_local_player_position` to reconcile the physics body.
#[derive(Resource)]
pub struct OwnServerEntity(pub Entity);

/// Fires when the server replicates a `PlayerId` component to this client.
///
/// - Own entity (same client ID): record as `OwnServerEntity` for later reconciliation,
///   but do NOT insert a mesh — we already have the local physics capsule.
/// - Remote entities: insert mesh + dead-reckoning so we can render them.
pub fn on_remote_player_replicated(
    trigger: On<Add, PlayerId>,
    local_id: Res<LocalClientId>,
    time: Res<Time>,
    player_query: Query<(&PlayerId, Option<&PlayerPosition>, Option<&PlayerVelocity>)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let entity = trigger.event_target();
    let Ok((play_id, pos_opt, vel_opt)) = player_query.get(entity) else { return };

    if play_id.0 == local_id.0 {
        // Record which server entity is ours so correct_local_player_position
        // can watch it for authoritative position updates.
        commands.insert_resource(OwnServerEntity(entity));
        info!("[CLIENT] PlayerId replicated for our own entity {entity:?} — recorded as OwnServerEntity");
        return;
    }

    let translation = pos_opt.map(|p| p.to_vec3()).unwrap_or(Vec3::ZERO);
    let vel = vel_opt.map(|v| Vec2::new(v.vx, v.vz)).unwrap_or(Vec2::ZERO);
    let vel_y = vel_opt.map(|v| v.vy).unwrap_or(0.0);

    commands.entity(entity).insert((
        Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.6, 0.9), // blue — distinct from red enemies
            ..default()
        })),
        Transform::from_translation(translation),
        PlayerDeadReckoning { base_pos: translation, vel, vel_y, base_time: time.elapsed_secs() },
        // Sensor: intangible for physics but still hittable by raycasts (click selection).
        Collider::capsule(0.4, 1.0),
        Sensor,
        RemotePlayerMarker,
    ));
    info!("[CLIENT] Rendering remote player {entity:?} (client_id={})", play_id.0);
}

/// Records the latest server-authoritative state into the dead-reckoning baseline.
///
/// Does not attempt visual smoothing here — that belongs in `sync_player_positions`
/// which runs every frame.  Snapping state to server truth immediately gives the
/// extrapolation the most accurate starting point.
pub fn apply_player_corrections(
    time: Res<Time>,
    mut query: Query<
        (&PlayerPosition, &PlayerVelocity, &mut PlayerDeadReckoning),
        (Changed<PlayerPosition>, With<RemotePlayerMarker>),
    >,
) {
    let now = time.elapsed_secs();
    for (pos, vel, mut dr) in query.iter_mut() {
        dr.base_pos = pos.to_vec3();
        dr.vel = Vec2::new(vel.vx, vel.vz);
        dr.vel_y = vel.vy;
        dr.base_time = now;
    }
}

/// Extrapolates each remote player's `Transform` from its dead-reckoning baseline,
/// then smoothly chases the target so server corrections never produce instant jumps.
pub fn sync_player_positions(
    time: Res<Time>,
    terrain: Res<CurrentInstanceTerrain>,
    mut query: Query<(&PlayerDeadReckoning, &mut Transform), With<RemotePlayerMarker>>,
) {
    let t = time.elapsed_secs();
    let frame_dt = time.delta_secs();
    for (dr, mut tf) in query.iter_mut() {
        let dt = (t - dr.base_time).clamp(0.0, 0.3);
        let target_x = dr.base_pos.x + dr.vel.x * dt;
        let target_z = dr.base_pos.z + dr.vel.y * dt;
        let extrap_y = dr.base_pos.y + dr.vel_y * dt;
        let floor_y = sample_height(&terrain.noise, target_x, target_z, &terrain.cfg) + 1.1;
        let target_y = extrap_y.max(floor_y);
        let target = Vec3::new(target_x, target_y, target_z);

        // Smooth-chase the extrapolated target each frame rather than snapping.
        // At 60 fps this converges to within 2 cm in ~8 frames (~130 ms).
        let alpha = (15.0 * frame_dt).min(1.0);
        tf.translation = tf.translation.lerp(target, alpha);
    }
}

/// Snaps the local physics body to the server position only on large discontinuities
/// (knockbacks, teleports, respawns).  Normal divergence is ignored — the client's
/// physics owns movement.  Y is set to at least the terrain floor so the snap never
/// lands the player underground.
pub fn correct_local_player_position(
    own_server_entity: Option<Res<OwnServerEntity>>,
    server_query: Query<&PlayerPosition, Changed<PlayerPosition>>,
    mut player_query: Query<(&Transform, &mut Position), With<PlayerMarker>>,
    terrain: Res<CurrentInstanceTerrain>,
) {
    let Some(own) = own_server_entity else { return };
    let Ok(server_pos) = server_query.get(own.0) else { return };
    let Ok((tf, mut avian_pos)) = player_query.single_mut() else { return };

    let error_x = server_pos.x - tf.translation.x;
    let error_z = server_pos.z - tf.translation.z;
    let error = Vec2::new(error_x, error_z).length();

    if error < 3.0 {
        return;
    }

    let floor_y = sample_height(&terrain.noise, server_pos.x, server_pos.z, &terrain.cfg) + 1.1;
    avian_pos.0.x = server_pos.x;
    avian_pos.0.z = server_pos.z;
    avian_pos.0.y = server_pos.y.max(floor_y);
}
