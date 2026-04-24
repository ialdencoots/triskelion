//! Enemy telegraph rendering. When the server replicates an `EnemyCast`
//! component onto a mob, spawn a ground disc at the cast's aim point that
//! fills up as the telegraph progresses. Remove the disc when the component
//! goes away (resolve or cancel).
//!
//! The mob's own mesh stays unchanged — the telegraph is a separate entity
//! at the `aim` point, which may be different from the mob's position (e.g.
//! GroundSlam locked on a kiting player's last seen spot).

use bevy::prelude::*;

use shared::components::combat::AttackShape;
use shared::components::enemy::EnemyCast;

/// Marks a spawned telegraph entity. The mob entity stores a handle to its
/// telegraph via `TelegraphEntity` so removal is O(1) without a query scan.
#[derive(Component)]
pub struct TelegraphMarker;

/// On a mob entity: the world entity that renders its current telegraph.
/// Present iff the mob has an active `EnemyCast`.
#[derive(Component)]
pub struct TelegraphEntity(pub Entity);

/// Height above the terrain to float the disc so it reads clearly without
/// z-fighting the ground. A centimeter or two would be enough visually,
/// but terrain normals may vary so a more generous offset is safer.
const DISC_LIFT: f32 = 0.05;
/// Maximum alpha at full fill. Below 1 so terrain reads through.
const DISC_ALPHA_MAX: f32 = 0.65;

/// Fires whenever the server replicates an `EnemyCast` onto a mob.
/// Spawns the telegraph rendering entity at the aim point and links it.
pub fn on_enemy_cast_added(
    trigger: On<Add, EnemyCast>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    casts: Query<&EnemyCast>,
) {
    let mob = trigger.event_target();
    let Ok(cast) = casts.get(mob) else { return };

    let radius = match cast.shape {
        AttackShape::Radius { radius } => radius,
        // Single-target attacks don't render a ground telegraph for now —
        // a floating cast-bar over the mob could replace this later.
        AttackShape::Single => return,
        // Cone telegraphs: stub a small disc at the aim for now. Proper
        // cone wedge geometry is a future polish pass.
        AttackShape::Cone { .. } => 1.0,
    };

    // Flat disc mesh (thin cylinder so it reads from any camera angle).
    let mesh = meshes.add(Cylinder::new(radius, 0.02));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.95, 0.2, 0.15, 0.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let disc = commands
        .spawn((
            TelegraphMarker,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(Vec3::new(
                cast.aim_x,
                cast.aim_y + DISC_LIFT,
                cast.aim_z,
            ))
            .with_scale(Vec3::new(0.01, 1.0, 0.01)),
        ))
        .id();

    commands.entity(mob).insert(TelegraphEntity(disc));
}

/// Fires when `EnemyCast` is removed from a mob (resolve, cancel, or
/// entity despawn). Despawns the linked telegraph entity and clears the
/// mob's `TelegraphEntity` pointer.
pub fn on_enemy_cast_removed(
    trigger: On<Remove, EnemyCast>,
    mut commands: Commands,
    linked: Query<&TelegraphEntity>,
) {
    let mob = trigger.event_target();
    let Ok(link) = linked.get(mob) else { return };
    commands.entity(link.0).despawn();
    commands.entity(mob).remove::<TelegraphEntity>();
}

/// Each frame, grow the telegraph disc scale from 0 → 1 over the cast
/// duration, and fade its material alpha up. `EnemyCast` replicates
/// `elapsed` and `duration` from the server, so this is a pure read.
pub fn update_telegraph_visuals(
    casts: Query<(&EnemyCast, &TelegraphEntity)>,
    mut discs: Query<(&mut Transform, &MeshMaterial3d<StandardMaterial>), With<TelegraphMarker>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (cast, link) in casts.iter() {
        let Ok((mut tf, mat_handle)) = discs.get_mut(link.0) else { continue };
        let progress = if cast.duration > 0.0 {
            (cast.elapsed / cast.duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        tf.scale.x = progress.max(0.01);
        tf.scale.z = progress.max(0.01);
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            mat.base_color.set_alpha(progress * DISC_ALPHA_MAX);
        }
    }
}
