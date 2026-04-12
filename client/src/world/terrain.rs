use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use fastnoise_lite::*;

use super::{ControlScheme, ControlSchemeConfig};

const TERRAIN_SIZE: usize = 128;
const TERRAIN_SCALE: f32 = 1.0;
const HEIGHT_SCALE: f32 = 6.0;

fn build_terrain_mesh() -> Mesh {
    let mut noise = FastNoiseLite::new();
    noise.set_noise_type(Some(NoiseType::Perlin));
    noise.set_frequency(Some(0.04));
    noise.set_fractal_type(Some(FractalType::FBm));
    noise.set_fractal_octaves(Some(4));
    noise.set_fractal_lacunarity(Some(2.0));
    noise.set_fractal_gain(Some(0.5));

    let n = TERRAIN_SIZE + 1;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * n);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(n * n);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(n * n);
    let mut indices: Vec<u32> = Vec::with_capacity(TERRAIN_SIZE * TERRAIN_SIZE * 6);

    let offset = (TERRAIN_SIZE as f32 * TERRAIN_SCALE) / 2.0;

    for z in 0..n {
        for x in 0..n {
            let wx = x as f32 * TERRAIN_SCALE - offset;
            let wz = z as f32 * TERRAIN_SCALE - offset;
            let h = noise.get_noise_2d(wx, wz) * HEIGHT_SCALE;
            positions.push([wx, h, wz]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([x as f32 / TERRAIN_SIZE as f32, z as f32 / TERRAIN_SIZE as f32]);
        }
    }

    // Smooth normals via central differences
    for z in 0..n {
        for x in 0..n {
            let h = |xi: i32, zi: i32| -> f32 {
                let cx = xi.clamp(0, (n - 1) as i32) as usize;
                let cz = zi.clamp(0, (n - 1) as i32) as usize;
                positions[cz * n + cx][1]
            };
            let xi = x as i32;
            let zi = z as i32;
            let dx = h(xi + 1, zi) - h(xi - 1, zi);
            let dz = h(xi, zi + 1) - h(xi, zi - 1);
            let normal = Vec3::new(-dx, 2.0 * TERRAIN_SCALE, -dz).normalize();
            normals[z * n + x] = normal.into();
        }
    }

    for z in 0..TERRAIN_SIZE {
        for x in 0..TERRAIN_SIZE {
            let i = (z * n + x) as u32;
            let row = n as u32;
            indices.extend_from_slice(&[i, i + row, i + 1, i + 1, i + row, i + row + 1]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub fn spawn_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = build_terrain_mesh();
    let collider = Collider::trimesh_from_mesh(&mesh).expect("terrain collider");
    let mesh_handle = meshes.add(mesh);

    commands.spawn((
        Name::new("Terrain"),
        Mesh3d(mesh_handle),
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
