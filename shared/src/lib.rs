#![allow(dead_code)]

pub mod channels;
pub mod components;
pub mod inputs;
pub mod instances;
pub mod messages;
pub mod settings;
pub mod terrain;

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;

pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        use channels::{GameChannel, PositionChannel};
        use components::{
            combat::{AbilityCooldowns, CombatState, Health, ReplicatedThreatList},
            enemy::{BossMarker, EnemyMarker, EnemyName, EnemyPosition, EnemyVelocity, MobTarget},
            instance::InstanceId,
            minigame::{
                arc::{ArcState, SecondaryArcState}, bar_fill::BarFillState, dag::DagState,
                heartbeat::HeartbeatState, value_lock::ValueLockState,
                wave_interference::WaveInterferenceState,
            },
            player::{GroupId, PlayerClass, PlayerId, PlayerName, PlayerPosition, PlayerSelectedTarget, PlayerSubclass, PlayerVelocity},
        };
        use messages::{InstanceEnteredMsg, PlayerDespawnedMsg, PlayerSpawnedMsg, RequestInstanceMsg, RequestSpawnMsg, SelectTargetMsg};

        // ── Channels ──────────────────────────────────────────────────────────
        app.add_channel::<GameChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            send_frequency: Duration::default(),
            priority: 1.0,
        })
        .add_direction(NetworkDirection::Bidirectional);

        app.add_channel::<PositionChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            send_frequency: Duration::default(),
            priority: 0.5,
        })
        .add_direction(NetworkDirection::ServerToClient);

        // ── Messages ──────────────────────────────────────────────────────────
        app.register_message::<inputs::PlayerInput>()
            .add_direction(NetworkDirection::ClientToServer);

        app.register_message::<RequestSpawnMsg>()
            .add_direction(NetworkDirection::ClientToServer);

        app.register_message::<RequestInstanceMsg>()
            .add_direction(NetworkDirection::ClientToServer);

        app.register_message::<SelectTargetMsg>()
            .add_direction(NetworkDirection::ClientToServer)
            .add_map_entities();

        app.register_message::<PlayerSpawnedMsg>()
            .add_direction(NetworkDirection::ServerToClient)
            .add_map_entities();

        app.register_message::<PlayerDespawnedMsg>()
            .add_direction(NetworkDirection::ServerToClient);

        app.register_message::<InstanceEnteredMsg>()
            .add_direction(NetworkDirection::ServerToClient);

        // ── Components: player identity (replicated once at spawn) ─────────────
        app.register_component::<GroupId>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..default()
            });
        app.register_component::<PlayerId>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..default()
            });
        app.register_component::<PlayerName>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..default()
            });
        app.register_component::<PlayerClass>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..default()
            });
        app.register_component::<PlayerSubclass>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..default()
            });

        // ── Components: enemies ───────────────────────────────────────────────
        app.register_component::<EnemyMarker>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..default()
            });
        app.register_component::<BossMarker>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..default()
            });
        app.register_component::<EnemyName>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..default()
            });
        app.register_component::<EnemyPosition>();
        app.register_component::<EnemyVelocity>();
        app.register_component::<MobTarget>();

        // ── Components: player position/velocity (replicated every tick) ────────
        app.register_component::<PlayerPosition>();
        app.register_component::<PlayerVelocity>();

        // ── Components: combat state (changes during play) ────────────────────
        app.register_component::<Health>();
        app.register_component::<CombatState>();
        app.register_component::<AbilityCooldowns>();
        app.register_component::<ReplicatedThreatList>();

        app.register_component::<PlayerSelectedTarget>();

        // ── Components: instance identity ────────────────────────────────────
        // Not replicate_once — players can transition instances mid-session,
        // and other clients need the updated InstanceId to filter visibility.
        app.register_component::<InstanceId>();

        // ── Components: minigame state (server-authoritative, no prediction) ──
        // These are never predicted on the client — the client renders exactly
        // what the server sends. Ghost arc history and other visual flourishes
        // are maintained locally by the client UI systems.
        app.register_component::<ArcState>();
        app.register_component::<SecondaryArcState>();
        app.register_component::<DagState>();
        app.register_component::<BarFillState>();
        app.register_component::<WaveInterferenceState>();
        app.register_component::<ValueLockState>();
        app.register_component::<HeartbeatState>();
    }
}
