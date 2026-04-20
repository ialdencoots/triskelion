use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::{AbilityCooldowns, CombatState, Health, ReplicatedThreatList};
use shared::components::enemy::{EnemyMarker, EnemyPosition};
use shared::components::instance::InstanceId;
use shared::components::minigame::arc::{ArcState, SecondaryArcState};
use shared::components::player::{
    PlayerId, PlayerPosition, PlayerSelectedTarget, PlayerVelocity, RoleStance, SelectedMobOrPlayer,
};
use shared::inputs::PlayerInput;
use shared::instances::{find_def, sample_height};
use shared::messages::SelectTargetMsg;

use super::connection::PlayerEntityLink;
use super::instances::InstanceRegistry;
use super::minigame::process_arc_commit;

// ── Server-only threat components ────────────────────────────────────────────

/// One entry in a mob's server-side threat table.
#[derive(Clone, Debug)]
pub struct ThreatEntry {
    pub player_entity: Entity,
    pub threat: f32,
}

/// Server-side threat table on mob entities.  Never replicated — use
/// `ReplicatedThreatList` for the client-facing version.
#[derive(Component, Default, Debug)]
pub struct ThreatList {
    pub entries: Vec<ThreatEntry>,
}

/// A time-limited additive threat multiplier bonus from any source.
#[derive(Clone, Debug)]
pub struct ThreatBonus {
    pub multiplier: f32,
    pub expires_at: f32,
}

/// Server-side per-player threat generation modifiers.  Never replicated.
/// `role_multiplier` is derived from the active stance each tick.
/// `bonuses` holds stacking temporary bonuses from any source (DAG, skills, etc.)
#[derive(Component, Debug)]
pub struct ThreatModifiers {
    pub role_multiplier: f32,
    pub bonuses: Vec<ThreatBonus>,
}

impl Default for ThreatModifiers {
    fn default() -> Self {
        Self {
            role_multiplier: 0.5,
            bonuses: Vec::new(),
        }
    }
}

impl ThreatModifiers {
    pub fn effective_multiplier(&self) -> f32 {
        let bonus_sum: f32 = self.bonuses.iter().map(|b| b.multiplier).sum();
        self.role_multiplier * (1.0 + bonus_sum)
    }
}

// ── Threat systems ────────────────────────────────────────────────────────────

const PLAYER_SPEED: f32 = 6.0;

/// Read buffered `PlayerInput` messages from all connected clients and apply
/// movement and stance changes to their server-side components.
pub fn process_player_inputs(
    time: Res<Time>,
    reg: Res<InstanceRegistry>,
    mut link_query: Query<(&PlayerEntityLink, &mut MessageReceiver<PlayerInput>)>,
    mut player_query: Query<(
        &mut PlayerPosition,
        &mut PlayerVelocity,
        &mut CombatState,
        Option<&InstanceId>,
        Option<&mut ArcState>,
        Option<&mut SecondaryArcState>,
        Option<&PlayerSelectedTarget>,
        &ThreatModifiers,
    ), With<PlayerId>>,
    mut enemy_query: Query<(&EnemyPosition, Option<&mut Health>, Option<&mut ThreatList>), With<EnemyMarker>>,
) {
    let dt = time.delta_secs();

    for (link, mut receiver) in link_query.iter_mut() {
        // Use the most recent input in the buffer; ignore stale ones.
        let last_input = receiver.receive().last();
        let Ok((mut pos, mut vel, mut combat, iid_opt, mut arc_opt, mut secondary_arc_opt, target_opt, threat_mods)) = player_query.get_mut(link.0) else { continue };

        if let Some(input) = last_input {
            // ── Movement ───────────────────────────────────────────────────────
            // XYZ position comes directly from the client's physics body so the
            // server position stays in sync with what the client actually simulates.
            // Velocity is derived from the input direction for dead-reckoning by
            // other clients; it is not used to integrate position server-side.
            let raw = Vec3::new(input.movement.x, 0.0, -input.movement.y);
            let dir = if raw.length_squared() > 0.01 { raw.normalize() } else { Vec3::ZERO };

            vel.vx = dir.x * PLAYER_SPEED;
            vel.vz = dir.z * PLAYER_SPEED;
            vel.vy = input.vy;
            pos.x = input.x;
            pos.z = input.z;
            pos.y = input.y;

            // ── Stance transitions ─────────────────────────────────────────────
            if input.abilities.exit_stance {
                combat.active_stance = None;
            } else if let Some(stance) = input.abilities.enter_stance {
                // Pressing the active stance's key again toggles back to neutral.
                combat.active_stance = if combat.active_stance == Some(stance) {
                    None
                } else {
                    Some(stance)
                };
            }

            // ── Arc commits (Physical class) ───────────────────────────────────
            if input.minigame.action_1 {
                info!("[ARC] action_1 received — stance={:?} arc_present={}",
                    combat.active_stance, arc_opt.is_some());
            }
            if input.minigame.action_1 && combat.active_stance.is_some() {
                if let Some(ref mut arc) = arc_opt {
                    let was_unlocked = !arc.in_lockout;
                    info!("[ARC] commit attempt — was_unlocked={was_unlocked} quality={:.2} facing_yaw={:.2}",
                        arc.last_commit_quality, input.facing_yaw);
                    process_arc_commit(arc);
                    if was_unlocked {
                        apply_arc_damage(
                            arc.last_commit_quality,
                            input.facing_yaw,
                            link.0,
                            threat_mods,
                            &pos,
                            target_opt,
                            &mut enemy_query,
                        );
                    }
                }
            }
            if input.minigame.action_2 && combat.active_stance == Some(RoleStance::Dps) {
                if let Some(ref mut secondary) = secondary_arc_opt {
                    let was_unlocked = !secondary.0.in_lockout;
                    process_arc_commit(&mut secondary.0);
                    if was_unlocked {
                        apply_arc_damage(
                            secondary.0.last_commit_quality,
                            input.facing_yaw,
                            link.0,
                            threat_mods,
                            &pos,
                            target_opt,
                            &mut enemy_query,
                        );
                    }
                }
            }
        } else {
            // No input this tick — zero XZ motion.
            vel.vx = 0.0;
            vel.vz = 0.0;
            vel.vy = 0.0;
            let floor_y = if let Some(iid) = iid_opt {
                if let Some(live) = reg.instances.get(&iid.0) {
                    let def = find_def(live.kind);
                    sample_height(&live.noise, pos.x, pos.z, &def.terrain) + 1.1
                } else {
                    pos.y
                }
            } else {
                pos.y
            };
            if pos.y <= floor_y + 0.1 {
                pos.y = floor_y;
            }
        }
    }
}

/// Reads `SelectTargetMsg` from each client and updates `PlayerSelectedTarget`
/// on their player entity.
pub fn process_target_selections(
    mut link_query: Query<(&PlayerEntityLink, &mut MessageReceiver<SelectTargetMsg>)>,
    mut player_query: Query<&mut PlayerSelectedTarget>,
) {
    for (link, mut receiver) in link_query.iter_mut() {
        if let Some(msg) = receiver.receive().last() {
            if let Ok(mut target) = player_query.get_mut(link.0) {
                target.0 = msg.0;
            }
        }
    }
}

/// Decrement all ability cooldowns by the elapsed fixed-timestep delta.
pub fn tick_ability_cooldowns(time: Res<Time>, mut query: Query<&mut AbilityCooldowns>) {
    let dt = time.delta_secs();
    for mut cd in query.iter_mut() {
        cd.mobility_cd = (cd.mobility_cd - dt).max(0.0);
        cd.cc_cd = (cd.cc_cd - dt).max(0.0);
        cd.taunt_cd = (cd.taunt_cd - dt).max(0.0);
        cd.interrupt_cd = (cd.interrupt_cd - dt).max(0.0);
        cd.stance_cd = (cd.stance_cd - dt).max(0.0);
    }
}

/// Derives each player's threat `role_multiplier` from their active stance and
/// prunes any expired timed bonuses.
pub fn apply_stance_multipliers(
    time: Res<Time>,
    mut query: Query<(&CombatState, &mut ThreatModifiers)>,
) {
    let now = time.elapsed_secs();
    for (combat, mut modifiers) in query.iter_mut() {
        modifiers.role_multiplier = match combat.active_stance {
            Some(RoleStance::Tank) => 2.0,
            Some(RoleStance::Dps)  => 1.0,
            Some(RoleStance::Heal) => 0.0,
            None                   => 0.5,
        };
        modifiers.bonuses.retain(|b| b.expires_at == 0.0 || b.expires_at > now);
    }
}

/// Syncs each mob's server-side `ThreatList` to its replicated `ReplicatedThreatList`,
/// translating internal `Entity` keys to `PlayerId` (u64) for clients.
/// Entries are sorted descending by threat.
pub fn sync_replicated_threat_list(
    mut mob_query: Query<(&ThreatList, &mut ReplicatedThreatList)>,
    player_query: Query<(Entity, &PlayerId)>,
) {
    for (threat_list, mut replicated) in mob_query.iter_mut() {
        let mut entries: Vec<(u64, f32)> = threat_list.entries.iter()
            .filter_map(|entry| {
                player_query.get(entry.player_entity).ok()
                    .map(|(_, pid)| (pid.0, entry.threat))
            })
            .collect();
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        replicated.entries = entries;
    }
}

/// Adds `amount` threat for `player` on `list`, creating an entry if needed.
fn add_threat(list: &mut ThreatList, player: Entity, amount: f32) {
    if let Some(entry) = list.entries.iter_mut().find(|e| e.player_entity == player) {
        entry.threat += amount;
    } else {
        list.entries.push(ThreatEntry { player_entity: player, threat: amount });
    }
}

/// Apply threat to a mob from a damage event.
/// Called by the damage system when it is implemented.
pub fn apply_damage_threat(
    threat_list: &mut ThreatList,
    attacker: Entity,
    damage: f32,
    modifiers: &ThreatModifiers,
) {
    add_threat(threat_list, attacker, damage * modifiers.effective_multiplier());
}

// ── Arc damage ────────────────────────────────────────────────────────────────

/// Applies arc commit damage to the player's selected mob target, provided the
/// player is facing it within the allowed cone.
///
/// Damage = BASE_DAMAGE × commit_quality.  Quality is 0–1 based on how close
/// the dot was to the nadir at commit time (see `process_arc_commit`).
fn apply_arc_damage(
    quality: f32,
    facing_yaw: f32,
    attacker: Entity,
    threat_mods: &ThreatModifiers,
    pos: &PlayerPosition,
    target: Option<&PlayerSelectedTarget>,
    enemy_query: &mut Query<(&EnemyPosition, Option<&mut Health>, Option<&mut ThreatList>), With<EnemyMarker>>,
) {
    const BASE_DAMAGE: f32 = 15.0;
    /// Melee reach in world units.
    const MELEE_RANGE: f32 = 3.0;
    // Allow commits within a 120° cone (cos 60° = 0.5) — forgiving but not trivial.
    const FACING_COS_MIN: f32 = 0.5;

    let mob_entity = match target.and_then(|t| t.0.as_ref()) {
        Some(SelectedMobOrPlayer::Mob(e)) => *e,
        other => {
            info!("[ARC DMG] no mob target — target={other:?}");
            return;
        }
    };

    let Ok((enemy_pos, health_opt, threat_list_opt)) = enemy_query.get_mut(mob_entity) else {
        info!("[ARC DMG] {mob_entity:?} not in enemy_query — missing EnemyMarker or EnemyPosition; \
               ensure server was restarted after Health was added to spawn_mob");
        return;
    };
    let Some(mut health) = health_opt else {
        info!("[ARC DMG] {mob_entity:?} found but has no Health component — restart the server so mobs spawn with Health");
        return;
    };
    if !health.is_alive() {
        info!("[ARC DMG] target already dead");
        return;
    }

    // Distance and facing checks in the XZ plane.
    let player_xz = Vec3::new(pos.x, 0.0, pos.z);
    let enemy_xz  = Vec3::new(enemy_pos.x, 0.0, enemy_pos.z);
    let delta      = enemy_xz - player_xz;
    let dist       = delta.length();

    if dist > MELEE_RANGE {
        info!("[ARC DMG] out of range — dist={dist:.2} max={MELEE_RANGE}");
        return;
    }

    let to_target = delta.normalize_or_zero();
    if to_target == Vec3::ZERO {
        info!("[ARC DMG] player standing on mob");
        return;
    }

    let forward = Quat::from_rotation_y(facing_yaw) * Vec3::NEG_Z;
    let dot = forward.dot(to_target);
    if dot < FACING_COS_MIN {
        info!("[ARC DMG] not facing target — dot={dot:.2} min={FACING_COS_MIN}");
        return;
    }

    let dmg = BASE_DAMAGE * quality;
    health.current = (health.current - dmg).max(0.0);
    info!("[ARC DMG] hit! dmg={dmg:.1} quality={quality:.2} dist={dist:.2} dot={dot:.2} hp={:.1}/{:.1}",
        health.current, health.max);
    if let Some(mut tl) = threat_list_opt {
        apply_damage_threat(&mut tl, attacker, dmg, threat_mods);
    }
}

/// Distribute healing threat across all engaged mobs in an instance.
/// Called when a heal is applied. Rate: 0.4 per point healed, full amount per mob.
pub fn distribute_healing_threat(
    healer: Entity,
    heal_amount: f32,
    instance_id: u32,
    mob_query: &mut Query<(&mut ThreatList, &InstanceId)>,
) {
    let generated = heal_amount * 0.4;
    for (mut threat_list, mob_iid) in mob_query.iter_mut() {
        if mob_iid.0 != instance_id || threat_list.entries.is_empty() {
            continue;
        }
        add_threat(&mut threat_list, healer, generated);
    }
}
