//! System-level tests for `tick_dots`.
//!
//! Drives the DoT scheduler end-to-end: registers `DamageEvent` as a Bevy
//! message (the "local, non-networked" registration mirrored from
//! `SharedPlugin`), spawns targets with `DamageOverTimes`, advances time,
//! and inspects the captured `DamageEvent` stream via a sink system.
//!
//! This pattern is reusable for any other system that emits `DamageEvent`
//! (e.g. `enemy::tick_enemy_abilities`) once the harness handles Lightyear's
//! networking dependencies — for now, scope is limited to `tick_dots`, which
//! takes only `Time`, a query, and the message writer.

mod common;

use std::time::Duration;

use bevy::prelude::*;

use shared::components::combat::DamageType;
use shared::events::combat::DamageEvent;

use server::systems::combat::{self, DamageOverTime, DamageOverTimes};

#[derive(Resource, Default)]
struct CapturedDamage(Vec<DamageEvent>);

fn capture_damage(mut reader: MessageReader<DamageEvent>, mut sink: ResMut<CapturedDamage>) {
    for ev in reader.read() {
        sink.0.push(ev.clone());
    }
}

fn dots_app() -> App {
    let mut app = common::minimal_app();
    app.add_message::<DamageEvent>();
    app.init_resource::<CapturedDamage>();
    // Capture must run after the producer so the same-tick events are visible.
    app.add_systems(Update, (combat::tick_dots, capture_damage).chain());
    app
}

fn one_shot_dot(source: Entity, per_tick: f32, interval: f32, ticks: u32) -> DamageOverTime {
    DamageOverTime {
        source,
        ty: DamageType::Nature,
        per_tick,
        interval,
        remaining_ticks: ticks,
        since_last: 0.0,
    }
}

#[test]
fn dot_does_not_fire_before_interval_elapses() {
    let mut app = dots_app();
    let source = Entity::from_raw_u32(1).unwrap();
    app.world_mut().spawn(DamageOverTimes(vec![one_shot_dot(source, 10.0, 1.0, 3)]));

    // Advance halfway through the first interval.
    common::advance(&mut app, Duration::from_millis(500));

    let captured = app.world().resource::<CapturedDamage>();
    assert!(captured.0.is_empty(), "DoT must not fire before interval elapses");
}

#[test]
fn dot_fires_after_interval_with_correct_payload() {
    let mut app = dots_app();
    let source = Entity::from_raw_u32(1).unwrap();
    let target = app
        .world_mut()
        .spawn(DamageOverTimes(vec![one_shot_dot(source, 10.0, 1.0, 3)]))
        .id();

    // One tick that crosses the interval boundary.
    common::advance(&mut app, Duration::from_millis(1100));

    let captured = app.world().resource::<CapturedDamage>();
    assert_eq!(captured.0.len(), 1, "exactly one fire after one interval");
    let ev = &captured.0[0];
    assert_eq!(ev.attacker, source);
    assert_eq!(ev.target, target);
    assert_eq!(ev.base, 10.0);
    assert_eq!(ev.ty, DamageType::Nature);
    assert_eq!(ev.quality, 1.0, "DoT ticks always carry quality 1.0");

    let dots = app
        .world()
        .entity(target)
        .get::<DamageOverTimes>()
        .unwrap();
    assert_eq!(dots.0.len(), 1);
    assert_eq!(dots.0[0].remaining_ticks, 2);
}

#[test]
fn dot_is_removed_after_last_tick() {
    let mut app = dots_app();
    let source = Entity::from_raw_u32(1).unwrap();
    let target = app
        .world_mut()
        .spawn(DamageOverTimes(vec![one_shot_dot(source, 5.0, 0.5, 2)]))
        .id();

    // Two consecutive intervals → DoT exhausts both ticks and is removed.
    common::advance(&mut app, Duration::from_millis(600));
    common::advance(&mut app, Duration::from_millis(600));

    let captured = app.world().resource::<CapturedDamage>();
    assert_eq!(captured.0.len(), 2, "two ticks should produce two events");

    let dots = app.world().entity(target).get::<DamageOverTimes>().unwrap();
    assert!(dots.0.is_empty(), "exhausted DoT should be removed from the stack");
}

#[test]
fn multiple_dots_on_same_target_tick_independently() {
    let mut app = dots_app();
    let src_a = Entity::from_raw_u32(1).unwrap();
    let src_b = Entity::from_raw_u32(2).unwrap();
    let target = app
        .world_mut()
        .spawn(DamageOverTimes(vec![
            one_shot_dot(src_a, 7.0, 0.4, 5),  // fast
            one_shot_dot(src_b, 20.0, 1.0, 5), // slow
        ]))
        .id();

    // 0.5s — only the fast DoT (interval 0.4) crosses; slow stays under.
    common::advance(&mut app, Duration::from_millis(500));

    let captured = app.world().resource::<CapturedDamage>();
    assert_eq!(captured.0.len(), 1);
    assert_eq!(captured.0[0].attacker, src_a);
    assert_eq!(captured.0[0].base, 7.0);

    let dots = app.world().entity(target).get::<DamageOverTimes>().unwrap();
    let by_source = |s: Entity| dots.0.iter().find(|d| d.source == s).unwrap();
    assert_eq!(by_source(src_a).remaining_ticks, 4, "fast DoT consumed one tick");
    assert_eq!(by_source(src_b).remaining_ticks, 5, "slow DoT untouched");
}

#[test]
fn one_tick_fires_at_most_once_even_when_dt_exceeds_multiple_intervals() {
    // The DoT scheduler only fires once per system tick — if `dt` is large
    // enough to span several intervals, the surplus stays in `since_last`
    // and unwinds across subsequent ticks. This keeps a single laggy frame
    // from delivering a burst of damage.
    let mut app = dots_app();
    let source = Entity::from_raw_u32(1).unwrap();
    let target = app
        .world_mut()
        .spawn(DamageOverTimes(vec![one_shot_dot(source, 1.0, 0.2, 10)]))
        .id();

    common::advance(&mut app, Duration::from_millis(1000)); // 5× interval

    let captured = app.world().resource::<CapturedDamage>();
    assert_eq!(captured.0.len(), 1, "burst-fire prevention: one fire per tick");

    let dots = app.world().entity(target).get::<DamageOverTimes>().unwrap();
    assert_eq!(dots.0[0].remaining_ticks, 9);
    assert!(
        dots.0[0].since_last >= 0.2,
        "carry-over should leave since_last ≥ interval so the next tick fires immediately, was {}",
        dots.0[0].since_last,
    );
}
