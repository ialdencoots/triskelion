//! Floating heal-number popups above the warded player.
//!
//! Server emits `HealNumberMsg` to both the healer and the ward when a heal
//! lands; this module pops a green "+N" above the ward's transform that
//! rises and fades over `LIFETIME` seconds. Mirrors `damage_numbers.rs`
//! structure — the two surfaces share visual conventions (outline +
//! cardinal-offset stroke, jitter, crit pop) but stay in separate files so
//! the type-specific palette and target-query rules don't interleave.

use std::sync::atomic::{AtomicU64, Ordering};

use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::player::PlayerId;
use shared::messages::HealNumberMsg;

use crate::world::camera::OrbitCamera;
use crate::world::instance::FollowsEntity;
use crate::world::players::OwnServerEntity;
use crate::world::terrain::PlayerMarker;

const LIFETIME: f32 = 1.0;
const RISE: f32 = 0.9;
const BASE_Y_OFFSET: f32 = 2.0;
const HOLD_FRACTION: f32 = 0.55;
const JITTER_X: f32 = 0.55;
const OUTLINE_PX: f32 = 0.5;
const FONT_SIZE: f32 = 18.0;
const CRIT_POP_START_SCALE: f32 = 2.0;
const CRIT_POP_DURATION: f32 = 0.18;

/// Saturated green for the main "+N" fill.
const HEAL_FILL: Color = Color::srgb(0.40, 1.00, 0.55);
/// Lighter green for the outline halo.
const HEAL_OUTLINE: Color = Color::srgb(0.85, 1.00, 0.88);

/// A live floating heal number. Lives on a parent `Node` that carries
/// positioning; the visible text sits in five children (four outline copies
/// plus one main).
#[derive(Component)]
pub struct FloatingHealNumber {
    target: Entity,
    spawn_time: f32,
    offset_x: f32,
    is_crit: bool,
}

/// One text layer of a floating heal number — outline or main.
#[derive(Component)]
pub struct FloatingHealNumberPart {
    base_color: Color,
}

static SPAWN_COUNTER: AtomicU64 = AtomicU64::new(0);

fn jitter_from_seed(seed: u64) -> f32 {
    let mut x = seed.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^=  x >> 31;
    let unit = (x as u32) as f32 / u32::MAX as f32;
    unit * 2.0 - 1.0
}

/// Reads incoming `HealNumberMsg`s and spawns a floating-number node for each.
pub fn spawn_heal_numbers(
    mut link_query: Query<&mut MessageReceiver<HealNumberMsg>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let Ok(mut receiver) = link_query.single_mut() else { return };
    let now = time.elapsed_secs();

    for msg in receiver.receive() {
        let display = msg.amount.round().max(0.0) as i32;
        let label = format!("+{display}");
        let initial_font_size = if msg.is_crit { FONT_SIZE * CRIT_POP_START_SCALE } else { FONT_SIZE };
        let seed = SPAWN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let offset_x = jitter_from_seed(seed) * JITTER_X;

        commands
            .spawn((
                FloatingHealNumber {
                    target: msg.target,
                    spawn_time: now,
                    offset_x,
                    is_crit: msg.is_crit,
                },
                FollowsEntity(msg.target),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-9999.0),
                    top: Val::Px(-9999.0),
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|parent| {
                for (dx, dy) in [(-OUTLINE_PX, 0.0), (OUTLINE_PX, 0.0), (0.0, -OUTLINE_PX), (0.0, OUTLINE_PX)] {
                    parent.spawn((
                        FloatingHealNumberPart { base_color: HEAL_OUTLINE },
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(dx),
                            top:  Val::Px(dy),
                            ..default()
                        },
                        Text::new(label.clone()),
                        TextFont { font_size: initial_font_size, ..default() },
                        TextColor(HEAL_OUTLINE),
                    ));
                }
                parent.spawn((
                    FloatingHealNumberPart { base_color: HEAL_FILL },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top:  Val::Px(0.0),
                        ..default()
                    },
                    Text::new(label),
                    TextFont { font_size: FONT_SIZE, ..default() },
                    TextColor(HEAL_FILL),
                ));
            });
    }
}

/// Each frame: place each heal number above its target, fade alpha, despawn
/// past lifetime or when the target's gone. Heal targets are players. The
/// local player's server-replicated entity has `PlayerId` but no `Transform`
/// (the visible avatar is the physics capsule with `PlayerMarker`), so we
/// special-case `OwnServerEntity` to use the capsule's transform.
pub fn update_heal_numbers(
    time: Res<Time>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    own_server: Option<Res<OwnServerEntity>>,
    local_capsule_query: Query<&Transform, With<PlayerMarker>>,
    remote_query: Query<&Transform, With<PlayerId>>,
    mut parent_query: Query<(Entity, &FloatingHealNumber, &mut Node, &mut Visibility, &Children)>,
    mut part_query: Query<(&FloatingHealNumberPart, &mut TextColor, &mut TextFont)>,
    mut commands: Commands,
) {
    let Ok((camera, cam_tf)) = camera_query.single() else { return };
    let now = time.elapsed_secs();
    let own_id = own_server.as_ref().map(|r| r.0);

    for (entity, number, mut node, mut vis, children) in parent_query.iter_mut() {
        let elapsed = now - number.spawn_time;
        if elapsed >= LIFETIME {
            commands.entity(entity).despawn();
            continue;
        }

        let target_tf = if Some(number.target) == own_id {
            local_capsule_query.single().ok()
        } else {
            remote_query.get(number.target).ok()
        };
        let Some(target_tf) = target_tf else {
            commands.entity(entity).despawn();
            continue;
        };

        let progress = (elapsed / LIFETIME).clamp(0.0, 1.0);
        let world_pos = target_tf.translation
            + Vec3::new(number.offset_x, BASE_Y_OFFSET + RISE * progress, 0.0);

        let Ok(screen) = camera.world_to_viewport(cam_tf, world_pos) else {
            *vis = Visibility::Hidden;
            continue;
        };

        node.left = Val::Px(screen.x - 10.0);
        node.top  = Val::Px(screen.y - 10.0);
        *vis = Visibility::Inherited;

        let alpha = if progress < HOLD_FRACTION {
            1.0
        } else {
            1.0 - (progress - HOLD_FRACTION) / (1.0 - HOLD_FRACTION)
        };

        let font_size = if number.is_crit && elapsed < CRIT_POP_DURATION {
            let t = (elapsed / CRIT_POP_DURATION).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - t).powi(2);
            let start = FONT_SIZE * CRIT_POP_START_SCALE;
            start + (FONT_SIZE - start) * eased
        } else {
            FONT_SIZE
        };

        for child in children.iter() {
            if let Ok((part, mut color, mut font)) = part_query.get_mut(child) {
                *color = TextColor(part.base_color.with_alpha(alpha));
                if font.font_size != font_size {
                    font.font_size = font_size;
                }
            }
        }
    }
}
