use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Tags every player and mob entity with the runtime instance they belong to.
/// Replicated once at spawn — instances never change mid-session for an entity.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct InstanceId(pub u32);
