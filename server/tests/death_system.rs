//! System-level tests for `apply_death_transition`.
//!
//! Spawns entities with HP at zero (or sets it after the fact), runs the
//! transition system, and asserts the corpse-cleanup invariants the rest of
//! the combat code relies on:
//!   - `Dead` marker present
//!   - `DamageOverTimes` removed
//!   - `ThreatList.entries` cleared (mob branch)
//!   - velocity zeroed (mob and player branches)
//!   - in-flight `EnemyCast` removed
//!   - already-dead entities skipped (no double-processing)

mod common;

use bevy::prelude::*;

use shared::components::combat::{
    AbilityKind, AttackShape, CombatState, Dead, DamageType, Health,
};
use shared::components::enemy::{EnemyCast, EnemyMarker, EnemyVelocity};
use shared::components::minigame::arc::ArcState;
use shared::components::player::{PlayerId, PlayerVelocity, RoleStance};

use server::systems::combat::{self, DamageOverTime, DamageOverTimes, ThreatEntry, ThreatList};

fn death_app() -> App {
    let mut app = common::minimal_app();
    app.add_systems(Update, combat::apply_death_transition);
    app
}

fn fake_cast(target: u64) -> EnemyCast {
    EnemyCast {
        ability: AbilityKind::MeleeAuto,
        shape: AttackShape::Single,
        target,
        aim_x: 0.0,
        aim_y: 0.0,
        aim_z: 0.0,
        elapsed: 0.0,
        duration: 1.0,
    }
}

fn fake_dot(source: Entity) -> DamageOverTime {
    DamageOverTime {
        source,
        ty: DamageType::Nature,
        per_tick: 5.0,
        interval: 1.0,
        remaining_ticks: 3,
        since_last: 0.0,
    }
}

#[test]
fn hp_zero_inserts_dead_marker() {
    let mut app = death_app();
    let entity = app
        .world_mut()
        .spawn((EnemyMarker, Health { current: 0.0, max: 100.0 }))
        .id();

    app.update();

    assert!(
        app.world().entity(entity).get::<Dead>().is_some(),
        "Dead must be inserted when HP reaches 0",
    );
}

#[test]
fn positive_hp_does_not_become_dead() {
    let mut app = death_app();
    let entity = app
        .world_mut()
        .spawn((EnemyMarker, Health { current: 1.0, max: 100.0 }))
        .id();

    app.update();

    assert!(app.world().entity(entity).get::<Dead>().is_none());
}

#[test]
fn dead_strips_dots() {
    let mut app = death_app();
    let source = Entity::from_raw_u32(7).unwrap();
    let entity = app
        .world_mut()
        .spawn((
            EnemyMarker,
            Health { current: 0.0, max: 100.0 },
            DamageOverTimes(vec![fake_dot(source)]),
        ))
        .id();

    app.update();

    assert!(
        app.world().entity(entity).get::<DamageOverTimes>().is_none(),
        "DoTs must be stripped on death so corpses don't tick",
    );
}

#[test]
fn dead_clears_mob_threat_list() {
    let mut app = death_app();
    let player = Entity::from_raw_u32(42).unwrap();
    let mut threat = ThreatList::default();
    threat.entries.push(ThreatEntry { player_entity: player, threat: 50.0 });
    threat.dirty = false;
    let entity = app
        .world_mut()
        .spawn((EnemyMarker, Health { current: 0.0, max: 100.0 }, threat))
        .id();

    app.update();

    let tl = app.world().entity(entity).get::<ThreatList>().unwrap();
    assert!(tl.entries.is_empty(), "threat list must be cleared so a corpse exits combat");
    assert!(tl.dirty, "clearing must flip dirty so the next sync replicates an empty list");
}

#[test]
fn dead_zeroes_enemy_velocity() {
    let mut app = death_app();
    let entity = app
        .world_mut()
        .spawn((
            EnemyMarker,
            Health { current: 0.0, max: 100.0 },
            EnemyVelocity { vx: 3.0, vz: -2.0 },
        ))
        .id();

    app.update();

    let v = app.world().entity(entity).get::<EnemyVelocity>().unwrap();
    assert_eq!(v.vx, 0.0);
    assert_eq!(v.vz, 0.0);
}

#[test]
fn dead_zeroes_player_velocity() {
    let mut app = death_app();
    let entity = app
        .world_mut()
        .spawn((
            PlayerId(1),
            Health { current: 0.0, max: 100.0 },
            PlayerVelocity { vx: 1.0, vy: 4.5, vz: -3.0 },
        ))
        .id();

    app.update();

    let v = app.world().entity(entity).get::<PlayerVelocity>().unwrap();
    assert_eq!(v.vx, 0.0);
    assert_eq!(v.vy, 0.0);
    assert_eq!(v.vz, 0.0);
}

#[test]
fn dead_cancels_enemy_cast() {
    let mut app = death_app();
    let entity = app
        .world_mut()
        .spawn((EnemyMarker, Health { current: 0.0, max: 100.0 }, fake_cast(0)))
        .id();

    app.update();

    assert!(
        app.world().entity(entity).get::<EnemyCast>().is_none(),
        "an in-flight cast must be cancelled when the caster dies",
    );
}

#[test]
fn dead_player_drops_active_stance_and_resets_arc() {
    let mut app = death_app();
    let arc = ArcState {
        streak: 7,
        streak_at_last_activation: 3,
        apex_visits_since_commit: 1,
        ..Default::default()
    };
    let entity = app
        .world_mut()
        .spawn((
            PlayerId(1),
            Health { current: 0.0, max: 100.0 },
            CombatState { in_combat: true, active_stance: Some(RoleStance::Dps) },
            arc,
        ))
        .id();

    app.update();

    let combat = app.world().entity(entity).get::<CombatState>().unwrap();
    assert_eq!(
        combat.active_stance, None,
        "active stance must clear on death — corpse must not hold a stance",
    );
    let arc_after = app.world().entity(entity).get::<ArcState>().unwrap();
    assert_eq!(arc_after.streak, 0, "arc streak must reset on stance drop");
    assert_eq!(arc_after.streak_at_last_activation, 0);
    assert_eq!(arc_after.apex_visits_since_commit, 0);
}

#[test]
fn dead_player_with_no_stance_leaves_combat_state_alone() {
    // No stance → no minigame state to clear → reset_on_stance_change is not
    // called. CombatState is observed but not mutated beyond what it already
    // was. This pins the "skip if active_stance is None" gate.
    let mut app = death_app();
    let entity = app
        .world_mut()
        .spawn((
            PlayerId(1),
            Health { current: 0.0, max: 100.0 },
            CombatState { in_combat: false, active_stance: None },
        ))
        .id();

    app.update();

    let combat = app.world().entity(entity).get::<CombatState>().unwrap();
    assert_eq!(combat.active_stance, None);
    assert!(app.world().entity(entity).get::<Dead>().is_some());
}

#[test]
fn already_dead_entity_is_not_reprocessed() {
    // Without<Dead> on the candidate query is the contract; this pins it.
    // Spawn already-Dead with a populated DoT stack and make sure the system
    // does not strip or otherwise mutate it.
    let mut app = death_app();
    let source = Entity::from_raw_u32(9).unwrap();
    let entity = app
        .world_mut()
        .spawn((
            EnemyMarker,
            Dead,
            Health { current: 0.0, max: 100.0 },
            DamageOverTimes(vec![fake_dot(source)]),
        ))
        .id();

    app.update();

    assert!(
        app.world().entity(entity).get::<DamageOverTimes>().is_some(),
        "system must not re-strip components on an already-Dead entity",
    );
}

