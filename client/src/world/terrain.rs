use avian3d::prelude::*;
use bevy::prelude::*;

use shared::instances::TerrainConfig;

use super::{ControlScheme, ControlSchemeConfig};
use super::instance::{build_terrain_mesh_from_config, InstanceSceneTag};

pub fn spawn_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cfg = TerrainConfig::default();
    let mesh = build_terrain_mesh_from_config(&cfg);
    let collider = Collider::trimesh_from_mesh(&mesh).expect("terrain collider");

    commands.spawn((
        Name::new("Terrain"),
        InstanceSceneTag,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.55, 0.25),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::default(),
        RigidBody::Static,
        collider,
    ));
}

#[derive(Component)]
pub struct PlayerMarker;

pub fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut configs: ResMut<Assets<ControlSchemeConfig>>,
) {
    use bevy_tnua::builtins::{TnuaBuiltinJumpConfig, TnuaBuiltinWalkConfig};

    let config_handle = configs.add(ControlSchemeConfig {
        basis: TnuaBuiltinWalkConfig {
            float_height: 1.05,
            speed: 6.0,
            ..default()
        },
        jump: TnuaBuiltinJumpConfig {
            height: 1.5,
            ..default()
        },
    });

    commands.spawn((
        Name::new("Player"),
        PlayerMarker,
        Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.3, 0.8),
            ..default()
        })),
        Transform::from_xyz(0.0, 8.0, 0.0),
        RigidBody::Dynamic,
        Collider::capsule(0.4, 1.0),
        LockedAxes::ROTATION_LOCKED,
        bevy_tnua::TnuaController::<ControlScheme>::default(),
        bevy_tnua::TnuaConfig::<ControlScheme>(config_handle),
        bevy_tnua_avian3d::TnuaAvian3dSensorShape(Collider::cylinder(0.35, 0.0)),
    ));
}

pub fn spawn_light(mut commands: Commands) {
    commands.spawn((
        Name::new("Sun"),
        DirectionalLight {
            illuminance: 10_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.4, 0.0)),
    ));
}
