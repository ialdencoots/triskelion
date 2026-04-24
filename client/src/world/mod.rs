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
pub mod telegraph;
pub mod terrain;

pub use selection::SelectedTarget;

/// Maximum interval (seconds) over which dead-reckoning extrapolates a
/// remote entity's position before clamping. Bounds visual drift if the
/// server goes quiet (~10 Hz updates expected).
pub(crate) const DEAD_RECKONING_MAX_EXTRAP_SECS: f32 = 0.3;

/// Client-only extrapolation baseline for a server-replicated entity.
///
/// When a server position/velocity update arrives, the baseline is refreshed
/// with the authoritative values and the local wall-clock time. Each rendered
/// frame the caller computes `base_pos + vel * elapsed` to produce a smooth
/// 60+ Hz visual between ~10 Hz server updates.
///
/// Shared by remote players and enemies; `vel_y` is 0 for enemies since the
/// server doesn't replicate enemy vertical velocity. The correction-application
/// and per-frame sync systems stay separate because remote players smooth-chase
/// the target (lerp) while enemies snap to floor-clamped XZ.
#[derive(Component)]
pub struct DeadReckoning {
    /// Authoritative position at the time of the last server update.
    pub base_pos: Vec3,
    /// XZ velocity received from the server at that same update.
    pub vel: Vec2,
    /// Vertical velocity received from the server; used to extrapolate Y
    /// between updates so jumps appear smooth rather than stepping per update.
    /// Stays 0 for entities whose server state has no vertical velocity.
    pub vel_y: f32,
    /// `Time::elapsed_secs()` (client wall clock) when the update was applied.
    pub base_time: f32,
}

impl DeadReckoning {
    /// Seconds since the last server update, clamped to the extrapolation budget.
    #[inline]
    pub fn elapsed(&self, now: f32) -> f32 {
        (now - self.base_time).clamp(0.0, DEAD_RECKONING_MAX_EXTRAP_SECS)
    }

    /// Extrapolated 3-D position: `base_pos + vel * elapsed`. Y is meaningful
    /// only for entities whose server state replicates `vel_y` (remote players).
    #[inline]
    pub fn extrapolated_at(&self, now: f32) -> Vec3 {
        let dt = self.elapsed(now);
        Vec3::new(
            self.base_pos.x + self.vel.x * dt,
            self.base_pos.y + self.vel_y * dt,
            self.base_pos.z + self.vel.y * dt,
        )
    }
}

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
        // Observers: spawn/despawn telegraph disc when EnemyCast arrives / is removed.
        app.add_observer(telegraph::on_enemy_cast_added);
        app.add_observer(telegraph::on_enemy_cast_removed);

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
                telegraph::update_telegraph_visuals,
            ).chain(),
        );

        // Runs after every Update-schedule UI updater so its `Hidden` write is
        // the last word on this frame's visibility.
        app.add_systems(PostUpdate, instance::hide_out_of_instance_overlays);
    }
}
