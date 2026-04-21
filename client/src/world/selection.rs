use avian3d::prelude::*;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

use crate::world::camera::OrbitCamera;
use crate::world::players::RemotePlayerMarker;
use crate::world::terrain::PlayerMarker;
use shared::components::enemy::EnemyMarker;

/// A left-click is a "tap" (select) only if total mouse movement while held
/// stays below this pixel threshold.
const DRAG_THRESHOLD: f32 = 8.0;

#[derive(Resource, Default)]
pub struct SelectedTarget(pub Option<Entity>);

/// Tracks whether the in-progress left-click is a tap or a drag.
#[derive(Resource, Default)]
pub struct LeftClickState {
    drag_px: f32,
    started_on_ui: bool,
}

/// Runs each frame to accumulate drag distance for the held left button.
/// Must run before `select_on_click`.
pub fn track_left_drag(
    buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    ui_buttons: Query<&Interaction, With<Button>>,
    mut state: ResMut<LeftClickState>,
) {
    if buttons.just_pressed(MouseButton::Left) {
        state.drag_px = 0.0;
        state.started_on_ui = ui_buttons
            .iter()
            .any(|i| matches!(i, Interaction::Pressed | Interaction::Hovered));
    }
    if buttons.pressed(MouseButton::Left) {
        state.drag_px += mouse_motion.delta.length();
    }
}

/// Radius of the sphere used as a fallback pick when the precise raycast misses.
/// Gives a bit of click forgiveness in world space — close enemies gain the most
/// screen-space leniency, distant ones still require a reasonably accurate click.
const PICK_SPHERE_RADIUS: f32 = 0.5;

pub fn select_on_click(
    buttons: Res<ButtonInput<MouseButton>>,
    state: Res<LeftClickState>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    spatial_query: SpatialQuery,
    enemy_query: Query<(), With<EnemyMarker>>,
    remote_player_query: Query<(), With<RemotePlayerMarker>>,
    mut selected: ResMut<SelectedTarget>,
) {
    // Fire on release only — and only if the cursor barely moved (tap, not drag).
    if !buttons.just_released(MouseButton::Left)
        || buttons.pressed(MouseButton::Right)
        || state.drag_px > DRAG_THRESHOLD
        || state.started_on_ui
    {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Some(cursor_pos) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = camera_query.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor_pos) else { return };

    let is_target = |e: Entity| enemy_query.contains(e) || remote_player_query.contains(e);
    let filter = SpatialQueryFilter::default();

    // Precise raycast first — pinpoint clicks respect what's literally under the cursor,
    // including terrain occlusion (ray hits terrain → selection clears).
    let ray_hit = spatial_query.cast_ray(
        ray.origin,
        ray.direction,
        2000.0,
        true,
        &filter,
    );

    if let Some(h) = ray_hit {
        if is_target(h.entity) {
            selected.0 = Some(h.entity);
            return;
        }
    }

    // Forgiving fallback: sphere-cast along the ray, skipping non-target entities.
    // Runs whenever the precise ray didn't land on a target (terrain hit, sky, etc.),
    // so near-misses grab the nearest enemy within the swept volume.
    let shape = Collider::sphere(PICK_SPHERE_RADIUS);
    let config = ShapeCastConfig::from_max_distance(2000.0);
    let hit = spatial_query.cast_shape_predicate(
        &shape,
        ray.origin,
        Quat::IDENTITY,
        ray.direction,
        &config,
        &filter,
        &is_target,
    );

    selected.0 = hit.map(|h| h.entity);
}

/// Tab key cycles through nearby enemies, sorted nearest-first.
pub fn tab_cycle_selection(
    keys: Res<ButtonInput<KeyCode>>,
    player_query: Query<&GlobalTransform, With<PlayerMarker>>,
    enemy_query: Query<(Entity, &Transform), With<EnemyMarker>>,
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
