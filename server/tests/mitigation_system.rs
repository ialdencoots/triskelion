//! System-level tests for the Intercessor mitigation pipeline.
//!
//! Drives `apply_damage_events` end-to-end with one or more Intercessor
//! healers warding a player target, verifying that:
//!   - the saturating absorption curve drains pools correctly
//!   - sequential best-after stacking preserves additivity without a global cap
//!   - the invuln window short-circuits any incoming hit
//!   - prevention threat lands only on the specific attacker
//!   - non-Heal stances and non-Intercessors don't engage the mitigation path
//!   - the ward Health absorbs leftover damage past the pool

mod common;

use bevy::prelude::*;

use shared::components::combat::{CombatState, DamageType, Health, Resistances};
use shared::components::enemy::EnemyMarker;
use shared::components::instance::InstanceId;
use shared::components::minigame::arc::ArcState;
use shared::components::player::{
    PlayerId, PlayerPosition, PlayerSelectedTarget, RoleStance, SelectedMobOrPlayer,
};
use shared::components::combat::{DisruptionKind, DisruptionProfile};
use shared::events::combat::{DamageEvent, DisruptionEvent};

use server::systems::combat::{
    self, CritStreak, MitigationCommitEvent, MitigationPool, ThreatList,
};

#[derive(Resource, Default)]
struct PendingDamage(Vec<DamageEvent>);

fn seed_damage(mut pending: ResMut<PendingDamage>, mut writer: MessageWriter<DamageEvent>) {
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
    app.add_systems(Update, (seed_damage, combat::apply_damage_events).chain());
    app
}

fn ev(attacker: Entity, target: Entity, base: f32) -> DamageEvent {
    DamageEvent {
        attacker,
        target,
        base,
        ty: DamageType::Physical,
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

/// Build an `ArcState` whose `commit.mean()` returns `mean_q` (one history entry).
fn arc_with_mean_q(mean_q: f32) -> ArcState {
    let mut arc = ArcState::default();
    arc.commit.push(mean_q, 4);
    arc
}

/// Spawn an Intercessor healer warding `ward_id`. Returns the entity.
fn spawn_intercessor(
    app: &mut App,
    healer_id: u64,
    ward_id: u64,
    pool: f32,
    invuln_until: f32,
    mean_q: f32,
    instance_id: u32,
) -> Entity {
    app.world_mut()
        .spawn((
            PlayerId(healer_id),
            Health::default(),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            PlayerSelectedTarget(Some(SelectedMobOrPlayer::Player(ward_id))),
            MitigationPool { amount: pool, invuln_until },
            CritStreak::default(),
            arc_with_mean_q(mean_q),
            InstanceId(instance_id),
        ))
        .id()
}

fn spawn_ward(app: &mut App, pid: u64, hp: f32, instance_id: u32) -> Entity {
    app.world_mut()
        .spawn((
            PlayerId(pid),
            Health::new(hp),
            CombatState::default(),
            PlayerPosition::new(0.0, 0.0, 0.0),
            InstanceId(instance_id),
        ))
        .id()
}

fn spawn_mob(app: &mut App, hp: f32, instance_id: u32) -> Entity {
    app.world_mut()
        .spawn((
            EnemyMarker,
            Health::new(hp),
            Resistances::default(),
            ThreatList::default(),
            InstanceId(instance_id),
        ))
        .id()
}

// ── Single-Intercessor mitigation ───────────────────────────────────────────

#[test]
fn small_hit_absorbed_near_max_fraction() {
    // q=1.0, incoming=10, pool=1000:
    // absorbed = 1.0 * 25.5 * 10 / 40 = 6.375.
    // ward HP -= 10 - 6.375 = 3.625.
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 100.0, 1);
    let healer = spawn_intercessor(&mut app, 2, 1, 1000.0, 0.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward, 10.0));

    let hp = app.world().entity(ward).get::<Health>().unwrap().current;
    assert!((hp - (100.0 - 3.625)).abs() < 0.01, "ward hp {hp}");
    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert!((pool - (1000.0 - 6.375)).abs() < 0.01, "pool {pool}");
}

#[test]
fn large_hit_saturates_absorption() {
    // q=1.0, incoming=1000:
    // absorbed = 25.5 * 1000 / 1030 ≈ 24.7573.
    // ward HP -= 1000 - 24.7573 ≈ 975.24.  Ward starts at 2000 to survive.
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 2000.0, 1);
    let healer = spawn_intercessor(&mut app, 2, 1, 1000.0, 0.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward, 1000.0));

    let absorbed = 25.5 * 1000.0 / 1030.0;
    let hp = app.world().entity(ward).get::<Health>().unwrap().current;
    assert!((hp - (2000.0 - (1000.0 - absorbed))).abs() < 0.01, "ward hp {hp}");
    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert!((pool - (1000.0 - absorbed)).abs() < 0.01, "pool {pool}");
}

#[test]
fn pool_clamps_absorption_when_low() {
    // pool=5, mean_q=1.0, incoming=100. desired = 25.5 * 100/130 = 19.6, but
    // pool only has 5 → absorbed = 5. ward HP -= 95, pool = 0.
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 200.0, 1);
    let healer = spawn_intercessor(&mut app, 2, 1, 5.0, 0.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward, 100.0));

    let hp = app.world().entity(ward).get::<Health>().unwrap().current;
    assert!((hp - (200.0 - 95.0)).abs() < 0.01, "ward hp {hp}");
    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert!(pool.abs() < 0.01, "pool drained, got {pool}");
}

#[test]
fn invuln_window_absorbs_entire_hit_and_leaves_pool_alone() {
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 100.0, 1);
    let healer = spawn_intercessor(&mut app, 2, 1, 50.0, 999.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward, 80.0));

    let hp = app.world().entity(ward).get::<Health>().unwrap().current;
    assert_eq!(hp, 100.0, "ward took zero damage during invuln");
    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert_eq!(pool, 50.0, "invuln does not drain the pool");
}

// ── Stacking ─────────────────────────────────────────────────────────────────

#[test]
fn two_intercessors_stack_via_best_after_sequential_resolution() {
    // Both healers q=1.0. Pool A=100, pool B=60. incoming=100.
    //   A's would-absorb = min(25.5*100/130, 100) = 19.615.
    //   B's would-absorb = min(25.5*100/130, 60) = 19.615.
    //   Post-pool: A 80.385, B 40.385 → A wins, absorbs first.
    //   Remaining 80.385.
    //   B against 80.385: 25.5 * 80.385/110.385 = 18.572.
    //   Remaining 80.385 - 18.572 = 61.813.
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 200.0, 1);
    let a = spawn_intercessor(&mut app, 2, 1, 100.0, 0.0, 1.0, 1);
    let b = spawn_intercessor(&mut app, 3, 1, 60.0, 0.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward, 100.0));

    let abs_a = 25.5 * 100.0 / 130.0;
    let abs_b = 25.5 * (100.0 - abs_a) / (100.0 - abs_a + 30.0);
    let through = 100.0 - abs_a - abs_b;

    let hp = app.world().entity(ward).get::<Health>().unwrap().current;
    assert!(
        (hp - (200.0 - through)).abs() < 0.01,
        "ward hp {hp} expected {} through={through}",
        200.0 - through,
    );
    let pa = app.world().entity(a).get::<MitigationPool>().unwrap().amount;
    let pb = app.world().entity(b).get::<MitigationPool>().unwrap().amount;
    assert!((pa - (100.0 - abs_a)).abs() < 0.01, "pool A {pa}");
    assert!((pb - (60.0 - abs_b)).abs() < 0.01, "pool B {pb}");
}

#[test]
fn two_intercessors_total_absorbed_below_incoming_for_tiny_hits() {
    // The diminishing-returns curve naturally bounds total absorption: even
    // two perfect-quality healers cannot fully eat a 1-damage hit.
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 100.0, 1);
    let _a = spawn_intercessor(&mut app, 2, 1, 100.0, 0.0, 1.0, 1);
    let _b = spawn_intercessor(&mut app, 3, 1, 100.0, 0.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward, 1.0));

    let hp = app.world().entity(ward).get::<Health>().unwrap().current;
    // Ward should take at least *some* damage — the two-healer cap from the
    // saturating curve is well below 100%.
    assert!(hp < 100.0, "ward hp {hp} should be < 100 (some damage gets through)");
    assert!(hp > 99.0, "ward hp {hp} should still be near full (most absorbed)");
}

// ── Stance / subclass gating ────────────────────────────────────────────────

#[test]
fn no_mitigation_when_healer_not_in_heal_stance() {
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 100.0, 1);
    // Healer has the components but is in a non-Heal stance — should not engage.
    let healer = spawn_intercessor(&mut app, 2, 1, 1000.0, 0.0, 1.0, 1);
    app.world_mut()
        .entity_mut(healer)
        .insert(CombatState {
            in_combat: true,
            active_stance: Some(RoleStance::Tank),
        });
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward, 50.0));

    let hp = app.world().entity(ward).get::<Health>().unwrap().current;
    assert_eq!(hp, 50.0, "no mitigation in Tank stance — full hit lands");
    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert_eq!(pool, 1000.0, "pool untouched outside Heal stance");
}

#[test]
fn no_mitigation_when_intercessor_targets_different_player() {
    let mut app = damage_app();
    let ward_a = spawn_ward(&mut app, 1, 100.0, 1);
    let _ward_b = spawn_ward(&mut app, 2, 100.0, 1);
    // Intercessor wards player 2, but damage lands on player 1.
    let _healer = spawn_intercessor(&mut app, 3, 2, 1000.0, 0.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward_a, 50.0));

    let hp = app.world().entity(ward_a).get::<Health>().unwrap().current;
    assert_eq!(hp, 50.0, "no mitigation for non-warded player");
}

#[test]
fn self_ward_with_mob_target_drains_pool() {
    // Healer targets a Mob (effective ward = self via the dispatch helper).
    // Damage on the healer should still be absorbed by their own pool.
    let mut app = damage_app();
    let mob = spawn_mob(&mut app, 100.0, 1);
    // Spawn the "healer" as both healer and damage target (self-ward).
    let healer = app.world_mut()
        .spawn((
            PlayerId(1),
            Health::new(200.0),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            // Targets the mob, not a player — effective ward resolves to self.
            PlayerSelectedTarget(Some(SelectedMobOrPlayer::Mob(mob))),
            MitigationPool { amount: 1000.0, invuln_until: 0.0 },
            CritStreak::default(),
            arc_with_mean_q(1.0),
            InstanceId(1),
        ))
        .id();

    send(&mut app, ev(mob, healer, 100.0));

    let absorbed = 25.5 * 100.0 / 130.0;
    let hp = app.world().entity(healer).get::<Health>().unwrap().current;
    assert!(
        (hp - (200.0 - (100.0 - absorbed))).abs() < 0.01,
        "self-ward should absorb part of the hit; got hp {hp}",
    );
    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert!(
        (pool - (1000.0 - absorbed)).abs() < 0.01,
        "pool should drain by absorbed amount; got {pool}",
    );
}

#[test]
fn no_mitigation_across_instances() {
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 100.0, 1);
    // Healer is in instance 2, ward is in instance 1 — should not bridge.
    let healer = spawn_intercessor(&mut app, 2, 1, 1000.0, 0.0, 1.0, 2);
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward, 50.0));

    let hp = app.world().entity(ward).get::<Health>().unwrap().current;
    assert_eq!(hp, 50.0, "no cross-instance mitigation");
    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert_eq!(pool, 1000.0);
}

// ── Threat ──────────────────────────────────────────────────────────────────

#[test]
fn prevention_threat_lands_only_on_the_specific_attacker() {
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 200.0, 1);
    let healer = spawn_intercessor(&mut app, 2, 1, 1000.0, 0.0, 1.0, 1);
    let mob_a = spawn_mob(&mut app, 100.0, 1);
    let mob_b = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob_a, ward, 100.0));

    let absorbed = 25.5 * 100.0 / 130.0; // ≈ 19.615
    let threat_a = app.world().entity(mob_a).get::<ThreatList>().unwrap();
    let entry = threat_a.entries.iter().find(|e| e.player_entity == healer);
    assert!(
        entry.is_some(),
        "attacker mob_a must have healer credited",
    );
    let credit = entry.unwrap().threat;
    assert!(
        (credit - 0.4 * absorbed).abs() < 0.01,
        "threat credit {credit} expected {} (0.4 × absorbed)",
        0.4 * absorbed,
    );

    let threat_b = app.world().entity(mob_b).get::<ThreatList>().unwrap();
    assert!(
        threat_b.entries.iter().all(|e| e.player_entity != healer),
        "non-attacking mob_b must NOT have healer credited (no fan-out)",
    );
}

// ── Disruption scaling ──────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct CapturedDisruption(Vec<DisruptionEvent>);

fn capture_disruption(
    mut reader: MessageReader<DisruptionEvent>,
    mut sink: ResMut<CapturedDisruption>,
) {
    for ev in reader.read() {
        sink.0.push(ev.clone());
    }
}

fn damage_app_with_disruption_capture() -> App {
    let mut app = damage_app();
    app.init_resource::<CapturedDisruption>();
    app.add_systems(Update, capture_disruption.after(combat::apply_damage_events));
    app
}

fn ev_with_disruption(
    attacker: Entity,
    target: Entity,
    base: f32,
    profile: DisruptionProfile,
) -> DamageEvent {
    let mut e = ev(attacker, target, base);
    e.disruption = Some(profile);
    e
}

#[test]
fn disruption_full_when_no_mitigation() {
    let mut app = damage_app_with_disruption_capture();
    let ward = spawn_ward(&mut app, 1, 200.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    let profile = DisruptionProfile { kind: DisruptionKind::Spike, magnitude: 0.5 };
    send(&mut app, ev_with_disruption(mob, ward, 50.0, profile));

    let captured = app.world().resource::<CapturedDisruption>();
    assert_eq!(captured.0.len(), 1);
    let got = captured.0[0].profile.magnitude;
    assert!((got - 0.5).abs() < 1e-4, "expected full magnitude 0.5, got {got}");
}

#[test]
fn disruption_zero_when_fully_mitigated_by_invuln() {
    let mut app = damage_app_with_disruption_capture();
    let ward = spawn_ward(&mut app, 1, 200.0, 1);
    let _healer = spawn_intercessor(&mut app, 2, 1, 50.0, 999.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    let profile = DisruptionProfile { kind: DisruptionKind::Spike, magnitude: 0.5 };
    send(&mut app, ev_with_disruption(mob, ward, 50.0, profile));

    let captured = app.world().resource::<CapturedDisruption>();
    assert!(
        captured.0.is_empty(),
        "no disruption should fire when invuln absorbs the entire hit",
    );
}

#[test]
fn disruption_scales_with_mitigation_ratio() {
    // Pool=1000, mean_q=1.0, incoming=100. Absorbed = 25.5 * 100/130 ≈ 19.615.
    // Through = 100 - 19.615 ≈ 80.385. Ratio ≈ 0.804. Magnitude scaled to ≈ 0.402.
    let mut app = damage_app_with_disruption_capture();
    let ward = spawn_ward(&mut app, 1, 500.0, 1);
    let _healer = spawn_intercessor(&mut app, 2, 1, 1000.0, 0.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    let profile = DisruptionProfile { kind: DisruptionKind::Spike, magnitude: 0.5 };
    send(&mut app, ev_with_disruption(mob, ward, 100.0, profile));

    let captured = app.world().resource::<CapturedDisruption>();
    assert_eq!(captured.0.len(), 1);
    let absorbed = 25.5 * 100.0 / 130.0;
    let through = 100.0 - absorbed;
    let expected = 0.5 * (through / 100.0);
    let got = captured.0[0].profile.magnitude;
    assert!(
        (got - expected).abs() < 1e-3,
        "expected scaled magnitude {expected:.4}, got {got:.4}",
    );
}

#[test]
fn disruption_omitted_when_event_has_none() {
    let mut app = damage_app_with_disruption_capture();
    let ward = spawn_ward(&mut app, 1, 200.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    // ev() builds with disruption: None.
    send(&mut app, ev(mob, ward, 50.0));

    let captured = app.world().resource::<CapturedDisruption>();
    assert!(
        captured.0.is_empty(),
        "no disruption fires when DamageEvent.disruption is None",
    );
}

// ── Commit pipeline (apply_mitigation_commits) ──────────────────────────────

#[derive(Resource, Default)]
struct PendingMitCommits(Vec<MitigationCommitEvent>);

fn seed_mit_commits(
    mut pending: ResMut<PendingMitCommits>,
    mut writer: MessageWriter<MitigationCommitEvent>,
) {
    for ev in pending.0.drain(..) {
        writer.write(ev);
    }
}

fn commits_app() -> App {
    let mut app = common::minimal_app();
    app.add_message::<MitigationCommitEvent>();
    app.init_resource::<PendingMitCommits>();
    app.add_systems(
        Update,
        (seed_mit_commits, combat::apply_mitigation_commits).chain(),
    );
    app
}

fn send_commit(app: &mut App, event: MitigationCommitEvent) {
    app.world_mut().resource_mut::<PendingMitCommits>().0.push(event);
    app.update();
}

/// Adds `player` to a freshly-spawned mob's threat list, so the player counts
/// as "in combat" for both the commit gate (apply_mitigation_commits checks
/// the ward's threat presence) and the decay gate (tick_mitigation_decay
/// checks the healer's threat presence). Returns the mob.
fn put_player_in_combat(app: &mut App, player: Entity, instance_id: u32) -> Entity {
    let mob = spawn_mob(app, 100.0, instance_id);
    let mut mob_ent = app.world_mut().entity_mut(mob);
    let mut tl = mob_ent.get_mut::<ThreatList>().unwrap();
    tl.entries.push(server::systems::combat::ThreatEntry {
        player_entity: player,
        threat: 1.0,
    });
    tl.dirty = true;
    mob
}

#[test]
fn commit_fills_pool_when_ward_is_in_combat() {
    let mut app = commits_app();
    let ward = spawn_ward(&mut app, 1, 100.0, 1);
    // Healer needs minimum components for apply_mitigation_commits queries:
    // PlayerId + InstanceId + Health (friendly_query) + MitigationPool + CritStreak.
    let healer = app.world_mut()
        .spawn((
            PlayerId(2),
            Health::default(),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            MitigationPool::default(),
            CritStreak::default(),
            InstanceId(1),
        ))
        .id();
    let _mob = put_player_in_combat(&mut app, ward, 1);

    send_commit(&mut app, MitigationCommitEvent {
        healer,
        ward_player_id: 1,
        quality: 0.5,
        healer_pos: Vec3::new(0.0, 0.0, 0.0),
        healer_facing_yaw: 0.0,
    });

    // pool += BASE_MITIGATION (15.0) * quality (0.5) = 7.5.
    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert!((pool - 7.5).abs() < 1e-4, "pool {pool}");
}

#[test]
fn commit_does_not_fill_pool_when_ward_not_in_combat() {
    let mut app = commits_app();
    let _ward = spawn_ward(&mut app, 1, 100.0, 1);
    let healer = app.world_mut()
        .spawn((
            PlayerId(2),
            Health::default(),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            MitigationPool::default(),
            CritStreak::default(),
            InstanceId(1),
        ))
        .id();
    // No mob, so ward is not in any threat list — out of combat.

    send_commit(&mut app, MitigationCommitEvent {
        healer,
        ward_player_id: 1,
        quality: 1.0,
        healer_pos: Vec3::new(0.0, 0.0, 0.0),
        healer_facing_yaw: 0.0,
    });

    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert_eq!(pool, 0.0, "out-of-combat commits do not fill the pool");
}

#[test]
fn commit_skipped_when_ward_out_of_melee_range() {
    let mut app = commits_app();
    let ward = spawn_ward(&mut app, 1, 100.0, 1);
    let healer = app.world_mut()
        .spawn((
            PlayerId(2),
            Health::default(),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            MitigationPool::default(),
            CritStreak::default(),
            InstanceId(1),
        ))
        .id();
    // Place ward at origin; healer reports its own position 100 units away.
    let _mob = put_player_in_combat(&mut app, ward, 1);

    send_commit(&mut app, MitigationCommitEvent {
        healer,
        ward_player_id: 1,
        quality: 1.0,
        healer_pos: Vec3::new(100.0, 0.0, 0.0),  // way beyond MELEE_RANGE
        healer_facing_yaw: 0.0,
    });

    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert_eq!(pool, 0.0, "out-of-range commit does not fill pool");
}

#[test]
fn three_consecutive_crit_quality_commits_grant_invuln() {
    // crit_chance(1.0) == 1.0 by construction, so quality=1.0 commits always crit.
    let mut app = commits_app();
    let ward = spawn_ward(&mut app, 1, 200.0, 1);
    let healer = app.world_mut()
        .spawn((
            PlayerId(2),
            Health::default(),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            MitigationPool::default(),
            CritStreak::default(),
            InstanceId(1),
        ))
        .id();
    let _mob = put_player_in_combat(&mut app, ward, 1);

    for _ in 0..3 {
        send_commit(&mut app, MitigationCommitEvent {
            healer,
            ward_player_id: 1,
            quality: 1.0,
            healer_pos: Vec3::new(0.0, 0.0, 0.0),
            healer_facing_yaw: 0.0,
        });
    }

    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap();
    assert!(pool.invuln_until > 0.0, "invuln_until should be set after 3 crits");
    let streak = app.world().entity(healer).get::<CritStreak>().unwrap();
    assert_eq!(streak.count, 0, "streak resets after triggering invuln");
}

#[test]
fn streak_resets_when_ward_changes() {
    let mut app = commits_app();
    let ward_a = spawn_ward(&mut app, 1, 100.0, 1);
    let ward_b = spawn_ward(&mut app, 2, 100.0, 1);
    let healer = app.world_mut()
        .spawn((
            PlayerId(3),
            Health::default(),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            MitigationPool::default(),
            CritStreak::default(),
            InstanceId(1),
        ))
        .id();
    let _mob_a = put_player_in_combat(&mut app, ward_a, 1);
    let _mob_b = put_player_in_combat(&mut app, ward_b, 1);

    // Two crits on ward A.
    for _ in 0..2 {
        send_commit(&mut app, MitigationCommitEvent {
            healer,
            ward_player_id: 1,
            quality: 1.0,
            healer_pos: Vec3::new(0.0, 0.0, 0.0),
            healer_facing_yaw: 0.0,
        });
    }
    assert_eq!(app.world().entity(healer).get::<CritStreak>().unwrap().count, 2);

    // Switch to ward B with one crit — streak resets, then bumps to 1.
    send_commit(&mut app, MitigationCommitEvent {
        healer,
        ward_player_id: 2,
        quality: 1.0,
        healer_pos: Vec3::new(0.0, 0.0, 0.0),
        healer_facing_yaw: 0.0,
    });

    let streak = app.world().entity(healer).get::<CritStreak>().unwrap();
    assert_eq!(streak.count, 1, "streak reset on switch, then bumped by the new commit");
    assert_eq!(streak.last_ward, Some(2));
    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap();
    assert_eq!(pool.invuln_until, 0.0, "no invuln granted when streak was reset by switch");
}

// ── Decay (tick_mitigation_decay) ───────────────────────────────────────────

fn decay_app() -> App {
    let mut app = common::minimal_app();
    app.add_systems(Update, combat::tick_mitigation_decay);
    app
}

#[test]
fn pool_decays_when_healer_has_no_threat() {
    use std::time::Duration;
    let mut app = decay_app();
    let _ward = spawn_ward(&mut app, 1, 100.0, 1);
    let healer = app.world_mut()
        .spawn((
            PlayerId(2),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            PlayerSelectedTarget(Some(SelectedMobOrPlayer::Player(1))),
            MitigationPool { amount: 50.0, invuln_until: 0.0 },
            CritStreak::default(),
            InstanceId(1),
        ))
        .id();
    // No mob has ward on threat list → out of combat.

    common::advance(&mut app, Duration::from_secs(2));

    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    // OOC_DECAY_PER_SEC = 5.0, so 2 seconds → 10 drained → 40 left.
    assert!((pool - 40.0).abs() < 0.1, "pool {pool} expected ~40 after 2s decay");
}

#[test]
fn pool_does_not_decay_while_healer_has_threat() {
    use std::time::Duration;
    let mut app = decay_app();
    let _ward = spawn_ward(&mut app, 1, 100.0, 1);
    let healer = app.world_mut()
        .spawn((
            PlayerId(2),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            PlayerSelectedTarget(Some(SelectedMobOrPlayer::Player(1))),
            MitigationPool { amount: 50.0, invuln_until: 0.0 },
            CritStreak::default(),
            InstanceId(1),
        ))
        .id();
    // Healer is on a mob's threat list — counts as in-combat for decay.
    let _mob = put_player_in_combat(&mut app, healer, 1);

    common::advance(&mut app, Duration::from_secs(2));

    let pool = app.world().entity(healer).get::<MitigationPool>().unwrap().amount;
    assert_eq!(pool, 50.0, "pool unchanged while healer has threat on a mob");
}

#[test]
fn streak_resets_when_healer_has_no_threat() {
    use std::time::Duration;
    let mut app = decay_app();
    let _ward = spawn_ward(&mut app, 1, 100.0, 1);
    let healer = app.world_mut()
        .spawn((
            PlayerId(2),
            CombatState {
                in_combat: true,
                active_stance: Some(RoleStance::Heal),
            },
            PlayerSelectedTarget(Some(SelectedMobOrPlayer::Player(1))),
            MitigationPool::default(),
            CritStreak { count: 2, last_ward: Some(1) },
            InstanceId(1),
        ))
        .id();
    // No mob → out of combat.

    common::advance(&mut app, Duration::from_millis(100));

    let streak = app.world().entity(healer).get::<CritStreak>().unwrap();
    assert_eq!(streak.count, 0, "streak resets out of combat");
}

#[test]
fn invuln_credits_full_hit_amount_to_attacker() {
    let mut app = damage_app();
    let ward = spawn_ward(&mut app, 1, 100.0, 1);
    let healer = spawn_intercessor(&mut app, 2, 1, 50.0, 999.0, 1.0, 1);
    let mob = spawn_mob(&mut app, 100.0, 1);

    send(&mut app, ev(mob, ward, 80.0));

    let threat = app.world().entity(mob).get::<ThreatList>().unwrap();
    let entry = threat.entries.iter().find(|e| e.player_entity == healer).unwrap();
    assert!(
        (entry.threat - 0.4 * 80.0).abs() < 0.01,
        "invuln credits the full prevented amount, got {}",
        entry.threat,
    );
}
