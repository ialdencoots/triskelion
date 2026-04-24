use avian3d::prelude::*;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::ui::hud::combat_log::UiPointerGuard;

use super::terrain::PlayerMarker;

#[derive(Resource)]
pub struct OrbitState {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for OrbitState {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: -0.5, distance: 12.0 }
    }
}

#[derive(Component)]
pub struct OrbitCamera;

pub fn update_orbit_camera(
    mut commands: Commands,
    mut orbit: ResMut<OrbitState>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    pointer_guard: Res<UiPointerGuard>,
    player_query: Query<(Entity, &Transform), With<PlayerMarker>>,
    camera_query: Query<Entity, With<OrbitCamera>>,
    spatial_query: SpatialQuery,
) {
    if camera_query.is_empty() {
        commands.spawn((
            Name::new("OrbitCamera"),
            OrbitCamera,
            Camera3d::default(),
            Transform::default(),
        ));
        return;
    }

    // Skip orbit rotation while a UI element is consuming drag (combat-log
    // resize handle). Without this, click-drag on the handle would also spin
    // the camera.
    if !pointer_guard.blocks_camera_orbit
        && (mouse_buttons.pressed(MouseButton::Right) || mouse_buttons.pressed(MouseButton::Left))
    {
        orbit.yaw -= mouse_motion.delta.x * 0.005;
        orbit.pitch = (orbit.pitch - mouse_motion.delta.y * 0.005).clamp(-1.5, 1.5);
    }

    // Skip wheel-zoom when a UI element is consuming scroll (combat log
    // hover). `blocks_camera_zoom` is set every frame by the UI scroll handler.
    if !pointer_guard.blocks_camera_zoom {
        orbit.distance = (orbit.distance - mouse_scroll.delta.y * 1.5).clamp(0.0, 50.0);
    }

    let Ok((player_entity, player_tf)) = player_query.single() else { return };
    let Ok(camera_entity) = camera_query.single() else { return };

    let rot = Quat::from_euler(EulerRot::YXZ, orbit.yaw, orbit.pitch, 0.0);
    let target = player_tf.translation + Vec3::Y * 1.0;
    let ray_dir = rot * Vec3::Z;

    // Pull camera in when geometry would block the desired position.
    let actual_distance = if orbit.distance > 0.01 {
        if let Ok(dir) = Dir3::new(ray_dir) {
            let filter = SpatialQueryFilter {
                excluded_entities: [player_entity].into_iter().collect(),
                ..default()
            };
            spatial_query
                .cast_ray(target, dir, orbit.distance, true, &filter)
                .map(|hit| (hit.distance - 0.1).max(0.0))
                .unwrap_or(orbit.distance)
        } else {
            orbit.distance
        }
    } else {
        orbit.distance
    };

    let eye = target + ray_dir * actual_distance;
    // Use looking_to so first-person (distance=0) works without a degenerate look_at call.
    let transform = Transform::from_translation(eye).looking_to(rot * Vec3::NEG_Z, Vec3::Y);

    commands.entity(camera_entity).insert(transform);
}
