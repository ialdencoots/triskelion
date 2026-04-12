use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

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
    player_query: Query<&Transform, With<PlayerMarker>>,
    camera_query: Query<Entity, With<OrbitCamera>>,
) {
    // Spawn camera on first run if it doesn't exist
    if camera_query.is_empty() {
        commands.spawn((
            Name::new("OrbitCamera"),
            OrbitCamera,
            Camera3d::default(),
            Transform::default(),
        ));
        return;
    }

    // Orbit on right-click drag
    if mouse_buttons.pressed(MouseButton::Right) {
        orbit.yaw -= mouse_motion.delta.x * 0.005;
        orbit.pitch = (orbit.pitch - mouse_motion.delta.y * 0.005).clamp(-1.4, -0.05);
    }

    // Zoom with scroll wheel
    orbit.distance = (orbit.distance - mouse_scroll.delta.y * 1.5).clamp(3.0, 50.0);

    let Ok(player_tf) = player_query.single() else { return };
    let camera_entity = camera_query.single().unwrap();

    let rot = Quat::from_euler(EulerRot::YXZ, orbit.yaw, orbit.pitch, 0.0);
    let offset = rot * Vec3::new(0.0, 0.0, orbit.distance);
    let eye = player_tf.translation + Vec3::Y * 1.0 + offset;
    let target = player_tf.translation + Vec3::Y * 1.0;

    commands.entity(camera_entity).insert(
        Transform::from_translation(eye).looking_at(target, Vec3::Y)
    );
}
