use avian3d::prelude::{Collider, Position, Sensor};
use bevy::prelude::*;

use shared::components::player::{PlayerId, PlayerPosition, PlayerVelocity};
use shared::terrain;

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

/// Smoothly corrects the dead-reckoning baseline whenever the server sends a new position.
///
/// Rather than snapping `base_pos` directly to the server value, we extrapolate
/// to the current time first, then blend toward the server position.  This eliminates
/// the visible position jump that occurred every ~100 ms at 6 m/s.
pub fn apply_player_corrections(
    time: Res<Time>,
    mut query: Query<
        (&PlayerPosition, &PlayerVelocity, &mut PlayerDeadReckoning),
        (Changed<PlayerPosition>, With<RemotePlayerMarker>),
    >,
) {
    let now = time.elapsed_secs();
    for (pos, vel, mut dr) in query.iter_mut() {
        let server_pos = pos.to_vec3();

        // Extrapolate the current predicted position for a fair comparison.
        let extrap_dt = (now - dr.base_time).clamp(0.0, 0.3);
        let extrap_y = dr.base_pos.y + dr.vel_y * extrap_dt;
        let predicted = Vec3::new(
            dr.base_pos.x + dr.vel.x * extrap_dt,
            extrap_y,
            dr.base_pos.z + dr.vel.y * extrap_dt,
        );

        // XZ error — drives horizontal blending.
        let error_xz = Vec2::new(
            server_pos.x - predicted.x,
            server_pos.z - predicted.z,
        ).length();

        // Y error — blended independently.  Jumps produce large but valid Y
        // differences (several metres), so we use a generous snap threshold and
        // blend smoothly for normal corrections to avoid visible Y teleports.
        let error_y = (server_pos.y - extrap_y).abs();
        let new_y = if error_y < 0.3 {
            // Dead-reckoning is close enough — trust it.
            predicted.y
        } else if error_y < 5.0 {
            // Normal drift — blend 40 % toward server value.
            predicted.y + (server_pos.y - predicted.y) * 0.4
        } else {
            // Large discontinuity (respawn / teleport) — snap immediately.
            server_pos.y
        };

        let new_base = if error_xz < 0.1 {
            // Tiny XZ error — keep current prediction.
            Vec3::new(predicted.x, new_y, predicted.z)
        } else if error_xz < 3.0 {
            // Normal drift — blend 40 % toward server to converge smoothly.
            let blended = predicted.lerp(server_pos, 0.4);
            Vec3::new(blended.x, new_y, blended.z)
        } else {
            // Large discontinuity (teleport / respawn) — snap immediately.
            server_pos
        };

        dr.base_pos = new_base;
        dr.vel = Vec2::new(vel.vx, vel.vz);
        dr.vel_y = vel.vy;
        dr.base_time = now;
    }
}

/// Extrapolates each remote player's `Transform` from its dead-reckoning baseline.
pub fn sync_player_positions(
    time: Res<Time>,
    mut query: Query<(&PlayerDeadReckoning, &mut Transform), With<RemotePlayerMarker>>,
) {
    let t = time.elapsed_secs();
    for (dr, mut tf) in query.iter_mut() {
        let dt = (t - dr.base_time).clamp(0.0, 0.3);
        let new_x = dr.base_pos.x + dr.vel.x * dt;
        let new_z = dr.base_pos.z + dr.vel.y * dt;
        // Extrapolate Y from vertical velocity; clamp to terrain so the
        // player never clips underground if the server Y briefly goes stale.
        let extrap_y = dr.base_pos.y + dr.vel_y * dt;
        let floor_y = terrain::height_at(new_x, new_z) + 1.1;
        let new_y = extrap_y.max(floor_y);
        tf.translation = Vec3::new(new_x, new_y, new_z);
    }
}

/// Reconciles the local physics body with the server-authoritative position.
///
/// Runs in `Update` but only activates when the server sends a new `PlayerPosition`
/// (~10 Hz), via the `Changed<PlayerPosition>` filter.  Nudges Avian3d's `Position`
/// component (not `Transform` directly) so the physics simulation starts the next
/// tick from a corrected XZ location.  Y is intentionally left alone — TnuaController
/// manages the float height above the terrain.
pub fn correct_local_player_position(
    own_server_entity: Option<Res<OwnServerEntity>>,
    server_query: Query<&PlayerPosition, Changed<PlayerPosition>>,
    mut player_query: Query<(&Transform, &mut Position), With<PlayerMarker>>,
) {
    let Some(own) = own_server_entity else { return };
    let Ok(server_pos) = server_query.get(own.0) else { return };
    let Ok((tf, mut avian_pos)) = player_query.single_mut() else { return };

    let error_x = server_pos.x - tf.translation.x;
    let error_z = server_pos.z - tf.translation.z;
    let error = Vec2::new(error_x, error_z).length();

    if error < 0.2 {
        return;
    }

    let alpha = if error > 2.0 { 1.0 } else { 0.3 };

    avian_pos.0.x += error_x * alpha;
    avian_pos.0.z += error_z * alpha;
}
