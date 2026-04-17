use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::server::*;

use shared::settings;

use crate::systems::{combat, connection, enemy, instances::InstanceRegistry, instances, minigame};

pub struct ServerGamePlugin;

impl Plugin for ServerGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InstanceRegistry>();
        app.add_systems(Startup, start_listening);
        app.add_observer(enemy::on_server_started);
        app.add_observer(debug_server_linked);
        app.add_observer(debug_server_started);

        // Handle new link-of connections (add replication/messaging components).
        app.add_observer(connection::on_link_of_added);
        // Diagnostic: fires on the same trigger as Lightyear's handle_connection.
        app.add_observer(connection::debug_connected_sender);
        // Handle full client connections (spawn player entity).
        app.add_observer(connection::on_client_connected);
        // Handle disconnections (despawn player entity).
        app.add_observer(connection::on_client_disconnected);

        // Combat and minigame tick in FixedUpdate for deterministic simulation.
        app.add_systems(
            FixedUpdate,
            (
                // Spawn requests must be processed before input so the
                // PlayerEntityLink exists when process_player_inputs runs.
                connection::process_spawn_requests,
                connection::process_instance_requests,
                instances::tick_instance_teardown,
                enemy::tick_enemy_walk,
                combat::process_player_inputs,
                combat::tick_ability_cooldowns,
                minigame::tick_arc_states,
                minigame::tick_dag_states,
                minigame::tick_bar_fill_states,
                minigame::tick_wave_interference_states,
                minigame::tick_value_lock_states,
                minigame::tick_heartbeat_states,
            ),
        );
    }
}

fn debug_server_linked(trigger: On<Add, Linked>) {
    info!("[SERVER] Linked added to {:?} — UDP socket is bound", trigger.event_target());
}

fn debug_server_started(trigger: On<Add, Started>) {
    info!("[SERVER] Started added to {:?} — server is fully up", trigger.event_target());
}

/// Spawns the server transport entity and begins listening for UDP connections.
fn start_listening(mut commands: Commands) {
    let addr = settings::server_listen_addr();
    let entity = commands.spawn((
        Name::new("GameServer"),
        NetcodeServer::new(
            NetcodeConfig::default().with_protocol_id(settings::PROTOCOL_ID),
        ),
        ServerUdpIo::default(),
        LocalAddr(addr),
    )).id();
    info!("[SERVER] Spawned NetcodeServer entity {entity:?}, triggering Start on {addr}");
    // Trigger the Lightyear startup chain: Start → LinkStart → Linked → Started.
    // Enemies are spawned in an On<Add, Started> observer so Replicate finds
    // an active server and fills per_sender_state immediately.
    commands.entity(entity).trigger(|e| Start { entity: e });
}
