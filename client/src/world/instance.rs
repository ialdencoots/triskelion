use avian3d::prelude::{Collider, RigidBody};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use fastnoise_lite::FastNoiseLite;
use lightyear::prelude::*;

use shared::components::enemy::EnemyMarker;
use shared::components::instance::InstanceId;
use shared::instances::{
    build_noise, find_def, sample_height, terrain_surface_y,
    InstanceDef, InstanceKind, TerrainConfig,
};
use shared::messages::InstanceEnteredMsg;

use super::players::RemotePlayerMarker;
use super::terrain::PlayerMarker;

// ── Resources ─────────────────────────────────────────────────────────────────

/// Client-side cached terrain configuration and noise generator.
/// Initialized from the default config (overworld) at startup.
/// Rebuilt when `InstanceEnteredMsg` arrives from the server.
#[derive(Resource)]
pub struct CurrentInstanceTerrain {
    pub cfg: TerrainConfig,
    pub noise: FastNoiseLite,
}

impl Default for CurrentInstanceTerrain {
    fn default() -> Self {
        let cfg = TerrainConfig::default();
        let noise = build_noise(&cfg);
        Self { cfg, noise }
    }
}

/// Tracks which instance the local player is currently in.
/// Used to show/hide entities that belong to other instances.
/// Initialised to 0 (the first-created Overworld instance).
#[derive(Resource, Default)]
pub struct CurrentInstanceId(pub u32);

// ── Tag ───────────────────────────────────────────────────────────────────────

/// Marks entities that belong to the current instance's visual scene and
/// should be despawned when the client transitions to a new instance.
/// Applied to: terrain mesh entity, enemy render entities, remote player entities.
#[derive(Component)]
pub struct InstanceSceneTag;

// ── System ────────────────────────────────────────────────────────────────────

/// Reads `InstanceEnteredMsg` from the server and rebuilds the terrain mesh.
pub fn handle_instance_entered(
    mut link_query: Query<&mut MessageReceiver<InstanceEnteredMsg>>,
    mut terrain_res: ResMut<CurrentInstanceTerrain>,
    mut current_instance: ResMut<CurrentInstanceId>,
    scene_entities: Query<Entity, With<InstanceSceneTag>>,
    player_query: Query<Entity, With<PlayerMarker>>,
    mut avian_positions: Query<&mut avian3d::prelude::Position>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(mut receiver) = link_query.single_mut() else { return };
    for msg in receiver.receive() {
        info!(
            "[CLIENT] InstanceEnteredMsg: instance={} kind={:?}",
            msg.instance_id, msg.kind
        );

        // Record which instance we are now in (used by sync_instance_visibility).
        current_instance.0 = msg.instance_id;

        // Despawn the previous instance's terrain.
        for e in scene_entities.iter() {
            commands.entity(e).despawn();
        }

        // Rebuild terrain resource from new config.
        terrain_res.cfg = msg.terrain;
        terrain_res.noise = build_noise(&msg.terrain);

        let base_color = terrain_color_for_kind(msg.kind);
        let def = find_def(msg.kind);

        // Spawn new terrain mesh — corridor-aware for layout instances.
        let mesh = if def.use_layout_terrain {
            build_layout_terrain_mesh(&msg.terrain, def)
        } else {
            build_terrain_mesh_from_config(&msg.terrain)
        };
        let collider = Collider::trimesh_from_mesh(&mesh).expect("terrain collider");
        commands.spawn((
            Name::new("Terrain"),
            InstanceSceneTag,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color,
                perceptual_roughness: 0.9,
                ..default()
            })),
            Transform::default(),
            RigidBody::Static,
            collider,
        ));

        // Teleport local physics body to spawn position. Use the layout-aware
        // surface height so the player lands on the actual mesh — bare
        // sample_height underplaces into walls when use_layout_terrain is true,
        // putting the camera inside the terrain (renders as void).
        let floor_y = terrain_surface_y(&terrain_res.noise, msg.spawn_x, msg.spawn_z, def);
        if let Ok(player_entity) = player_query.single() {
            if let Ok(mut pos) = avian_positions.get_mut(player_entity) {
                pos.0 = Vec3::new(msg.spawn_x, floor_y + 1.1, msg.spawn_z);
            }
        }
    }
}

/// Returns a distinct ground color for each instance kind.
fn terrain_color_for_kind(kind: InstanceKind) -> Color {
    match kind {
        InstanceKind::Overworld      => Color::srgb(0.35, 0.55, 0.25), // earthy green
        InstanceKind::CrystalCaverns => Color::srgb(0.18, 0.12, 0.38), // deep purple
    }
}

/// Shows entities whose `InstanceId` matches the current instance and hides
/// all others.  Runs every frame so late-arriving replicated entities are
/// caught without needing a separate observer.
pub fn sync_instance_visibility(
    current: Res<CurrentInstanceId>,
    mut enemies: Query<(&InstanceId, &mut Visibility), With<EnemyMarker>>,
    mut remote_players: Query<
        (&InstanceId, &mut Visibility),
        (With<RemotePlayerMarker>, Without<EnemyMarker>),
    >,
) {
    for (inst_id, mut vis) in enemies.iter_mut().chain(remote_players.iter_mut()) {
        *vis = if inst_id.0 == current.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

// ── Mesh builders ─────────────────────────────────────────────────────────────

/// Builds a layout-aware terrain mesh where heights outside the walkable
/// corridor/room footprint ramp up steeply to form impassable walls.
///
/// The height at each vertex is:
///   `sample_height(x,z) + clamp(layout_sdf(x,z) / BLEND_DIST, 0, 1) * WALL_HEIGHT`
///
/// Normals are computed from the modified positions so wall faces receive
/// correct shading (near-horizontal normals on ~79° wall faces).
pub fn build_layout_terrain_mesh(cfg: &TerrainConfig, def: &InstanceDef) -> Mesh {
    let noise = build_noise(cfg);
    let n = cfg.size + 1;
    let scale = cfg.tile_scale;
    let offset = (cfg.size as f32 * scale) / 2.0;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * n);
    let mut normals:   Vec<[f32; 3]> = Vec::with_capacity(n * n);
    let mut uvs:       Vec<[f32; 2]> = Vec::with_capacity(n * n);
    let mut indices:   Vec<u32>      = Vec::with_capacity(cfg.size * cfg.size * 6);

    for z in 0..n {
        for x in 0..n {
            let wx = x as f32 * scale - offset;
            let wz = z as f32 * scale - offset;
            positions.push([wx, terrain_surface_y(&noise, wx, wz, def), wz]);
            normals.push([0.0, 1.0, 0.0]); // overwritten below
            uvs.push([x as f32 / cfg.size as f32, z as f32 / cfg.size as f32]);
        }
    }

    // Central-difference normals from the already-computed (modified) positions.
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
            let normal = Vec3::new(-dx, 2.0 * scale, -dz).normalize();
            normals[z * n + x] = normal.into();
        }
    }

    for z in 0..cfg.size {
        for x in 0..cfg.size {
            let i = (z * n + x) as u32;
            let row = n as u32;
            indices.extend_from_slice(&[i, i + row, i + 1, i + 1, i + row, i + row + 1]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL,   normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0,     uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Builds a terrain mesh from an arbitrary `TerrainConfig`.
/// Extracted so both the initial `spawn_terrain` and the instance-switch path
/// use identical mesh generation logic.
pub fn build_terrain_mesh_from_config(cfg: &TerrainConfig) -> Mesh {
    let noise = build_noise(cfg);
    let n = cfg.size + 1;
    let scale = cfg.tile_scale;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * n);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(n * n);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(n * n);
    let mut indices: Vec<u32> = Vec::with_capacity(cfg.size * cfg.size * 6);

    let offset = (cfg.size as f32 * scale) / 2.0;

    for z in 0..n {
        for x in 0..n {
            let wx = x as f32 * scale - offset;
            let wz = z as f32 * scale - offset;
            let h = sample_height(&noise, wx, wz, cfg);
            positions.push([wx, h, wz]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([x as f32 / cfg.size as f32, z as f32 / cfg.size as f32]);
        }
    }

    // Smooth normals via central differences.
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
            let normal = Vec3::new(-dx, 2.0 * scale, -dz).normalize();
            normals[z * n + x] = normal.into();
        }
    }

    for z in 0..cfg.size {
        for x in 0..cfg.size {
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
