use bevy::prelude::*;
use lightyear::prelude::Replicate;
use lightyear::connection::network_target::NetworkTarget;

use shared::components::enemy::{EnemyMarker, EnemyName, EnemyPosition, EnemyVelocity};
use shared::components::instance::InstanceId;
use shared::instances::MobKind;

/// Server-only AI behavior (never replicated).
#[derive(Component)]
pub enum MobBehavior {
    /// Lissajous-style wandering path unique to each mob.
    Wander { phase: f32 },
    /// Chase the nearest player in the same instance within `aggro_range`.
    /// Stop moving when within `melee_range`.
    Aggro { aggro_range: f32, melee_range: f32 },
}

/// Backwards-compat alias used in existing call sites.
pub type EnemyWalkState = MobBehavior;

impl MobBehavior {
    pub fn wander(phase: f32) -> Self {
        MobBehavior::Wander { phase }
    }
}

fn default_behavior_for_kind(kind: MobKind, phase: f32) -> MobBehavior {
    match kind {
        MobKind::Goblin       => MobBehavior::Wander { phase },
        MobKind::Orc          => MobBehavior::Wander { phase },
        MobKind::Troll        => MobBehavior::Wander { phase },
        MobKind::CrystalGolem => MobBehavior::Aggro { aggro_range: 12.0, melee_range: 2.0 },
    }
}

pub fn mob_name_for_kind(kind: MobKind) -> &'static str {
    match kind {
        MobKind::Goblin       => "Goblin",
        MobKind::Orc          => "Orc",
        MobKind::Troll        => "Troll",
        MobKind::CrystalGolem => "Crystal Golem",
    }
}

/// Spawn one mob.  `y` is the pre-computed world-space Y (caller already
/// applied `sample_height + 1.1`).  Returns the spawned entity.
pub fn spawn_mob(
    commands: &mut Commands,
    kind: MobKind,
    x: f32,
    y: f32,
    z: f32,
    phase: f32,
    instance_id: u32,
) -> Entity {
    let name = mob_name_for_kind(kind);
    let behavior = default_behavior_for_kind(kind, phase);
    commands.spawn((
        Name::new(name),
        EnemyMarker,
        EnemyName(name.to_string()),
        EnemyPosition::new(x, y, z),
        EnemyVelocity { vx: 0.0, vz: 0.0 },
        behavior,
        InstanceId(instance_id),
        Replicate::to_clients(NetworkTarget::All),
    )).id()
}

/// Deterministic scatter: returns an XZ offset for mob `mob_idx` within a pack
/// at `node_idx`, scattered within `spread` world units.
pub fn scatter_offset(node_idx: usize, mob_idx: usize, spread: f32) -> (f32, f32) {
    let t = (node_idx * 31 + mob_idx * 17) as f32;
    let angle = t * 2.399; // golden-angle-ish step
    let r = spread * ((mob_idx as f32 + 1.0) / 9.0).min(1.0);
    (r * angle.cos(), r * angle.sin())
}
