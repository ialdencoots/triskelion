use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::server::*;

use shared::components::enemy::{EnemyMarker, EnemyPosition};
use shared::terrain;

/// Server-only AI state.  Never replicated.
#[derive(Component)]
pub struct EnemyWalkState {
    /// Per-enemy phase offset so multiple enemies wander in different paths.
    phase: f32,
}

/// Fires when the Lightyear server finishes starting (Started component added).
/// Spawning here guarantees Replicate::on_insert finds an active server and
/// populates per_sender_state so enemies replicate to connecting clients.
pub fn on_server_started(
    trigger: On<Add, Started>,
    server_q: Query<(), With<NetcodeServer>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if server_q.get(entity).is_err() {
        info!("[SERVER] on_server_started: Started added to {entity:?} which is NOT a NetcodeServer — skipping");
        return;
    }
    info!("[SERVER] on_server_started: Started on NetcodeServer {entity:?} — spawning enemies");
    for (i, (x, z)) in [(5.0_f32, -4.0_f32), (-8.0, 6.0), (12.0, 3.0)].iter().enumerate() {
        let y = terrain::height_at(*x, *z) + 1.1;
        let enemy = commands.spawn((
            Name::new(format!("Enemy{}", i + 1)),
            EnemyMarker,
            EnemyPosition::new(*x, y, *z),
            EnemyWalkState { phase: i as f32 * 2.1 },
            Replicate::to_clients(NetworkTarget::All),
        )).id();
        info!("[SERVER] Spawned Enemy{} as {enemy:?} at ({x:.1}, {y:.1}, {z:.1})", i + 1);
    }
}

pub fn tick_enemy_walk(time: Res<Time>, mut query: Query<(&mut EnemyPosition, &EnemyWalkState)>) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();

    // Log enemy count every 5 seconds.
    let prev = (t - dt) as u32;
    let curr = t as u32;
    if curr != prev && curr % 5 == 0 {
        let count = query.iter().count();
        info!("[SERVER] tick_enemy_walk: {count} enemies at t={t:.1}s");
    }
    const SPEED: f32 = 2.5;

    for (mut pos, walk) in query.iter_mut() {
        // Lissajous-style wandering: two sine waves at different frequencies
        // give a natural, looping path unique to each enemy's phase offset.
        let dx = (t * 0.4 + walk.phase).sin();
        let dz = (t * 0.3 + walk.phase * 1.7).cos();
        let dir = Vec2::new(dx, dz).normalize_or_zero();

        pos.x += dir.x * SPEED * dt;
        pos.z += dir.y * SPEED * dt;
        pos.y = terrain::height_at(pos.x, pos.z) + 1.1;
    }
}
