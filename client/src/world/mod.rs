use avian3d::schedule::PhysicsSchedule;
use bevy::prelude::*;
use bevy_tnua::builtins::{TnuaBuiltinJump, TnuaBuiltinWalk};
use bevy_tnua::prelude::*;
use bevy_tnua_avian3d::*;

use crate::plugin::AppState;

pub mod camera;
pub mod controller;
pub mod enemies;
pub mod instance;
pub mod players;
pub mod selection;
pub mod terrain;

pub use selection::SelectedTarget;

/// Maximum interval (seconds) over which dead-reckoning extrapolates a
/// remote entity's position before clamping. Bounds visual drift if the
/// server goes quiet (~10 Hz updates expected).
pub(crate) const DEAD_RECKONING_MAX_EXTRAP_SECS: f32 = 0.3;

#[derive(TnuaScheme)]
#[scheme(basis = TnuaBuiltinWalk)]
pub enum ControlScheme {
    Jump(TnuaBuiltinJump),
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            avian3d::prelude::PhysicsPlugins::default(),
            TnuaControllerPlugin::<ControlScheme>::new(PhysicsSchedule),
            TnuaAvian3dPlugin::new(PhysicsSchedule),
        ));

        app.init_resource::<camera::OrbitState>();
        app.init_resource::<SelectedTarget>();
        app.init_resource::<selection::LeftClickState>();
        app.init_resource::<instance::CurrentInstanceTerrain>();
        app.init_resource::<instance::CurrentInstanceId>();

        app.add_systems(
            OnEnter(AppState::InGame),
            (
                terrain::spawn_terrain,
                terrain::spawn_player,
                terrain::spawn_light,
            ),
        );

        // Observer: insert mesh/material whenever a server-replicated enemy arrives.
        app.add_observer(enemies::on_enemy_replicated);
        // Diagnostic: log when EnemyPosition arrives (even without EnemyMarker).
        app.add_observer(enemies::on_enemy_position_replicated);
        // Observer: render remote player entities when PlayerId replicates.
        app.add_observer(players::on_remote_player_replicated);

        app.add_systems(
            Update,
            (
                instance::handle_instance_entered,
                instance::sync_instance_visibility,
                controller::handle_input,
                camera::update_orbit_camera,
                selection::track_left_drag,
                selection::select_on_click,
                selection::tab_cycle_selection,
                enemies::apply_server_corrections,
                enemies::sync_enemy_positions,
                players::apply_player_corrections,
                players::sync_player_positions,
                players::correct_local_player_position,
            ).chain(),
        );

        // Runs after every Update-schedule UI updater so its `Hidden` write is
        // the last word on this frame's visibility.
        app.add_systems(PostUpdate, instance::hide_out_of_instance_overlays);
    }
}
