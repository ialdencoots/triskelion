//! System-level tests for `apply_disruption_events`.
//!
//! Drives the full disruption-routing matrix: each (`DisruptionKind`,
//! minigame component) pair maps to a specific field with a specific
//! coefficient. These constants are private to `combat.rs` — the tests
//! pin the *behavior* (multipliers and routing) so a refactor that
//! shuffles the constants will fail loudly here.

mod common;

use bevy::prelude::*;

use shared::components::minigame::arc::ArcState;
use shared::components::minigame::bar_fill::BarFillState;
use shared::components::minigame::heartbeat::HeartbeatState;
use shared::components::combat::{DisruptionKind, DisruptionProfile};
use shared::events::combat::DisruptionEvent;

use server::systems::combat;

#[derive(Resource, Default)]
struct PendingDisruption(Vec<DisruptionEvent>);

fn seed_disruption(
    mut pending: ResMut<PendingDisruption>,
    mut writer: MessageWriter<DisruptionEvent>,
) {
    for ev in pending.0.drain(..) {
        writer.write(ev);
    }
}

fn disruption_app() -> App {
    let mut app = common::minimal_app();
    app.add_message::<DisruptionEvent>();
    app.init_resource::<PendingDisruption>();
    app.add_systems(
        Update,
        (seed_disruption, combat::apply_disruption_events).chain(),
    );
    app
}

fn send(app: &mut App, target: Entity, kind: DisruptionKind, magnitude: f32) {
    app.world_mut()
        .resource_mut::<PendingDisruption>()
        .0
        .push(DisruptionEvent {
            target,
            profile: DisruptionProfile { kind, magnitude },
        });
    app.update();
}

// ── Arc routing ──────────────────────────────────────────────────────────────

#[test]
fn spike_to_arc_adds_spike_arc_coefficient_times_magnitude() {
    let mut app = disruption_app();
    let target = app.world_mut().spawn(ArcState::default()).id();

    send(&mut app, target, DisruptionKind::Spike, 0.5);

    let arc = app.world().entity(target).get::<ArcState>().unwrap();
    // SPIKE_ARC = 3.0 → 3.0 × 0.5 = 1.5
    assert!(
        (arc.disruption_velocity - 1.5).abs() < 1e-5,
        "expected disruption_velocity ≈ 1.5, got {}",
        arc.disruption_velocity,
    );
}

#[test]
fn sustained_to_arc_uses_drift_coefficient() {
    let mut app = disruption_app();
    let target = app.world_mut().spawn(ArcState::default()).id();

    send(&mut app, target, DisruptionKind::Sustained, 0.5);

    let arc = app.world().entity(target).get::<ArcState>().unwrap();
    // DRIFT_ARC = 0.8 → 0.8 × 0.5 = 0.4
    assert!(
        (arc.disruption_velocity - 0.4).abs() < 1e-5,
        "expected disruption_velocity ≈ 0.4, got {}",
        arc.disruption_velocity,
    );
}

#[test]
fn arc_disruption_accumulates_additively() {
    // Counter-directional impulses are additive — a second spike on top of
    // the first stacks rather than replacing.
    let mut app = disruption_app();
    let target = app.world_mut().spawn(ArcState::default()).id();

    send(&mut app, target, DisruptionKind::Spike, 0.4);
    send(&mut app, target, DisruptionKind::Spike, 0.3);

    let arc = app.world().entity(target).get::<ArcState>().unwrap();
    // Note: `tick_arc` would decay the first impulse during the second
    // app.update — but it's not registered in this app, so we observe pure
    // additive accumulation. Expected: 3.0 × 0.4 + 3.0 × 0.3 = 2.1
    assert!(
        (arc.disruption_velocity - 2.1).abs() < 1e-5,
        "expected disruption_velocity ≈ 2.1, got {}",
        arc.disruption_velocity,
    );
}

// ── Heartbeat routing ────────────────────────────────────────────────────────

#[test]
fn spike_to_heartbeat_increments_frequency_spike() {
    let mut app = disruption_app();
    let target = app.world_mut().spawn(HeartbeatState::default()).id();

    send(&mut app, target, DisruptionKind::Spike, 1.0);

    let hb = app.world().entity(target).get::<HeartbeatState>().unwrap();
    // SPIKE_HB = 0.6
    assert!((hb.frequency_spike - 0.6).abs() < 1e-5);
    assert_eq!(hb.envelope_noise, 0.0, "spike must not bleed into noise");
}

#[test]
fn sustained_to_heartbeat_increments_envelope_noise() {
    let mut app = disruption_app();
    let target = app.world_mut().spawn(HeartbeatState::default()).id();

    send(&mut app, target, DisruptionKind::Sustained, 1.0);

    let hb = app.world().entity(target).get::<HeartbeatState>().unwrap();
    // NOISE_HB = 0.25
    assert!((hb.envelope_noise - 0.25).abs() < 1e-5);
    assert_eq!(hb.frequency_spike, 0.0, "noise must not bleed into spike");
}

// ── Bar fill routing ────────────────────────────────────────────────────────

#[test]
fn any_disruption_to_bar_fill_increments_drain_pending() {
    // BarFillState collapses Spike/Sustained into a single drain mechanic —
    // both kinds add to `drain_pending` with the same coefficient.
    let mut app = disruption_app();
    let target = app.world_mut().spawn(BarFillState::default()).id();

    send(&mut app, target, DisruptionKind::Spike, 1.0);
    send(&mut app, target, DisruptionKind::Sustained, 1.0);

    let bf = app.world().entity(target).get::<BarFillState>().unwrap();
    // DRAIN_BF = 0.35 → 2 × 0.35 = 0.7
    assert!(
        (bf.drain_pending - 0.7).abs() < 1e-5,
        "expected drain_pending ≈ 0.7, got {}",
        bf.drain_pending,
    );
}

// ── Targeting ───────────────────────────────────────────────────────────────

#[test]
fn disruption_only_affects_named_target() {
    let mut app = disruption_app();
    let alice = app.world_mut().spawn(ArcState::default()).id();
    let bob = app.world_mut().spawn(ArcState::default()).id();

    send(&mut app, alice, DisruptionKind::Spike, 1.0);

    let alice_arc = app.world().entity(alice).get::<ArcState>().unwrap();
    let bob_arc = app.world().entity(bob).get::<ArcState>().unwrap();
    assert!(alice_arc.disruption_velocity > 0.0);
    assert_eq!(bob_arc.disruption_velocity, 0.0, "Bob must be untouched");
}

#[test]
fn disruption_to_entity_with_multiple_minigames_routes_to_each() {
    // A single entity can simultaneously hold ArcState + HeartbeatState +
    // BarFillState (no class restriction in the resolver). Each branch is
    // independent — one disruption fans out to every minigame component
    // present.
    let mut app = disruption_app();
    let target = app
        .world_mut()
        .spawn((
            ArcState::default(),
            HeartbeatState::default(),
            BarFillState::default(),
        ))
        .id();

    send(&mut app, target, DisruptionKind::Spike, 1.0);

    let entity_ref = app.world().entity(target);
    assert!(entity_ref.get::<ArcState>().unwrap().disruption_velocity > 0.0);
    assert!(entity_ref.get::<HeartbeatState>().unwrap().frequency_spike > 0.0);
    assert!(entity_ref.get::<BarFillState>().unwrap().drain_pending > 0.0);
}
