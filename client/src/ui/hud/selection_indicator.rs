use bevy::prelude::*;

use shared::components::enemy::{BossMarker, EnemyMarker};

use crate::world::camera::OrbitCamera;
use crate::world::players::RemotePlayerMarker;
use crate::world::selection::SelectedTarget;

const CORNER_PX: f32 = 12.0;
const THICKNESS: f32 = 2.0;
const PAD: f32 = 8.0;
const COLOR: Color = Color::srgba(0.95, 0.92, 0.1, 0.9);
const CLEAR: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);

#[derive(Component)]
pub struct SelectionIndicatorRoot;

fn corner_border(left: bool, right: bool, top: bool, bottom: bool) -> (Node, BorderColor) {
    let px = |show: bool| if show { Val::Px(THICKNESS) } else { Val::Px(0.0) };
    let c = |show: bool| if show { COLOR } else { CLEAR };
    (
        Node {
            position_type: PositionType::Absolute,
            left:   if left   { Val::Px(0.0) } else { Val::Auto },
            right:  if right  { Val::Px(0.0) } else { Val::Auto },
            top:    if top    { Val::Px(0.0) } else { Val::Auto },
            bottom: if bottom { Val::Px(0.0) } else { Val::Auto },
            width: Val::Px(CORNER_PX),
            height: Val::Px(CORNER_PX),
            border: UiRect {
                left:   px(left),
                right:  px(right),
                top:    px(top),
                bottom: px(bottom),
            },
            ..default()
        },
        BorderColor {
            left:   c(left),
            right:  c(right),
            top:    c(top),
            bottom: c(bottom),
        },
    )
}

pub fn spawn_selection_indicator(mut commands: Commands) {
    commands
        .spawn((
            SelectionIndicatorRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(-9999.0),
                top: Val::Px(-9999.0),
                width: Val::Px(60.0),
                height: Val::Px(80.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|p| {
            let (n, bc) = corner_border(true, false, true, false);   // top-left
            p.spawn((n, bc));
            let (n, bc) = corner_border(false, true, true, false);   // top-right
            p.spawn((n, bc));
            let (n, bc) = corner_border(true, false, false, true);   // bottom-left
            p.spawn((n, bc));
            let (n, bc) = corner_border(false, true, false, true);   // bottom-right
            p.spawn((n, bc));
        });
}

/// Each frame: projects the selected target's capsule bounding box to screen
/// space and sizes/positions the four corner brackets around it.
pub fn update_selection_indicator(
    selected: Res<SelectedTarget>,
    target_query: Query<&Transform, Or<(With<EnemyMarker>, With<RemotePlayerMarker>)>>,
    boss_query: Query<(), With<BossMarker>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    mut indicator_query: Query<(&mut Node, &mut Visibility), With<SelectionIndicatorRoot>>,
) {
    let Ok((mut node, mut vis)) = indicator_query.single_mut() else { return };
    let Ok((camera, cam_tf)) = camera_query.single() else { return };

    let Some(target) = selected.0 else {
        *vis = Visibility::Hidden;
        return;
    };

    let Ok(tf) = target_query.get(target) else {
        *vis = Visibility::Hidden;
        return;
    };

    let pos = tf.translation;
    // Camera right in world space — used to estimate screen-space width of the capsule.
    let cam_right = cam_tf.compute_transform().rotation * Vec3::X;

    // Boss: radius 1.0, cylinder height 2.0 → half-height 2.0, half-width 1.0.
    // Normal: radius 0.4, cylinder height 1.0 → half-height 0.9, half-width 0.4 (use 0.5).
    let (half_height, half_width) = if boss_query.contains(target) {
        (2.0_f32, 1.0_f32)
    } else {
        (0.9_f32, 0.5_f32)
    };
    let top_world   = pos + Vec3::Y * half_height;
    let bot_world   = pos - Vec3::Y * half_height;
    let right_world = pos + cam_right * half_width;

    let Ok(s_top)   = camera.world_to_viewport(cam_tf, top_world) else {
        *vis = Visibility::Hidden;
        return;
    };
    let Ok(s_bot)   = camera.world_to_viewport(cam_tf, bot_world) else {
        *vis = Visibility::Hidden;
        return;
    };
    let Ok(s_ctr)   = camera.world_to_viewport(cam_tf, pos) else {
        *vis = Visibility::Hidden;
        return;
    };
    let Ok(s_right) = camera.world_to_viewport(cam_tf, right_world) else {
        *vis = Visibility::Hidden;
        return;
    };

    // Viewport Y increases downward, so the capsule top has a smaller Y value.
    let half_w = (s_right.x - s_ctr.x).abs().max(10.0);
    let top_y  = s_top.y.min(s_bot.y);
    let h      = (s_top.y - s_bot.y).abs();

    node.left   = Val::Px(s_ctr.x - half_w - PAD);
    node.top    = Val::Px(top_y - PAD);
    node.width  = Val::Px((half_w + PAD) * 2.0);
    node.height = Val::Px(h + PAD * 2.0);
    *vis = Visibility::Inherited;
}
