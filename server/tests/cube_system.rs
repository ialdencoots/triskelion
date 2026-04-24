//! System-level tests for the cube minigame.
//!
//! Drives `tick_cube_states` end-to-end: builds a minimal App, spawns an
//! entity carrying the trio of components the system queries (`CubeState`,
//! `ArcState`, `CombatState`), then advances simulated time and inspects the
//! resulting world state.

mod common;

use std::time::Duration;

use shared::components::combat::CombatState;
use shared::components::minigame::arc::{ArcState, CUBE_CRITICAL_MASS_CAP};
use shared::components::minigame::cube::{CubeState, CUBE_FILL_CYCLE_SECS};
use shared::components::player::RoleStance;

use server::systems::minigame;

#[test]
fn cube_activates_when_streak_hits_critical_mass_in_tank_stance() {
    let mut app = common::minimal_app();
    app.add_systems(bevy::prelude::Update, minigame::tick_cube_states);

    let entity = app
        .world_mut()
        .spawn((
            ArcState {
                streak: CUBE_CRITICAL_MASS_CAP,
                streak_at_last_activation: 0,
                ..Default::default()
            },
            CubeState::default(),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Tank),
            },
        ))
        .id();

    common::advance(&mut app, Duration::from_secs_f64(1.0 / 60.0));

    let cube = app.world().entity(entity).get::<CubeState>().unwrap();
    let arc = app.world().entity(entity).get::<ArcState>().unwrap();
    assert!(cube.active, "cube should activate at critical mass");
    assert_eq!(
        arc.streak_at_last_activation, CUBE_CRITICAL_MASS_CAP,
        "activation must snapshot the streak baseline",
    );
    assert_eq!(arc.streak, CUBE_CRITICAL_MASS_CAP, "streak survives activation");
}

#[test]
fn cube_does_not_activate_outside_tank_or_heal_stance() {
    let mut app = common::minimal_app();
    app.add_systems(bevy::prelude::Update, minigame::tick_cube_states);

    let entity = app
        .world_mut()
        .spawn((
            ArcState {
                streak: CUBE_CRITICAL_MASS_CAP * 5, // well over cap
                ..Default::default()
            },
            CubeState::default(),
            CombatState {
                in_combat: true,
                active_stance: None, // no stance → cube gate is closed
            },
        ))
        .id();

    common::advance(&mut app, Duration::from_secs_f64(1.0 / 60.0));

    let cube = app.world().entity(entity).get::<CubeState>().unwrap();
    assert!(!cube.active, "cube must not activate without Tank/Heal stance");
}

#[test]
fn active_cube_fill_progress_resets_when_window_passes_unclaimed() {
    let mut app = common::minimal_app();
    app.add_systems(bevy::prelude::Update, minigame::tick_cube_states);

    // Pre-activated cube that has nearly swept the full collect window.
    let entity = app
        .world_mut()
        .spawn((
            ArcState::default(),
            CubeState {
                active: true,
                fill_progress: 0.99,
                rotations_remaining: 4,
                ..Default::default()
            },
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Tank),
            },
        ))
        .id();

    // Advance past the reset threshold (window = 0.18 → ~0.27 s of fill cycle).
    let dt = Duration::from_secs_f64((CUBE_FILL_CYCLE_SECS as f64) * 0.3);
    common::advance(&mut app, dt);

    let cube = app.world().entity(entity).get::<CubeState>().unwrap();
    assert!(cube.active, "cube stays active after a missed window");
    assert!(
        cube.fill_progress < 0.5,
        "fill_progress should reset toward 0 after sweeping past the window, was {}",
        cube.fill_progress,
    );
}
