//! System-level tests for `apply_damage_events`.
//!
//! Drives the full event-routing path end-to-end. Setup:
//! - Register `DamageEvent` as a Bevy message (mirroring `SharedPlugin`).
//! - Use a "pending events" resource + seed system to inject `DamageEvent`s
//!   from outside, since `MessageWriter` is only accessible inside a system.
//! - Leave the Lightyear `MessageSender<DamageNumberMsg>` and
//!   `MessageSender<CombatLogMsg>` queries with zero matching entities — no
//!   `PlayerEntityLink` is spawned, so the outbound-message branches are
//!   no-ops. The damage math, HP mutation, and threat updates all run
//!   normally; only the client-bound notifications are skipped.

mod common;

use bevy::prelude::*;

use shared::components::combat::{CombatState, DamageType, Dead, Health, Resistances};
use shared::components::enemy::EnemyMarker;
use shared::components::player::PlayerId;
use shared::events::combat::{DamageEvent, DisruptionEvent};

use server::systems::combat::{self, ThreatList, ThreatModifiers};

#[derive(Resource, Default)]
struct PendingDamage(Vec<DamageEvent>);

fn seed_damage(
    mut pending: ResMut<PendingDamage>,
    mut writer: MessageWriter<DamageEvent>,
) {
    for ev in pending.0.drain(..) {
        writer.write(ev);
    }
}

fn damage_app() -> App {
    let mut app = common::minimal_app();
    app.add_message::<DamageEvent>();
    // apply_damage_events emits scaled DisruptionEvents post-mitigation, so
    // the message type has to be registered even if no test consumes it.
    app.add_message::<DisruptionEvent>();
    app.init_resource::<PendingDamage>();
    // Seed must run before the resolver so events are visible same-tick.
    app.add_systems(Update, (seed_damage, combat::apply_damage_events).chain());
    app
}

fn ev(attacker: Entity, target: Entity, base: f32, ty: DamageType) -> DamageEvent {
    DamageEvent {
        attacker,
        target,
        base,
        ty,
        additive: 0.0,
        multipliers: 1.0,
        quality: 1.0,
        is_crit: false,
        disruption: None,
    }
}

fn send(app: &mut App, event: DamageEvent) {
    app.world_mut().resource_mut::<PendingDamage>().0.push(event);
    app.update();
}

// ── Player → Enemy ──────────────────────────────────────────────────────────

#[test]
fn player_to_enemy_applies_damage_and_credits_threat() {
    let mut app = damage_app();
    let attacker = app
        .world_mut()
        .spawn((PlayerId(1), ThreatModifiers::default())) // role_multiplier = 0.5
        .id();
    let target = app
        .world_mut()
        .spawn((
            EnemyMarker,
            Health::new(100.0),
            Resistances::default(),
            ThreatList::default(),
        ))
        .id();

    send(&mut app, ev(attacker, target, 20.0, DamageType::Physical));

    let health = app.world().entity(target).get::<Health>().unwrap();
    assert_eq!(health.current, 80.0);

    let threat = app.world().entity(target).get::<ThreatList>().unwrap();
    assert_eq!(threat.entries.len(), 1);
    assert_eq!(threat.entries[0].player_entity, attacker);
    assert_eq!(threat.entries[0].threat, 10.0); // 20 dmg × 0.5 default role
    assert!(threat.dirty);
}

#[test]
fn player_to_enemy_resist_is_routed_by_damage_type() {
    let mut app = damage_app();
    let attacker = app
        .world_mut()
        .spawn((PlayerId(1), ThreatModifiers::default()))
        .id();
    let target = app
        .world_mut()
        .spawn((
            EnemyMarker,
            Health::new(200.0),
            Resistances::new(0.5, 0.0, 0.0), // 50% physical only
            ThreatList::default(),
        ))
        .id();

    // Arcane damage bypasses the physical resist.
    send(&mut app, ev(attacker, target, 100.0, DamageType::Arcane));
    assert_eq!(app.world().entity(target).get::<Health>().unwrap().current, 100.0);

    // Physical damage is halved.
    send(&mut app, ev(attacker, target, 100.0, DamageType::Physical));
    assert_eq!(app.world().entity(target).get::<Health>().unwrap().current, 50.0);
}

#[test]
fn enemy_hp_clamps_to_zero_on_overkill() {
    let mut app = damage_app();
    let attacker = app
        .world_mut()
        .spawn((PlayerId(1), ThreatModifiers::default()))
        .id();
    let target = app
        .world_mut()
        .spawn((
            EnemyMarker,
            Health::new(50.0),
            Resistances::default(),
            ThreatList::default(),
        ))
        .id();

    send(&mut app, ev(attacker, target, 500.0, DamageType::Physical));

    let health = app.world().entity(target).get::<Health>().unwrap();
    assert_eq!(health.current, 0.0, "HP must clamp to 0 — never go negative");
}

#[test]
fn damage_to_dead_enemy_is_ignored() {
    let mut app = damage_app();
    let attacker = app
        .world_mut()
        .spawn((PlayerId(1), ThreatModifiers::default()))
        .id();
    let target = app
        .world_mut()
        .spawn((
            EnemyMarker,
            Health { current: 0.0, max: 100.0 }, // already dead
            Resistances::default(),
            ThreatList::default(),
        ))
        .id();

    send(&mut app, ev(attacker, target, 20.0, DamageType::Physical));

    let threat = app.world().entity(target).get::<ThreatList>().unwrap();
    assert!(
        threat.entries.is_empty(),
        "no threat should be credited for damage on a corpse",
    );
}

// ── Enemy → Player ──────────────────────────────────────────────────────────

#[test]
fn enemy_to_player_applies_damage_without_generating_threat() {
    let mut app = damage_app();
    let attacker = app.world_mut().spawn(EnemyMarker).id();
    let target = app
        .world_mut()
        .spawn((
            PlayerId(1),
            Health::new(100.0),
            CombatState::default(),
        ))
        .id();

    send(&mut app, ev(attacker, target, 30.0, DamageType::Physical));

    let health = app.world().entity(target).get::<Health>().unwrap();
    assert_eq!(health.current, 70.0);
}

#[test]
fn damage_to_dead_player_is_ignored() {
    // The is_alive() check on the player branch of apply_damage_events keeps
    // posthumous DoT ticks (or any in-flight enemy hit) from cratering an HP
    // bar that's already at 0. Mirrors `damage_to_dead_enemy_is_ignored`.
    let mut app = damage_app();
    let attacker = app.world_mut().spawn(EnemyMarker).id();
    let target = app
        .world_mut()
        .spawn((
            PlayerId(1),
            Health { current: 0.0, max: 100.0 }, // already dead
            Dead,
            CombatState::default(),
        ))
        .id();

    send(&mut app, ev(attacker, target, 30.0, DamageType::Physical));

    let health = app.world().entity(target).get::<Health>().unwrap();
    assert_eq!(health.current, 0.0, "HP must stay clamped at 0 — no posthumous damage");
}

#[test]
fn enemy_to_player_uses_player_resistances_when_present() {
    let mut app = damage_app();
    let attacker = app.world_mut().spawn(EnemyMarker).id();
    let target = app
        .world_mut()
        .spawn((
            PlayerId(1),
            Health::new(100.0),
            Resistances::new(0.0, 0.0, 0.4), // 40% nature
            CombatState::default(),
        ))
        .id();

    send(&mut app, ev(attacker, target, 50.0, DamageType::Nature));

    let health = app.world().entity(target).get::<Health>().unwrap();
    // 50 × (1 − 0.4) = 30
    assert!(
        (health.current - 70.0).abs() < 1e-4,
        "expected 70 HP after 30 nature damage, got {}",
        health.current,
    );
}

// ── Friendly fire ──────────────────────────────────────────────────────────

#[test]
fn player_to_player_is_silently_dropped() {
    let mut app = damage_app();
    let attacker = app
        .world_mut()
        .spawn((PlayerId(1), ThreatModifiers::default()))
        .id();
    let target = app
        .world_mut()
        .spawn((
            PlayerId(2),
            Health::new(100.0),
            CombatState::default(),
        ))
        .id();

    send(&mut app, ev(attacker, target, 50.0, DamageType::Physical));

    let health = app.world().entity(target).get::<Health>().unwrap();
    assert_eq!(health.current, 100.0, "friendly fire must not damage allies");
}

#[test]
fn enemy_to_enemy_is_silently_dropped() {
    let mut app = damage_app();
    let attacker = app.world_mut().spawn(EnemyMarker).id();
    let target = app
        .world_mut()
        .spawn((
            EnemyMarker,
            Health::new(100.0),
            Resistances::default(),
            ThreatList::default(),
        ))
        .id();

    send(&mut app, ev(attacker, target, 50.0, DamageType::Physical));

    let health = app.world().entity(target).get::<Health>().unwrap();
    assert_eq!(health.current, 100.0, "enemy-on-enemy is not a feature");

    let threat = app.world().entity(target).get::<ThreatList>().unwrap();
    assert!(threat.entries.is_empty());
}

// ── Threat scaling ──────────────────────────────────────────────────────────

#[test]
fn threat_credit_scales_with_attacker_threat_modifiers() {
    let mut app = damage_app();
    let attacker = app
        .world_mut()
        .spawn((
            PlayerId(1),
            ThreatModifiers {
                role_multiplier: 2.0, // Tank
                bonuses: vec![],
            },
        ))
        .id();
    let target = app
        .world_mut()
        .spawn((
            EnemyMarker,
            Health::new(200.0),
            Resistances::default(),
            ThreatList::default(),
        ))
        .id();

    send(&mut app, ev(attacker, target, 40.0, DamageType::Physical));

    let threat = app.world().entity(target).get::<ThreatList>().unwrap();
    assert_eq!(threat.entries[0].threat, 80.0); // 40 × 2.0 Tank role
}
