use avian3d::prelude::*;
use bevy::prelude::*;

use crate::world::camera::OrbitCamera;
use crate::world::enemies::EnemyMarker;

#[derive(Resource, Default)]
pub struct SelectedTarget(pub Option<Entity>);

pub fn select_on_click(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    spatial_query: SpatialQuery,
    enemy_query: Query<(), With<EnemyMarker>>,
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
        .filter(|h| enemy_query.contains(h.entity))
        .map(|h| h.entity);
}
