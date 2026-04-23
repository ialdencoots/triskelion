use avian3d::prelude::{Collider, Sensor};
use bevy::prelude::*;

use shared::components::enemy::{BossMarker, EnemyMarker, EnemyPosition, EnemyVelocity};
use shared::instances::{find_def, terrain_surface_y};
use shared::settings::{BOSS_FLOAT_HEIGHT, PLAYER_FLOAT_HEIGHT};

use super::instance::CurrentInstanceTerrain;
use super::DeadReckoning;

/// Fires when the server replicates an enemy entity to this client.
/// Inserts the local rendering components (mesh + material + transform).
pub fn on_enemy_replicated(
    trigger: On<Add, EnemyMarker>,
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    positions: Query<&EnemyPosition>,
    velocities: Query<&EnemyVelocity>,
    boss_query: Query<(), With<BossMarker>>,
) {
    let entity = trigger.event_target();
    let pos_result = positions.get(entity);
    let translation = pos_result.map(|p| p.to_vec3()).unwrap_or(Vec3::ZERO);
    let vel = velocities.get(entity)
        .map(|v| Vec2::new(v.vx, v.vz))
        .unwrap_or(Vec2::ZERO);

    let is_boss = boss_query.contains(entity);

    info!(
        "[CLIENT] EnemyMarker replicated → entity {entity:?} at {translation:?} \
         (EnemyPosition found: {}, boss: {is_boss})",
        pos_result.is_ok()
    );

    let (capsule_mesh, collider, color) = if is_boss {
        (
            meshes.add(Capsule3d::new(1.0, 2.0)),
            Collider::capsule(1.0, 2.0),
            Color::srgb(0.5, 0.1, 0.7),
        )
    } else {
        (
            meshes.add(Capsule3d::new(0.4, 1.0)),
            Collider::capsule(0.4, 1.0),
            Color::srgb(0.75, 0.15, 0.15),
        )
    };

    commands.entity(entity).insert((
        Mesh3d(capsule_mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: color,
            ..default()
        })),
        Transform::from_translation(translation),
        DeadReckoning { base_pos: translation, vel, vel_y: 0.0, base_time: time.elapsed_secs() },
        // Collider required for SpatialQuery::cast_ray click selection.
        // Sensor makes the capsule intangible (no collision response) while
        // still being hittable by raycasts.
        collider,
        Sensor,
    ));
    info!("[CLIENT] Inserted mesh+collider for enemy {entity:?}");
}

/// Fires when the server replicates EnemyPosition to this client (separate from EnemyMarker).
/// Useful for diagnosing partial replication — if this fires but on_enemy_replicated doesn't,
/// EnemyPosition is arriving but EnemyMarker is not.
pub fn on_enemy_position_replicated(trigger: On<Add, EnemyPosition>) {
    let entity = trigger.event_target();
    info!("[CLIENT] EnemyPosition added to {entity:?}");
}

/// Anchors the dead-reckoning baseline whenever the server sends a new position.
///
/// Runs in `Update` before `sync_enemy_positions`.  The `Changed<EnemyPosition>`
/// filter means this only processes entities that received a new server update
/// this frame — usually just 0–3 enemies.
pub fn apply_server_corrections(
    time: Res<Time>,
    mut query: Query<(&EnemyPosition, &EnemyVelocity, &mut DeadReckoning), (With<EnemyMarker>, Changed<EnemyPosition>)>,
) {
    for (pos, vel, mut dr) in query.iter_mut() {
        dr.base_pos = pos.to_vec3();
        dr.vel = Vec2::new(vel.vx, vel.vz);
        dr.base_time = time.elapsed_secs();
    }
}

/// Extrapolates each enemy's `Transform` from its dead-reckoning baseline.
///
/// Runs in `Update` after `apply_server_corrections`.  Because this fires every
/// rendered frame (not just at 10 Hz), enemies move smoothly at display frame
/// rate even when no server update has arrived.
pub fn sync_enemy_positions(
    time: Res<Time>,
    terrain: Res<CurrentInstanceTerrain>,
    mut query: Query<(&DeadReckoning, Option<&BossMarker>, &mut Transform), With<EnemyMarker>>,
) {
    // Log enemy count every 5 seconds.
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    let prev = (t - dt) as u32;
    let curr = t as u32;
    if curr != prev && curr % 5 == 0 {
        let count = query.iter().count();
        info!("[CLIENT] sync_enemy_positions: {count} enemies tracked at t={t:.1}s");
    }

    let def = find_def(terrain.kind);
    for (dr, boss, mut tf) in query.iter_mut() {
        let extrap = dr.extrapolated_at(t);
        let floor_offset = if boss.is_some() { BOSS_FLOAT_HEIGHT } else { PLAYER_FLOAT_HEIGHT };
        let new_y = terrain_surface_y(&terrain.noise, extrap.x, extrap.z, def) + floor_offset;
        tf.translation = Vec3::new(extrap.x, new_y, extrap.z);
    }
}
