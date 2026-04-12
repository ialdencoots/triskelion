use avian3d::prelude::*;
use bevy::prelude::*;

#[derive(Component)]
pub struct EnemyMarker;

pub fn spawn_enemies(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("Enemy"),
        EnemyMarker,
        Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.15, 0.15),
            ..default()
        })),
        Transform::from_xyz(5.0, 10.0, -4.0),
        RigidBody::Dynamic,
        Collider::capsule(0.4, 1.0),
        LockedAxes::ROTATION_LOCKED,
    ));
}
