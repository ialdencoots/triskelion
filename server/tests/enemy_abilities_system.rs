//! System-level tests for `tick_enemy_abilities` and `tick_enemy_casts`.
//!
//! Drives the enemy attack loop end-to-end:
//! - **`tick_enemy_abilities`** ticks cooldowns, optionally fires an instant
//!   auto-attack, and optionally initiates a telegraphed special by inserting
//!   an `EnemyCast` component.
//! - **`tick_enemy_casts`** advances active casts and resolves them at
//!   `elapsed >= duration` per the cast's `AttackShape`.
//!
//! Both systems write `DamageEvent` and `DisruptionEvent`; we capture both
//! via sink systems for inspection.
//!
//! `has_los` returns `true` when the mob's instance isn't in the registry,
//! so we leave `InstanceRegistry` empty in tests — line-of-sight is always
//! clear, which lets us focus on the ability/cast control flow without
//! standing up terrain noise.

mod common;

use std::time::Duration;

use bevy::prelude::*;

use shared::components::combat::{
    AbilityKind, AttackShape, DamageType, Dead, DisruptionKind, Health,
};
use shared::components::enemy::{
    EnemyAbilityCooldowns, EnemyCast, EnemyMarker, EnemyPosition,
};
use shared::components::instance::InstanceId;
use shared::components::player::{PlayerId, PlayerPosition};
use shared::events::combat::{DamageEvent, DisruptionEvent};
use shared::instances::MobKind;

use server::systems::{
    combat::{ThreatEntry, ThreatList},
    enemy,
    instances::InstanceRegistry,
    mob_defs::MobKindComp,
};

#[derive(Resource, Default)]
struct CapturedDamage(Vec<DamageEvent>);

#[derive(Resource, Default)]
struct CapturedDisruption(Vec<DisruptionEvent>);

fn capture_damage(mut reader: MessageReader<DamageEvent>, mut sink: ResMut<CapturedDamage>) {
    for ev in reader.read() {
        sink.0.push(ev.clone());
    }
}

fn capture_disruption(
    mut reader: MessageReader<DisruptionEvent>,
    mut sink: ResMut<CapturedDisruption>,
) {
    for ev in reader.read() {
        sink.0.push(ev.clone());
    }
}

fn enemy_app() -> App {
    let mut app = common::minimal_app();
    app.init_resource::<InstanceRegistry>();
    app.add_message::<DamageEvent>();
    app.add_message::<DisruptionEvent>();
    app.init_resource::<CapturedDamage>();
    app.init_resource::<CapturedDisruption>();
    app.add_systems(
        Update,
        (
            // Producers run first; sinks drain same-tick events.
            (enemy::tick_enemy_abilities, enemy::tick_enemy_casts),
            (capture_damage, capture_disruption),
        )
            .chain(),
    );
    app
}

/// Spawn a player suitable for being targeted by mob abilities.
fn spawn_player(app: &mut App, pid: u64, instance_id: u32, pos: (f32, f32, f32)) -> Entity {
    let (x, y, z) = pos;
    app.world_mut()
        .spawn((
            PlayerId(pid),
            PlayerPosition::new(x, y, z),
            InstanceId(instance_id),
            Health::new(100.0),
        ))
        .id()
}

/// Spawn a mob with all the components the ability/cast systems query for.
fn spawn_mob(
    app: &mut App,
    kind: MobKind,
    instance_id: u32,
    pos: (f32, f32, f32),
    threats: &[(Entity, f32)],
) -> Entity {
    let (x, y, z) = pos;
    let specials_count = match kind {
        MobKind::Troll | MobKind::CrystalGolemLord => 1,
        _ => 0,
    };
    let mut threat_list = ThreatList::default();
    for (e, t) in threats {
        threat_list.entries.push(ThreatEntry { player_entity: *e, threat: *t });
    }

    app.world_mut()
        .spawn((
            EnemyMarker,
            MobKindComp(kind),
            EnemyPosition::new(x, y, z),
            InstanceId(instance_id),
            threat_list,
            EnemyAbilityCooldowns {
                auto_cd: 0.0,
                specials_cd: vec![0.0; specials_count],
            },
        ))
        .id()
}

// ── tick_enemy_abilities: auto-attack ───────────────────────────────────────

#[test]
fn auto_attack_fires_at_top_threat_target_in_range() {
    let mut app = enemy_app();
    let player = spawn_player(&mut app, 1, 0, (1.0, 0.0, 0.0));
    let mob = spawn_mob(&mut app, MobKind::Goblin, 0, (0.0, 0.0, 0.0), &[(player, 10.0)]);

    common::advance(&mut app, Duration::from_millis(16));

    let damage = &app.world().resource::<CapturedDamage>().0;
    let disruption = &app.world().resource::<CapturedDisruption>().0;
    assert_eq!(damage.len(), 1, "exactly one auto-attack DamageEvent");
    assert_eq!(disruption.len(), 1, "exactly one auto-attack DisruptionEvent");

    let dmg = &damage[0];
    assert_eq!(dmg.attacker, mob);
    assert_eq!(dmg.target, player);
    assert_eq!(dmg.base, 8.0); // MeleeAuto damage
    assert_eq!(dmg.ty, DamageType::Physical);
    assert_eq!(dmg.quality, 1.0);

    let disr = &disruption[0];
    assert_eq!(disr.target, player);
    assert!(matches!(disr.profile.kind, DisruptionKind::Spike));
    assert!((disr.profile.magnitude - 0.15).abs() < 1e-5);

    let cd = app.world().entity(mob).get::<EnemyAbilityCooldowns>().unwrap();
    assert!(cd.auto_cd > 1.0, "auto_cd should reset to MeleeAuto cooldown (1.2s)");
}

#[test]
fn auto_attack_skipped_when_target_out_of_range() {
    let mut app = enemy_app();
    // MeleeAuto reach is max(melee_range=1.5, range=2.0) = 2.0.
    let player = spawn_player(&mut app, 1, 0, (10.0, 0.0, 0.0));
    spawn_mob(&mut app, MobKind::Goblin, 0, (0.0, 0.0, 0.0), &[(player, 10.0)]);

    common::advance(&mut app, Duration::from_millis(16));

    assert!(
        app.world().resource::<CapturedDamage>().0.is_empty(),
        "no damage when target is out of melee reach",
    );
}

#[test]
fn auto_attack_skipped_during_active_cast() {
    let mut app = enemy_app();
    let player = spawn_player(&mut app, 1, 0, (1.0, 0.0, 0.0));
    let mob = spawn_mob(&mut app, MobKind::Goblin, 0, (0.0, 0.0, 0.0), &[(player, 10.0)]);
    // Mid-cast: any EnemyCast component blocks auto-attack and special init.
    app.world_mut().entity_mut(mob).insert(EnemyCast {
        ability: AbilityKind::MeleeAuto,
        shape: AttackShape::Single,
        target: 1,
        aim_x: 1.0,
        aim_y: 0.0,
        aim_z: 0.0,
        elapsed: 0.0,
        duration: 5.0, // long enough to not resolve this tick
    });

    common::advance(&mut app, Duration::from_millis(16));

    // The cast is still in flight (elapsed=0.016 < duration=5.0). The auto
    // path was skipped due to the active cast.
    assert!(
        app.world().resource::<CapturedDamage>().0.is_empty(),
        "auto-attack must be suppressed during a cast",
    );
}

#[test]
fn auto_attack_cooldown_decrements_each_tick() {
    let mut app = enemy_app();
    let player = spawn_player(&mut app, 1, 0, (1.0, 0.0, 0.0));
    let mob = spawn_mob(&mut app, MobKind::Goblin, 0, (0.0, 0.0, 0.0), &[(player, 10.0)]);
    // Pre-loaded cooldown — auto must wait it out.
    app.world_mut()
        .entity_mut(mob)
        .get_mut::<EnemyAbilityCooldowns>()
        .unwrap()
        .auto_cd = 1.0;

    common::advance(&mut app, Duration::from_millis(500));

    let cd = app.world().entity(mob).get::<EnemyAbilityCooldowns>().unwrap();
    assert!(
        (cd.auto_cd - 0.5).abs() < 1e-3,
        "auto_cd should drop by ~dt, was {}",
        cd.auto_cd,
    );
    assert!(
        app.world().resource::<CapturedDamage>().0.is_empty(),
        "no auto-attack while cooling down",
    );
}

// ── tick_enemy_abilities: special initiation ────────────────────────────────

#[test]
fn special_initiates_cast_when_ready_and_in_range() {
    let mut app = enemy_app();
    // Troll has GroundSlam (range 8.0, telegraph 1.5s) as its only special.
    // Place player 5m away — within GroundSlam range, outside auto reach.
    let player = spawn_player(&mut app, 1, 0, (5.0, 0.0, 0.0));
    let mob = spawn_mob(&mut app, MobKind::Troll, 0, (0.0, 0.0, 0.0), &[(player, 10.0)]);

    common::advance(&mut app, Duration::from_millis(16));

    let cast = app
        .world()
        .entity(mob)
        .get::<EnemyCast>()
        .expect("Troll should have inserted a GroundSlam cast");
    assert!(matches!(cast.ability, AbilityKind::GroundSlam));
    assert!(matches!(cast.shape, AttackShape::Radius { .. }));
    assert_eq!(cast.target, 1, "cast.target should be locked to player PID");
    assert!((cast.duration - 1.5).abs() < 1e-5);
    assert_eq!(cast.elapsed, 0.0, "cast just started");

    let cd = app.world().entity(mob).get::<EnemyAbilityCooldowns>().unwrap();
    assert!((cd.specials_cd[0] - 8.0).abs() < 1e-5, "specials_cd reset to GroundSlam cooldown");
}

// ── tick_enemy_casts: resolution ────────────────────────────────────────────

#[test]
fn cast_advances_elapsed_each_tick() {
    let mut app = enemy_app();
    let player = spawn_player(&mut app, 1, 0, (5.0, 0.0, 0.0));
    let mob = spawn_mob(&mut app, MobKind::Troll, 0, (0.0, 0.0, 0.0), &[(player, 10.0)]);
    app.world_mut().entity_mut(mob).insert(EnemyCast {
        ability: AbilityKind::GroundSlam,
        shape: AttackShape::Radius { radius: 2.5 },
        target: 1,
        aim_x: 5.0,
        aim_y: 0.0,
        aim_z: 0.0,
        elapsed: 0.0,
        duration: 1.5,
    });

    common::advance(&mut app, Duration::from_millis(100));

    let cast = app.world().entity(mob).get::<EnemyCast>().unwrap();
    assert!(
        (cast.elapsed - 0.1).abs() < 1e-3,
        "elapsed should advance by dt, was {}",
        cast.elapsed,
    );
    assert!(
        app.world().resource::<CapturedDamage>().0.is_empty(),
        "cast should not resolve before duration elapses",
    );
}

#[test]
fn radius_cast_resolves_to_players_inside_radius() {
    let mut app = enemy_app();
    let inside = spawn_player(&mut app, 1, 0, (5.0, 0.0, 0.0));   // 0.0m from aim
    let edge   = spawn_player(&mut app, 2, 0, (7.4, 0.0, 0.0));   // 2.4m from aim
    let outside = spawn_player(&mut app, 3, 0, (10.0, 0.0, 0.0)); // 5.0m from aim
    let mob = spawn_mob(
        &mut app,
        MobKind::Troll,
        0,
        (0.0, 0.0, 0.0),
        &[(inside, 10.0)],
    );
    app.world_mut().entity_mut(mob).insert(EnemyCast {
        ability: AbilityKind::GroundSlam,
        shape: AttackShape::Radius { radius: 2.5 },
        target: 1,
        aim_x: 5.0,
        aim_y: 0.0,
        aim_z: 0.0,
        elapsed: 1.49,
        duration: 1.5,
    });

    // One tick > 0.01s pushes elapsed past duration → resolve fires.
    common::advance(&mut app, Duration::from_millis(20));

    let damage = &app.world().resource::<CapturedDamage>().0;
    let hit_targets: Vec<Entity> = damage.iter().map(|e| e.target).collect();
    assert!(hit_targets.contains(&inside), "inside player should be hit");
    assert!(hit_targets.contains(&edge), "edge player (within 2.5m) should be hit");
    assert!(!hit_targets.contains(&outside), "outside player must not be hit");
    assert!(
        app.world().entity(mob).get::<EnemyCast>().is_none(),
        "EnemyCast should be removed after resolve",
    );
}

#[test]
fn single_cast_resolves_only_to_locked_target() {
    let mut app = enemy_app();
    let locked = spawn_player(&mut app, 1, 0, (1.0, 0.0, 0.0));
    let bystander = spawn_player(&mut app, 2, 0, (1.1, 0.0, 0.0));
    let mob = spawn_mob(
        &mut app,
        MobKind::Goblin,
        0,
        (0.0, 0.0, 0.0),
        &[(locked, 10.0)],
    );
    app.world_mut().entity_mut(mob).insert(EnemyCast {
        ability: AbilityKind::MeleeAuto,
        shape: AttackShape::Single,
        target: 1, // PlayerId(1) = locked
        aim_x: 1.0,
        aim_y: 0.0,
        aim_z: 0.0,
        elapsed: 0.99,
        duration: 1.0,
    });

    common::advance(&mut app, Duration::from_millis(20));

    let damage = &app.world().resource::<CapturedDamage>().0;
    assert_eq!(damage.len(), 1, "single shape should hit exactly one target");
    assert_eq!(damage[0].target, locked);
    assert_ne!(damage[0].target, bystander, "bystander must not be hit");
}

// ── Death gating ────────────────────────────────────────────────────────────

#[test]
fn dead_mob_does_not_auto_attack() {
    let mut app = enemy_app();
    let player = spawn_player(&mut app, 1, 0, (1.0, 0.0, 0.0));
    let mob = spawn_mob(&mut app, MobKind::Goblin, 0, (0.0, 0.0, 0.0), &[(player, 10.0)]);
    app.world_mut().entity_mut(mob).insert(Dead);

    common::advance(&mut app, Duration::from_millis(16));

    assert!(
        app.world().resource::<CapturedDamage>().0.is_empty(),
        "a Dead mob must not fire its auto-attack",
    );
}

#[test]
fn mob_skips_dead_player_in_threat_list() {
    // Threat list points at a corpse — Without<Dead> on the player query
    // turns the entry's get() into Err so it's filtered out, and no
    // DamageEvent is emitted. (Pre-existing behavior of top_threat_target:
    // when the top entry doesn't resolve to a valid live player, the system
    // returns None for this tick rather than falling back to second-highest.
    // That fallback would be a separate, unrelated improvement.)
    let mut app = enemy_app();
    let corpse = spawn_player(&mut app, 1, 0, (1.0, 0.0, 0.0));
    app.world_mut().entity_mut(corpse).insert(Dead);
    spawn_mob(
        &mut app,
        MobKind::Goblin,
        0,
        (0.0, 0.0, 0.0),
        &[(corpse, 100.0)],
    );

    common::advance(&mut app, Duration::from_millis(16));

    assert!(
        app.world().resource::<CapturedDamage>().0.is_empty(),
        "a dead top-threat target must not be auto-attacked",
    );
}
