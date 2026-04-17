use bevy::prelude::*;

use shared::components::combat::Health;
use shared::components::enemy::EnemyMarker;

use crate::world::camera::OrbitCamera;
use crate::world::terrain::PlayerMarker;

const BAR_W: f32 = 56.0;
const BAR_H: f32 = 7.0;
const HEALTH_BAR_RANGE: f32 = 30.0;

/// Marks the root UI node of a floating enemy health bar.
#[derive(Component)]
pub struct EnemyHealthBarRoot(pub Entity);

/// Marks the fill child of an enemy health bar, linking it back to the enemy entity.
#[derive(Component)]
pub struct EnemyHealthBarFill(pub Entity);

/// Observer: spawns a floating health bar UI node when an enemy is replicated.
pub fn on_enemy_bar_added(trigger: On<Add, EnemyMarker>, mut commands: Commands) {
    let enemy = trigger.event_target();
    commands
        .spawn((
            EnemyHealthBarRoot(enemy),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(BAR_W),
                height: Val::Px(BAR_H),
                left: Val::Px(-9999.0),
                top: Val::Px(-9999.0),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.25, 0.04, 0.04, 0.9)),
            Visibility::Hidden,
        ))
        .with_children(|bar| {
            bar.spawn((
                EnemyHealthBarFill(enemy),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.15, 0.70, 0.15)),
            ));
        });
}

/// Each frame: shows health bars for all enemies within range and updates fill
/// widths from the Health component.
pub fn update_enemy_bars(
    player_query: Query<&GlobalTransform, With<PlayerMarker>>,
    enemy_query: Query<(&Transform, Option<&Health>), With<EnemyMarker>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    mut bar_query: Query<(&EnemyHealthBarRoot, &mut Node, &mut Visibility), Without<EnemyHealthBarFill>>,
    mut fill_query: Query<(&EnemyHealthBarFill, &mut Node), Without<EnemyHealthBarRoot>>,
) {
    let Ok((camera, cam_tf)) = camera_query.single() else { return };
    let player_pos = player_query.single().map(|tf| tf.translation()).ok();

    for (bar_root, mut node, mut vis) in bar_query.iter_mut() {
        let enemy = bar_root.0;

        let Ok((tf, _)) = enemy_query.get(enemy) else {
            *vis = Visibility::Hidden;
            continue;
        };

        let in_range = player_pos
            .map(|pp| tf.translation.distance(pp) <= HEALTH_BAR_RANGE)
            .unwrap_or(false);

        if !in_range {
            *vis = Visibility::Hidden;
            continue;
        }

        let world_pos = tf.translation + Vec3::new(0.0, 1.6, 0.0);
        let Ok(screen) = camera.world_to_viewport(cam_tf, world_pos) else {
            *vis = Visibility::Hidden;
            continue;
        };

        node.left = Val::Px(screen.x - BAR_W * 0.5);
        node.top = Val::Px(screen.y - BAR_H - 4.0);
        *vis = Visibility::Inherited;
    }

    for (fill, mut fill_node) in fill_query.iter_mut() {
        let enemy = fill.0;
        let pct = enemy_query
            .get(enemy)
            .ok()
            .and_then(|(_, h)| h)
            .map(|h| (h.current / h.max * 100.0).clamp(0.0, 100.0))
            .unwrap_or(100.0);
        fill_node.width = Val::Percent(pct);
    }
}
