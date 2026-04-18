use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::{AbilityCooldowns, CombatState, ReplicatedThreatList};
use shared::components::instance::InstanceId;
use shared::components::player::{PlayerId, PlayerPosition, PlayerSelectedTarget, PlayerVelocity, RoleStance};
use shared::inputs::PlayerInput;
use shared::instances::{find_def, sample_height};
use shared::messages::SelectTargetMsg;

use super::connection::PlayerEntityLink;
use super::instances::InstanceRegistry;

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
    mut player_query: Query<(&mut PlayerPosition, &mut PlayerVelocity, &mut CombatState, Option<&InstanceId>), With<PlayerId>>,
) {
    let dt = time.delta_secs();

    for (link, mut receiver) in link_query.iter_mut() {
        // Use the most recent input in the buffer; ignore stale ones.
        let last_input = receiver.receive().last();
        let Ok((mut pos, mut vel, mut combat, iid_opt)) = player_query.get_mut(link.0) else { continue };

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

/// Apply threat to a mob from a damage event.
/// Called by the damage system when it is implemented.
pub fn apply_damage_threat(
    threat_list: &mut ThreatList,
    attacker: Entity,
    damage: f32,
    modifiers: &ThreatModifiers,
) {
    let generated = damage * modifiers.effective_multiplier();
    if let Some(entry) = threat_list.entries.iter_mut().find(|e| e.player_entity == attacker) {
        entry.threat += generated;
    } else {
        threat_list.entries.push(ThreatEntry { player_entity: attacker, threat: generated });
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
        if let Some(entry) = threat_list.entries.iter_mut().find(|e| e.player_entity == healer) {
            entry.threat += generated;
        } else {
            threat_list.entries.push(ThreatEntry { player_entity: healer, threat: generated });
        }
    }
}
