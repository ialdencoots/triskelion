use avian3d::prelude::*;
use bevy::prelude::*;

use crate::world::camera::OrbitCamera;
use crate::world::players::RemotePlayerMarker;
use crate::world::terrain::PlayerMarker;
use shared::components::enemy::EnemyMarker;

#[derive(Resource, Default)]
pub struct SelectedTarget(pub Option<Entity>);

pub fn select_on_click(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    spatial_query: SpatialQuery,
    enemy_query: Query<(), With<EnemyMarker>>,
    remote_player_query: Query<(), With<RemotePlayerMarker>>,
    mut selected: ResMut<SelectedTarget>,
) {
    // Only act on a plain left click — skip if right mouse is held (that's orbit/movement).
    if !buttons.just_pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right) {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Some(cursor_pos) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = camera_query.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor_pos) else { return };

    let hit = spatial_query.cast_ray(
        ray.origin,
        ray.direction,
        500.0,
        true,
        &SpatialQueryFilter::default(),
    );

    selected.0 = hit
        .filter(|h| enemy_query.contains(h.entity) || remote_player_query.contains(h.entity))
        .map(|h| h.entity);
}

/// Tab key cycles through enemies and remote players within range, sorted nearest-first.
pub fn tab_cycle_selection(
    keys: Res<ButtonInput<KeyCode>>,
    player_query: Query<&GlobalTransform, With<PlayerMarker>>,
    enemy_query: Query<(Entity, &Transform), With<EnemyMarker>>,
    remote_player_query: Query<(Entity, &Transform), With<RemotePlayerMarker>>,
    mut selected: ResMut<SelectedTarget>,
) {
    if !keys.just_pressed(KeyCode::Tab) {
        return;
    }

    const RANGE: f32 = 30.0;
    let Ok(player_tf) = player_query.single() else { return };
    let player_pos = player_tf.translation();

    let mut nearby: Vec<(Entity, f32)> = enemy_query
        .iter()
        .chain(remote_player_query.iter())
        .filter_map(|(e, tf)| {
            let dist = tf.translation.distance(player_pos);
            if dist <= RANGE { Some((e, dist)) } else { None }
        })
        .collect();

    if nearby.is_empty() {
        return;
    }

    nearby.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let current_idx = selected
        .0
        .and_then(|e| nearby.iter().position(|(ne, _)| *ne == e));

    let next_idx = match current_idx {
        Some(i) => (i + 1) % nearby.len(),
        None => 0,
    };

    selected.0 = Some(nearby[next_idx].0);
}
