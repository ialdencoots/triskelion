use bevy::prelude::*;

pub mod action_bar;
pub mod frames;
pub mod minigame_anchor;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (frames::spawn_frames, action_bar::spawn_action_bar));
        app.add_systems(
            Update,
            (
                frames::anchor_frames_to_character,
                minigame_anchor::anchor_minigame_to_character,
            ),
        );
    }
}
