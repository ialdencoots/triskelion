use bevy::prelude::*;

use shared::components::enemy::EnemyMarker;

use crate::world::camera::OrbitCamera;
use crate::world::selection::SelectedTarget;

const BAR_W: f32 = 56.0;
const BAR_H: f32 = 7.0;

/// Marks the root UI node of a floating enemy health bar.
/// Stores the enemy entity this bar belongs to.
#[derive(Component)]
pub struct EnemyHealthBarRoot(pub Entity);

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
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.15, 0.70, 0.15)),
            ));
        });
}

/// Each frame: positions each floating bar above its enemy's head and
/// shows it only when that enemy is the current SelectedTarget.
pub fn update_enemy_bars(
    selected: Res<SelectedTarget>,
    enemy_query: Query<&Transform, With<EnemyMarker>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    mut bar_query: Query<(&EnemyHealthBarRoot, &mut Node, &mut Visibility)>,
) {
    let Ok((camera, cam_tf)) = camera_query.single() else { return };

    for (bar_root, mut node, mut vis) in bar_query.iter_mut() {
        let enemy = bar_root.0;

        if selected.0 != Some(enemy) {
            *vis = Visibility::Hidden;
            continue;
        }

        let Ok(tf) = enemy_query.get(enemy) else {
            *vis = Visibility::Hidden;
            continue;
        };

        // Project a point 1.6 units above the enemy origin to screen space.
        let world_pos = tf.translation + Vec3::new(0.0, 1.6, 0.0);
        let Ok(screen) = camera.world_to_viewport(cam_tf, world_pos) else {
            *vis = Visibility::Hidden;
            continue;
        };

        node.left = Val::Px(screen.x - BAR_W * 0.5);
        node.top = Val::Px(screen.y - BAR_H - 4.0);
        *vis = Visibility::Inherited;
    }
}
