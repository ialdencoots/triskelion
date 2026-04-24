//! System-level test for the arc minigame.
//!
//! Demonstrates the App-based pattern for testing a Bevy system end-to-end:
//! build a minimal App, register the system, spawn entities with the relevant
//! components, advance time deterministically, then assert on world state.
//!
//! Follow-up coverage to add using this same pattern:
//! - `tick_cube_states` — cube FSM transitions across rotation/hold/pop phases.
//! - `apply_damage_events` — full damage formula resolution against `Health`.
//! - `tick_dots` — DoT tick cadence and removal on expiry.

mod common;

use std::time::Duration;

use shared::components::minigame::arc::ArcState;

use server::systems::minigame;

#[test]
fn tick_arc_states_breaks_idle_streak_after_two_apex_visits() {
    let mut app = common::minimal_app();
    app.add_systems(bevy::prelude::Update, minigame::tick_arc_states);

    let entity = app
        .world_mut()
        .spawn(ArcState {
            streak: 5,
            streak_at_last_activation: 5,
            ..Default::default()
        })
        .id();

    // omega = π → period 2 s → two apex crossings per full oscillation.
    // Step at 60 Hz for ~2.1 s of simulated time.
    for _ in 0..130 {
        common::advance(&mut app, Duration::from_secs_f64(1.0 / 60.0));
    }

    let arc = app.world().entity(entity).get::<ArcState>().unwrap();
    assert_eq!(arc.streak, 0, "idle arc should break streak after two apex visits");
    assert_eq!(arc.streak_at_last_activation, 0);
}
