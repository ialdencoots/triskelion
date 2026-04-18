use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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
