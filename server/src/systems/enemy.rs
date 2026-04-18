use bevy::prelude::*;
use lightyear::prelude::server::*;

use shared::components::enemy::{EnemyPosition, EnemyVelocity};
use shared::components::instance::InstanceId;
use shared::components::player::{PlayerId, PlayerPosition};
use shared::instances::{find_def, layout_sdf, sample_height, InstanceKind};

use super::instances::{create_instance, InstanceRegistry};
use super::mob_defs::MobBehavior;

/// Fires when the NetcodeServer reports `Started`.
/// Creates the group-0 overworld instance and populates it with mobs.
pub fn on_server_started(
    trigger: On<Add, Started>,
    server_q: Query<(), With<NetcodeServer>>,
    mut reg: ResMut<InstanceRegistry>,
    _commands: Commands,
) {
    let entity = trigger.event_target();
    if server_q.get(entity).is_err() {
        info!(
            "[SERVER] on_server_started: Started on {entity:?} is NOT a NetcodeServer — skipping"
        );
        return;
    }
    // Create the overworld instance so group_instances is populated before any
    // client connects.  Mobs are populated lazily in process_spawn_requests
    // when the first player joins, so they can be spawned with the correct
    // NetworkTarget::Only([first_client]) from the start.
    info!("[SERVER] on_server_started: creating group-0 Overworld instance");
    create_instance(InstanceKind::Overworld, 0, &mut reg);
}

const SPEED: f32 = 2.5;
// Radius within which aggro'd mobs push each other apart.
const SEPARATION_RADIUS: f32 = 3.5;
// Maximum contribution of the separation force relative to the unit chase direction.
const SEPARATION_STRENGTH: f32 = 1.2;

/// Advances all mob AI each fixed tick.
/// Dispatches on `MobBehavior`: Wander uses Lissajous paths, Aggro chases
/// the nearest player in the same instance.
pub fn tick_enemy_walk(
    time: Res<Time>,
    reg: Res<InstanceRegistry>,
    mut mob_query: Query<(&mut EnemyPosition, &mut EnemyVelocity, &mut MobBehavior, &InstanceId)>,
    player_query: Query<(&PlayerPosition, &InstanceId), With<PlayerId>>,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();

    let prev = (t - dt) as u32;
    let curr = t as u32;
    if curr != prev && curr % 5 == 0 {
        let count = mob_query.iter().count();
        info!("[SERVER] tick_enemy_walk: {count} mobs at t={t:.1}s");
    }

    // Snapshot positions for separation calculations — collected before any
    // movement so all mobs react to the same frame state.
    let mob_positions: Vec<(Vec2, u32)> = mob_query
        .iter()
        .map(|(pos, _, _, iid)| (Vec2::new(pos.x, pos.z), iid.0))
        .collect();

    for (mut pos, mut vel, mut behavior, iid) in mob_query.iter_mut() {
        match &mut *behavior {
            MobBehavior::Patrol { phase, aggro_range, melee_range, aggroed } => {
                // Find nearest player in the same instance.
                let nearest = player_query
                    .iter()
                    .filter(|(_, pid)| pid.0 == iid.0)
                    .map(|(ppos, _)| {
                        let dx = ppos.x - pos.x;
                        let dz = ppos.z - pos.z;
                        let dist = (dx * dx + dz * dz).sqrt();
                        (dist, dx, dz)
                    })
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                // Aggro/de-aggro with hysteresis: break at 1.5× the aggro range
                // so mobs don't flicker when the player sits on the threshold.
                let leash_range = *aggro_range * 1.5;
                match nearest {
                    Some((dist, _, _)) if !*aggroed && dist <= *aggro_range => {
                        *aggroed = true;
                    }
                    Some((dist, _, _)) if *aggroed && dist > leash_range => {
                        *aggroed = false;
                    }
                    None => {
                        *aggroed = false;
                    }
                    _ => {}
                }

                if *aggroed {
                    let my_xz = Vec2::new(pos.x, pos.z);
                    let mut sep = Vec2::ZERO;
                    for (other_xz, other_iid) in &mob_positions {
                        if *other_iid != iid.0 { continue; }
                        let diff = my_xz - *other_xz;
                        let dist = diff.length();
                        if dist > 0.01 && dist < SEPARATION_RADIUS {
                            sep += diff.normalize() * (1.0 - dist / SEPARATION_RADIUS);
                        }
                    }
                    let sep = sep.clamp_length_max(1.0) * SEPARATION_STRENGTH;

                    if let Some((dist, dx, dz)) = nearest {
                        if dist > *melee_range {
                            let chase = Vec2::new(dx, dz).normalize_or_zero();
                            let dir = (chase + sep).normalize_or_zero();
                            vel.vx = dir.x * SPEED;
                            vel.vz = dir.y * SPEED;
                        } else {
                            let spread = sep.normalize_or_zero();
                            vel.vx = spread.x * SPEED * 0.5;
                            vel.vz = spread.y * SPEED * 0.5;
                        }
                    } else {
                        vel.vx = 0.0;
                        vel.vz = 0.0;
                    }
                } else {
                    // Resume Lissajous patrol.
                    let dx = (t * 0.4 + *phase).sin();
                    let dz = (t * 0.3 + *phase * 1.7).cos();
                    let dir = Vec2::new(dx, dz).normalize_or_zero();
                    vel.vx = dir.x * SPEED;
                    vel.vz = dir.y * SPEED;
                }
            }
            MobBehavior::Aggro { aggro_range, melee_range } => {
                // Find nearest player in the same instance.
                let nearest = player_query
                    .iter()
                    .filter(|(_, pid)| pid.0 == iid.0)
                    .map(|(ppos, _)| {
                        let dx = ppos.x - pos.x;
                        let dz = ppos.z - pos.z;
                        let dist = (dx * dx + dz * dz).sqrt();
                        (dist, dx, dz)
                    })
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                // Separation force: push away from nearby mob neighbours so
                // the pack fans out around the player rather than stacking.
                let my_xz = Vec2::new(pos.x, pos.z);
                let mut sep = Vec2::ZERO;
                for (other_xz, other_iid) in &mob_positions {
                    if *other_iid != iid.0 {
                        continue;
                    }
                    let diff = my_xz - *other_xz;
                    let dist = diff.length();
                    if dist > 0.01 && dist < SEPARATION_RADIUS {
                        sep += diff.normalize() * (1.0 - dist / SEPARATION_RADIUS);
                    }
                }
                // Clamp so a dense crowd doesn't let separation overpower the chase.
                let sep = sep.clamp_length_max(1.0) * SEPARATION_STRENGTH;

                if let Some((dist, dx, dz)) = nearest {
                    if dist <= *aggro_range {
                        if dist > *melee_range {
                            // Chase + spread sideways when closing in.
                            let chase = Vec2::new(dx, dz).normalize_or_zero();
                            let dir = (chase + sep).normalize_or_zero();
                            vel.vx = dir.x * SPEED;
                            vel.vz = dir.y * SPEED;
                        } else {
                            // At melee range: shuffle sideways to spread around
                            // the player rather than freezing in a pile.
                            let spread = sep.normalize_or_zero();
                            vel.vx = spread.x * SPEED * 0.5;
                            vel.vz = spread.y * SPEED * 0.5;
                        }
                    } else {
                        vel.vx = 0.0;
                        vel.vz = 0.0;
                    }
                } else {
                    vel.vx = 0.0;
                    vel.vz = 0.0;
                }
            }
        }

        let new_x = pos.x + vel.vx * dt;
        let new_z = pos.z + vel.vz * dt;

        if let Some(live) = reg.instances.get(&iid.0) {
            let def = find_def(live.kind);

            // For layout instances confine movement to corridors/rooms.
            // Try full move first, then axis-by-axis sliding, then stop.
            let (fx, fz) = if def.use_layout_terrain {
                if layout_sdf(new_x, new_z, def) <= 0.0 {
                    (new_x, new_z)
                } else if layout_sdf(new_x, pos.z, def) <= 0.0 {
                    (new_x, pos.z)
                } else if layout_sdf(pos.x, new_z, def) <= 0.0 {
                    (pos.x, new_z)
                } else {
                    (pos.x, pos.z)
                }
            } else {
                (new_x, new_z)
            };

            pos.x = fx;
            pos.z = fz;
            pos.y = sample_height(&live.noise, fx, fz, &def.terrain) + 1.1;
        } else {
            pos.x = new_x;
            pos.z = new_z;
        }
    }
}
