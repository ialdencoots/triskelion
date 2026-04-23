use std::sync::atomic::{AtomicU64, Ordering};

use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::DamageType;
use shared::components::enemy::EnemyMarker;
use shared::messages::DamageNumberMsg;

use crate::world::camera::OrbitCamera;
use crate::world::instance::FollowsEntity;

/// Seconds a floating damage number lives before despawning.
const LIFETIME: f32 = 1.0;
/// How many world units the number rises over its lifetime.
const RISE: f32 = 0.9;
/// World-space Y offset above the enemy's transform origin where numbers spawn.
const BASE_Y_OFFSET: f32 = 2.0;
/// Fraction of lifetime spent at full alpha before the fade begins.
const HOLD_FRACTION: f32 = 0.55;
/// Max world-space horizontal jitter applied at spawn, ± this value. Keeps
/// simultaneous hits on the same mob from stacking directly on top of each other.
const JITTER_X: f32 = 0.55;
/// Outline stroke width in logical pixels — applied in four cardinal offsets
/// around the main text to form a hand-rolled stroke.
const OUTLINE_PX: f32 = 0.5;
/// Font size for damage number text.
const FONT_SIZE: f32 = 18.0;
/// Initial font scale for a crit. The number spawns at this size and shrinks
/// back to `FONT_SIZE` over `CRIT_POP_DURATION` for a "pop" effect.
const CRIT_POP_START_SCALE: f32 = 2.0;
/// Seconds over which a crit shrinks from its peak size down to normal.
const CRIT_POP_DURATION: f32 = 0.18;

/// A live floating damage number. Lives on a parent `Node` that carries
/// positioning; the actual visible text is split across five children — four
/// outline copies at cardinal offsets plus one main copy on top.
#[derive(Component)]
pub struct FloatingNumber {
    target: Entity,
    spawn_time: f32,
    offset_x: f32,
    /// When true, the updater animates the font size from
    /// `CRIT_POP_START_SCALE × FONT_SIZE` down to `FONT_SIZE` over
    /// `CRIT_POP_DURATION`. No color change — the type hue still carries the
    /// identity; the pop carries the "this was special" beat.
    is_crit: bool,
}

/// A single text layer of a floating number (one outline copy or the main
/// copy). Stored so the updater can fade each child's alpha while keeping its
/// hue intact.
#[derive(Component)]
pub struct FloatingNumberPart {
    base_color: Color,
}

/// Monotonically increasing counter used to derive a unique jitter seed per
/// spawn, so that multiple hits on the same target in the same frame still
/// produce distinct horizontal offsets.
static SPAWN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Cheap hash of a u64 to a float in [-1, 1]. SplitMix64 scramble step.
fn jitter_from_seed(seed: u64) -> f32 {
    let mut x = seed.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^=  x >> 31;
    let unit = (x as u32) as f32 / u32::MAX as f32; // [0, 1]
    unit * 2.0 - 1.0
}

/// Main fill color for a damage type.
fn fill_for(ty: DamageType) -> Color {
    match ty {
        DamageType::Physical => Color::srgb(1.00, 0.72, 0.30),
        DamageType::Arcane   => Color::srgb(0.70, 0.48, 1.00),
        DamageType::Nature   => Color::srgb(0.44, 0.88, 0.48),
    }
}

/// Brighter outline color for a damage type. Lerped 80% toward white so the
/// type identity is preserved but the contrast against dark backgrounds (and
/// against the fill itself) is high.
fn outline_for(ty: DamageType) -> Color {
    match ty {
        DamageType::Physical => Color::srgb(1.00, 0.95, 0.80),
        DamageType::Arcane   => Color::srgb(0.94, 0.88, 1.00),
        DamageType::Nature   => Color::srgb(0.88, 1.00, 0.90),
    }
}

/// Reads incoming `DamageNumberMsg`s and spawns a floating-number node for
/// each. Each spawn creates a parent container with five text children
/// (four-direction outline + main). `update_damage_numbers` places and fades
/// them every frame.
pub fn spawn_damage_numbers(
    mut link_query: Query<&mut MessageReceiver<DamageNumberMsg>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let Ok(mut receiver) = link_query.single_mut() else { return };
    let now = time.elapsed_secs();

    for msg in receiver.receive() {
        let fill = fill_for(msg.ty);
        let outline = outline_for(msg.ty);
        // Round to nearest integer for display — sub-integer damage looks noisy.
        let display = msg.amount.round().max(0.0) as i32;
        let label = display.to_string();
        // Crits spawn at the enlarged size; the updater shrinks them back down.
        let initial_font_size = if msg.is_crit { FONT_SIZE * CRIT_POP_START_SCALE } else { FONT_SIZE };
        let seed = SPAWN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let offset_x = jitter_from_seed(seed) * JITTER_X;

        commands
            .spawn((
                FloatingNumber {
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
                // Start hidden; `update_damage_numbers` will place it on the
                // next frame once we have a camera projection for the target.
                Visibility::Hidden,
            ))
            .with_children(|parent| {
                // Four outline copies at cardinal offsets, rendered first so
                // they sit behind the main copy.
                for (dx, dy) in [(-OUTLINE_PX, 0.0), (OUTLINE_PX, 0.0), (0.0, -OUTLINE_PX), (0.0, OUTLINE_PX)] {
                    parent.spawn((
                        FloatingNumberPart { base_color: outline },
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(dx),
                            top:  Val::Px(dy),
                            ..default()
                        },
                        Text::new(label.clone()),
                        TextFont { font_size: initial_font_size, ..default() },
                        TextColor(outline),
                    ));
                }
                // Main fill copy, rendered last so it draws on top of the outline.
                parent.spawn((
                    FloatingNumberPart { base_color: fill },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top:  Val::Px(0.0),
                        ..default()
                    },
                    Text::new(label),
                    TextFont { font_size: FONT_SIZE, ..default() },
                    TextColor(fill),
                ));
            });
    }
}

/// Each frame: advance each number's animation, project its world-space
/// position to screen, fade alpha across all parts. Despawns numbers past
/// their lifetime or whose target no longer exists.
pub fn update_damage_numbers(
    time: Res<Time>,
    camera_query: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    enemy_query: Query<&Transform, With<EnemyMarker>>,
    mut parent_query: Query<(Entity, &FloatingNumber, &mut Node, &mut Visibility, &Children)>,
    mut part_query: Query<(&FloatingNumberPart, &mut TextColor, &mut TextFont)>,
    mut commands: Commands,
) {
    let Ok((camera, cam_tf)) = camera_query.single() else { return };
    let now = time.elapsed_secs();

    for (entity, number, mut node, mut vis, children) in parent_query.iter_mut() {
        let elapsed = now - number.spawn_time;
        if elapsed >= LIFETIME {
            commands.entity(entity).despawn();
            continue;
        }

        let Ok(target_tf) = enemy_query.get(number.target) else {
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

        // Hold full alpha for the first HOLD_FRACTION of the lifetime, then
        // fade linearly to 0 over the remainder.
        let alpha = if progress < HOLD_FRACTION {
            1.0
        } else {
            1.0 - (progress - HOLD_FRACTION) / (1.0 - HOLD_FRACTION)
        };

        // Crit pop: shrink from enlarged size back to FONT_SIZE over
        // CRIT_POP_DURATION with an ease-out curve so the settle feels soft.
        // Non-crits hold at FONT_SIZE (already their spawn size).
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
