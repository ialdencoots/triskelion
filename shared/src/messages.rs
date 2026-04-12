use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::player::{Class, Subclass};

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

// ── Client → Server ──────────────────────────────────────────────────────────

/// First message from a newly-connected client: choose name, class, and subclass.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RequestSpawnMsg {
    pub name: String,
    pub class: Class,
    pub subclass: Subclass,
}

// ── MapEntities ───────────────────────────────────────────────────────────────

impl MapEntities for PlayerSpawnedMsg {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        self.player_entity = mapper.get_mapped(self.player_entity);
    }
}
