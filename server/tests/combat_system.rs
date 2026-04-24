//! System-level tests for combat ECS systems.
//!
//! Coverage here is intentionally narrow: most combat systems
//! (`apply_damage_events`, `tick_dots`, `process_player_inputs`) read or
//! write Lightyear `MessageReader` / `MessageSender` queues, which would
//! require standing up the Lightyear plugin stack to drive end-to-end. The
//! gameplay-relevant math in those systems is covered by pure-function
//! tests inline in `server/src/systems/combat.rs`.
//!
//! `apply_stance_multipliers` is the lone combat system whose query shape is
//! purely Bevy components + `Res<Time>`, so it slots cleanly into the same
//! minimal-App harness used by the arc and cube tests.

mod common;

use std::time::Duration;

use bevy::prelude::*;

use shared::components::combat::CombatState;
use shared::components::player::RoleStance;

use server::systems::combat::{self, ThreatBonus, ThreatModifiers};

#[test]
fn stance_multiplier_tracks_active_stance() {
    let mut app = common::minimal_app();
    app.add_systems(Update, combat::apply_stance_multipliers);

    let cases = [
        (Some(RoleStance::Tank), 2.0),
        (Some(RoleStance::Dps),  1.0),
        (Some(RoleStance::Heal), 0.0),
        (None,                   0.5), // neutral fallback
    ];

    for (stance, expected) in cases {
        let entity = app
            .world_mut()
            .spawn((
                CombatState { in_combat: true, active_stance: stance },
                ThreatModifiers::default(),
            ))
            .id();

        common::advance(&mut app, Duration::from_secs_f64(1.0 / 60.0));

        let mods = app.world().entity(entity).get::<ThreatModifiers>().unwrap();
        assert_eq!(
            mods.role_multiplier, expected,
            "stance {stance:?} should map to role_multiplier {expected}",
        );

        app.world_mut().despawn(entity);
    }
}

#[test]
fn expired_threat_bonuses_are_pruned() {
    let mut app = common::minimal_app();
    app.add_systems(Update, combat::apply_stance_multipliers);

    // Advance the clock past `expires_at = 1.0` before the system runs once.
    // Time::advance_by sets elapsed_secs to the cumulative dt, so two 1s ticks
    // put us at elapsed = 2.0 — the 0.5s-expiring bonus is stale, the 5.0s
    // one isn't.
    let entity = app
        .world_mut()
        .spawn((
            CombatState { in_combat: true, active_stance: None },
            ThreatModifiers {
                role_multiplier: 0.5,
                bonuses: vec![
                    ThreatBonus { multiplier: 0.3, expires_at: 0.5 }, // stale
                    ThreatBonus { multiplier: 0.7, expires_at: 5.0 }, // alive
                ],
            },
        ))
        .id();

    common::advance(&mut app, Duration::from_secs(1));
    common::advance(&mut app, Duration::from_secs(1));

    let mods = app.world().entity(entity).get::<ThreatModifiers>().unwrap();
    assert_eq!(mods.bonuses.len(), 1, "expired bonus should have been pruned");
    assert_eq!(mods.bonuses[0].multiplier, 0.7);
}

#[test]
fn permanent_bonuses_with_zero_expiry_are_kept() {
    // expires_at == 0.0 is the sentinel for "never expires" — the prune
    // predicate explicitly preserves these. Lock that contract.
    let mut app = common::minimal_app();
    app.add_systems(Update, combat::apply_stance_multipliers);

    let entity = app
        .world_mut()
        .spawn((
            CombatState { in_combat: true, active_stance: None },
            ThreatModifiers {
                role_multiplier: 0.5,
                bonuses: vec![ThreatBonus { multiplier: 0.5, expires_at: 0.0 }],
            },
        ))
        .id();

    common::advance(&mut app, Duration::from_secs(60));

    let mods = app.world().entity(entity).get::<ThreatModifiers>().unwrap();
    assert_eq!(mods.bonuses.len(), 1, "permanent bonus must survive ticking");
}
