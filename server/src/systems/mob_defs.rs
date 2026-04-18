use bevy::prelude::*;
use lightyear::prelude::Replicate;
use lightyear::connection::network_target::NetworkTarget;

use shared::components::combat::ReplicatedThreatList;
use shared::components::enemy::{BossMarker, EnemyMarker, EnemyName, EnemyPosition, EnemyVelocity, MobTarget};
use shared::components::instance::InstanceId;
use shared::instances::MobKind;

use super::combat::ThreatList;

/// Server-only AI behavior (never replicated).
#[derive(Component)]
pub enum MobBehavior {
    /// Lissajous-style patrol that aggros when a player enters `aggro_range`.
    /// De-aggros and resumes wandering when the player exceeds 1.5× aggro_range.
    Patrol {
        phase: f32,
        aggro_range: f32,
        melee_range: f32,
        aggroed: bool,
    },
    /// Always chases the nearest player in the same instance within `aggro_range`.
    /// Stop moving when within `melee_range`.
    Aggro { aggro_range: f32, melee_range: f32 },
}

fn default_behavior_for_kind(kind: MobKind, phase: f32) -> MobBehavior {
    match kind {
        MobKind::Goblin => MobBehavior::Patrol {
            phase,
            aggro_range: 8.0,
            melee_range: 1.5,
            aggroed: false,
        },
        MobKind::Orc => MobBehavior::Patrol {
            phase,
            aggro_range: 10.0,
            melee_range: 1.8,
            aggroed: false,
        },
        MobKind::Troll => MobBehavior::Patrol {
            phase,
            aggro_range: 7.0,
            melee_range: 2.0,
            aggroed: false,
        },
        MobKind::CrystalGolem     => MobBehavior::Aggro { aggro_range: 12.0, melee_range: 2.0 },
        MobKind::CrystalGolemLord => MobBehavior::Aggro { aggro_range: 16.0, melee_range: 3.0 },
    }
}

/// Y offset from terrain floor to capsule center so the capsule base sits just
/// above the ground. Formula: radius + half_cylinder_height + 0.2 gap.
pub fn floor_offset_for_kind(kind: MobKind) -> f32 {
    match kind {
        MobKind::CrystalGolemLord => 2.2, // radius 1.0 + half-cyl 1.0 + gap 0.2
        _                         => 1.1, // radius 0.4 + half-cyl 0.5 + gap 0.2
    }
}

pub fn mob_name_for_kind(kind: MobKind) -> &'static str {
    match kind {
        MobKind::Goblin           => "Goblin",
        MobKind::Orc              => "Orc",
        MobKind::Troll            => "Troll",
        MobKind::CrystalGolem     => "Crystal Golem",
        MobKind::CrystalGolemLord => "Crystal Golem Lord",
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
    let mut entity = commands.spawn((
        Name::new(name),
        EnemyMarker,
        EnemyName(name.to_string()),
        EnemyPosition::new(x, y, z),
        EnemyVelocity { vx: 0.0, vz: 0.0 },
        behavior,
        ThreatList::default(),
        ReplicatedThreatList::default(),
        MobTarget::default(),
        InstanceId(instance_id),
        Replicate::to_clients(NetworkTarget::All),
    ));
    if kind == MobKind::CrystalGolemLord {
        entity.insert(BossMarker);
    }
    entity.id()
}

/// Deterministic scatter: returns an XZ offset for mob `mob_idx` within a pack
/// at `node_idx`, scattered within `spread` world units.
pub fn scatter_offset(node_idx: usize, mob_idx: usize, spread: f32) -> (f32, f32) {
    let t = (node_idx * 31 + mob_idx * 17) as f32;
    let angle = t * 2.399; // golden-angle-ish step
    let r = spread * ((mob_idx as f32 + 1.0) / 9.0).min(1.0);
    (r * angle.cos(), r * angle.sin())
}
