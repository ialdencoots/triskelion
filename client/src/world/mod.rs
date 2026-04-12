use avian3d::schedule::PhysicsSchedule;
use bevy::prelude::*;
use bevy_tnua::builtins::{TnuaBuiltinJump, TnuaBuiltinWalk};
use bevy_tnua::prelude::*;
use bevy_tnua_avian3d::*;

pub mod camera;
pub mod controller;
pub mod terrain;

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

        app.add_systems(
            Startup,
            (terrain::spawn_terrain, terrain::spawn_player, terrain::spawn_light),
        );

        app.add_systems(
            Update,
            (controller::handle_input, camera::update_orbit_camera),
        );
    }
}
