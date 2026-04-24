use bevy::prelude::*;
use lightyear::prelude::Replicate;
use lightyear::connection::network_target::NetworkTarget;

use shared::components::combat::{
    AbilityKind, AbilityStats, AttackShape, DamageType, DisruptionKind, DisruptionProfile,
    Health, ReplicatedThreatList, Resistances, TargetSelector,
};
use shared::components::enemy::{
    BossMarker, EnemyAbilityCooldowns, EnemyMarker, EnemyName, EnemyPosition, EnemyVelocity,
    MobTarget,
};
use shared::components::instance::InstanceId;
use shared::instances::MobKind;
use shared::settings::{BOSS_FLOAT_HEIGHT, PLAYER_FLOAT_HEIGHT};

use super::combat::ThreatList;

/// Server-only component that tags each spawned mob with its `MobKind` so
/// ability tick systems can look up `stats_for_kind` without replicating
/// the kind enum to clients.
#[derive(Component, Clone, Copy, Debug)]
pub struct MobKindComp(pub MobKind);

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

pub struct MobStats {
    pub name:         &'static str,
    pub max_health:   f32,
    /// Y offset from terrain floor to capsule center (radius + half_cyl + 0.2 gap).
    pub floor_offset: f32,
    pub aggro_range:  f32,
    pub melee_range:  f32,
    pub patrol:       bool,
    /// Per-type damage reduction in [0.0, 0.75]. Clamped by `Resistances::new`.
    pub resist_physical: f32,
    pub resist_arcane:   f32,
    pub resist_nature:   f32,
    /// Always-present auto-attack ability. All mobs have one.
    pub auto_attack:  AbilityKind,
    /// Optional special abilities. Index-aligned with `EnemyAbilityCooldowns.specials_cd`.
    pub specials:     &'static [AbilityKind],
}

pub fn stats_for_kind(kind: MobKind) -> MobStats {
    match kind {
        MobKind::Goblin           => MobStats { name: "Goblin",             max_health:    80.0, floor_offset: PLAYER_FLOAT_HEIGHT, aggro_range:  8.0, melee_range: 1.5, patrol: true,  resist_physical: 0.0, resist_arcane: 0.0, resist_nature: 0.0, auto_attack: AbilityKind::MeleeAuto, specials: &[] },
        MobKind::Orc              => MobStats { name: "Orc",                max_health:   120.0, floor_offset: PLAYER_FLOAT_HEIGHT, aggro_range: 10.0, melee_range: 1.8, patrol: true,  resist_physical: 0.2, resist_arcane: 0.0, resist_nature: 0.0, auto_attack: AbilityKind::MeleeAuto, specials: &[] },
        MobKind::Troll            => MobStats { name: "Troll",              max_health:   200.0, floor_offset: PLAYER_FLOAT_HEIGHT, aggro_range:  7.0, melee_range: 2.0, patrol: true,  resist_physical: 0.3, resist_arcane: 0.1, resist_nature: 0.0, auto_attack: AbilityKind::MeleeAuto, specials: &[AbilityKind::GroundSlam] },
        MobKind::CrystalGolem     => MobStats { name: "Crystal Golem",      max_health:   300.0, floor_offset: PLAYER_FLOAT_HEIGHT, aggro_range: 12.0, melee_range: 2.0, patrol: false, resist_physical: 0.5, resist_arcane: 0.0, resist_nature: 0.3, auto_attack: AbilityKind::MeleeAuto, specials: &[] },
        MobKind::CrystalGolemLord => MobStats { name: "Crystal Golem Lord", max_health:  1000.0, floor_offset: BOSS_FLOAT_HEIGHT,   aggro_range: 16.0, melee_range: 3.0, patrol: false, resist_physical: 0.6, resist_arcane: 0.2, resist_nature: 0.4, auto_attack: AbilityKind::MeleeAuto, specials: &[AbilityKind::GroundSlam] },
    }
}

/// Static parameter table for every enemy ability. Kept here (server-side)
/// because clients only need the ability's identity to pick the right
/// telegraph geometry — damage/disruption tuning is server authority.
pub fn stats_for_ability(kind: AbilityKind) -> AbilityStats {
    match kind {
        AbilityKind::MeleeAuto => AbilityStats {
            telegraph:      0.0,
            cooldown:       1.2,
            foiled_cd:      0.4,
            interrupted_cd: 0.4,
            range:          2.0,
            shape:          AttackShape::Single,
            selector:       TargetSelector::TopThreat,
            damage:         8.0,
            ty:             DamageType::Physical,
            disruption:     DisruptionProfile { kind: DisruptionKind::Spike, magnitude: 0.15 },
        },
        AbilityKind::GroundSlam => AbilityStats {
            telegraph:      1.5,
            cooldown:       8.0,
            foiled_cd:      3.0,
            interrupted_cd: 12.0,
            range:          8.0,
            shape:          AttackShape::Radius { radius: 2.5 },
            selector:       TargetSelector::TopThreat,
            damage:         25.0,
            ty:             DamageType::Physical,
            disruption:     DisruptionProfile { kind: DisruptionKind::Spike, magnitude: 0.9 },
        },
    }
}

/// Y offset from terrain floor to capsule center so the capsule base sits just
/// above the ground. Formula: radius + half_cylinder_height + 0.2 gap.
pub fn floor_offset_for_kind(kind: MobKind) -> f32 {
    stats_for_kind(kind).floor_offset
}

/// Spawn one mob.  `y` is the pre-computed world-space Y (caller already
/// applied `sample_height + floor_offset_for_kind`).  Returns the spawned entity.
pub fn spawn_mob(
    commands: &mut Commands,
    kind: MobKind,
    x: f32,
    y: f32,
    z: f32,
    phase: f32,
    instance_id: u32,
) -> Entity {
    let stats = stats_for_kind(kind);
    let name = stats.name;
    let max_hp = stats.max_health;
    let behavior = if stats.patrol {
        MobBehavior::Patrol { phase, aggro_range: stats.aggro_range, melee_range: stats.melee_range, aggroed: false }
    } else {
        MobBehavior::Aggro { aggro_range: stats.aggro_range, melee_range: stats.melee_range }
    };
    let cooldowns = EnemyAbilityCooldowns {
        auto_cd:     0.0,
        specials_cd: vec![0.0; stats.specials.len()],
    };
    let mut entity = commands.spawn((
        Name::new(name),
        EnemyMarker,
        EnemyName(name.to_string()),
        EnemyPosition::new(x, y, z),
        EnemyVelocity { vx: 0.0, vz: 0.0 },
        Health { current: max_hp, max: max_hp },
        Resistances::new(stats.resist_physical, stats.resist_arcane, stats.resist_nature),
        behavior,
        ThreatList::default(),
        ReplicatedThreatList::default(),
        MobTarget::default(),
        cooldowns,
        MobKindComp(kind),
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
