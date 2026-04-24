use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::server::*;

use shared::settings;

use crate::systems::{combat, connection, enemy, instances::InstanceRegistry, instances, minigame};
#[cfg(debug_assertions)]
use crate::systems::dev_dots;

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
        // Damage flow: emitters (process_player_inputs, tick_dots, enemy
        // ability/cast systems) run before the apply_damage_events resolver
        // so events are drained same-tick. apply_disruption_events consumes
        // DisruptionEvent messages after damage is applied — the minigame
        // ticks that follow then simulate on the updated state.
        app.add_systems(
            FixedUpdate,
            (
                // Spawn requests must be processed before input so the
                // PlayerEntityLink exists when process_player_inputs runs.
                connection::process_spawn_requests,
                connection::process_instance_requests,
                instances::tick_instance_teardown,
                enemy::tick_enemy_walk,
                (
                    combat::process_player_inputs,
                    combat::tick_dots,
                    // tick_enemy_casts runs before tick_enemy_abilities so
                    // resolving a cast this tick doesn't prevent the mob
                    // from initiating new actions until the next tick (the
                    // remove<EnemyCast> command flushes at the next sync
                    // point regardless, so this is correctness-neutral —
                    // the order just makes the data flow read naturally).
                    enemy::tick_enemy_casts,
                    enemy::tick_enemy_abilities,
                    combat::apply_damage_events,
                    combat::apply_disruption_events,
                ).chain(),
                combat::process_target_selections,
                combat::tick_ability_cooldowns,
                // Stance multipliers must update before sync so threat display
                // reflects the current role immediately.
                combat::apply_stance_multipliers,
                combat::sync_replicated_threat_list,
                minigame::tick_arc_states,
                minigame::tick_secondary_arc_states,
                minigame::tick_cube_states,
                minigame::tick_bar_fill_states,
                minigame::tick_wave_interference_states,
                minigame::tick_value_lock_states,
                minigame::tick_heartbeat_states,
            ),
        );

        // DEV-ONLY (debug builds only): consume DevApplyDotMsg from clients and
        // attach a typed DoT. Must run between input processing (which sets
        // selection) and tick_dots (which consumes attached DoTs).
        #[cfg(debug_assertions)]
        app.add_systems(
            FixedUpdate,
            dev_dots::process_dev_dot_requests
                .after(combat::process_player_inputs)
                .before(combat::tick_dots),
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
