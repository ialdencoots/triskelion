use avian3d::prelude::Collider;
use bevy::prelude::*;

use shared::components::enemy::{EnemyMarker, EnemyPosition};

/// Fires when the server replicates an enemy entity to this client.
/// Inserts the local rendering components (mesh + material + transform).
pub fn on_enemy_replicated(
    trigger: On<Add, EnemyMarker>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    positions: Query<&EnemyPosition>,
) {
    let entity = trigger.event_target();
    let pos_result = positions.get(entity);
    let translation = pos_result.map(|p| p.to_vec3()).unwrap_or(Vec3::ZERO);
    info!(
        "[CLIENT] EnemyMarker replicated → entity {entity:?} at {translation:?} \
         (EnemyPosition found: {})",
        pos_result.is_ok()
    );

    commands.entity(entity).insert((
        Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.15, 0.15),
            ..default()
        })),
        Transform::from_translation(translation),
        // Collider required for SpatialQuery::cast_ray click selection.
        Collider::capsule(0.4, 1.0),
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

/// Keeps each enemy's `Transform` in sync with the replicated `EnemyPosition` every frame.
pub fn sync_enemy_positions(
    time: Res<Time>,
    mut query: Query<(&EnemyPosition, &mut Transform), With<EnemyMarker>>,
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

    for (pos, mut tf) in query.iter_mut() {
        tf.translation = pos.to_vec3();
    }
}
