use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::combat::DamageType;
use crate::components::player::{Class, SelectedMobOrPlayer, Subclass};
use crate::instances::{InstanceKind, TerrainConfig};

// ── Server → Client ──────────────────────────────────────────────────────────

/// Sent to all clients when a new player fully spawns.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerSpawnedMsg {
    pub client_id: u64,
    pub name: String,
    pub class: Class,
    pub subclass: Subclass,
    /// The replicated entity handle so the client can locate the player entity.
    pub player_entity: Entity,
}

/// Sent to all clients when a player leaves.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerDespawnedMsg {
    pub client_id: u64,
}

/// Broadcast after a `DamageEvent` resolves: tells clients to pop a floating
/// number above `target`. Amount is post-resist final damage. Type drives color.
/// `is_crit` selects the crit visual treatment (larger, white-hot, "!" suffix).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DamageNumberMsg {
    pub target: Entity,
    pub amount: f32,
    pub ty: DamageType,
    pub is_crit: bool,
}

/// One entry in the combat log. Sent to every player in the attacker's or
/// target's party when a `DamageEvent` resolves. Names are serialized as
/// strings so receivers don't need the attacker or target entity replicated.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CombatLogMsg {
    pub attacker_name: String,
    pub target_name:   String,
    /// Post-resist, post-modifier damage landed on `target`.
    pub amount:        f32,
    pub ty:            DamageType,
    pub is_crit:       bool,
    /// True if the hit was dealt by a player (either the receiver or a party
    /// member). Used to pick log-line color — outgoing vs. incoming read.
    pub attacker_is_player: bool,
}

/// A party-chat line echoed to every player in the sender's group. The server
/// stamps `sender_name` from the sender's replicated `PlayerName` so clients
/// don't have to trust or look up the sender entity.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChatMsg {
    pub sender_name: String,
    pub text: String,
}

/// Sent to a client when the server assigns them to an instance.
/// The client uses `terrain` to rebuild its terrain mesh and `spawn_{x,z}` to
/// teleport the local physics body.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InstanceEnteredMsg {
    pub instance_id: u32,
    pub kind: InstanceKind,
    pub terrain: TerrainConfig,
    pub spawn_x: f32,
    pub spawn_z: f32,
}

// ── Client → Server ──────────────────────────────────────────────────────────

/// Sent by the client when the local player's selection changes.
/// Uses `SelectedMobOrPlayer` so player targets are identified by stable `PlayerId`
/// rather than an entity ID that may differ across clients.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SelectTargetMsg(pub Option<SelectedMobOrPlayer>);

/// First message from a newly-connected client: choose name, class, and subclass.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RequestSpawnMsg {
    pub name: String,
    pub class: Class,
    pub subclass: Subclass,
}

/// Sent by a client to request entering a specific instance.
/// The server finds or creates the group's instance of that kind, assigns the
/// player to it, and replies with `InstanceEnteredMsg`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RequestInstanceMsg {
    pub kind: InstanceKind,
}

/// Sent by a client to post a chat line to their party. The server fills in
/// the sender's name before echoing a `ChatMsg` back to each party member.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChatSendMsg {
    pub text: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// DEV-ONLY (debug builds only)
// Keys 4/5/6 on the client send this to apply a DoT of the chosen type to the
// player's currently selected mob. Gated out of release builds via cfg.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(debug_assertions)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DevApplyDotMsg {
    pub ty: DamageType,
}

// ── MapEntities ───────────────────────────────────────────────────────────────

impl MapEntities for PlayerSpawnedMsg {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        self.player_entity = mapper.get_mapped(self.player_entity);
    }
}

impl MapEntities for SelectTargetMsg {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        if let Some(SelectedMobOrPlayer::Mob(ref mut e)) = self.0 {
            *e = mapper.get_mapped(*e);
        }
    }
}

impl MapEntities for DamageNumberMsg {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        self.target = mapper.get_mapped(self.target);
    }
}
