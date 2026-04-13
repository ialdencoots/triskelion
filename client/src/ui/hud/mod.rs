use bevy::prelude::*;

pub mod action_bar;
pub mod enemy_bars;
pub mod frames;
pub mod minigame_anchor;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (frames::spawn_frames, action_bar::spawn_action_bar));
        app.add_observer(enemy_bars::on_enemy_bar_added);
        app.add_systems(
            Update,
            (
                frames::anchor_frames_to_character,
                frames::update_target_frame_visibility,
                minigame_anchor::anchor_minigame_to_character,
                enemy_bars::update_enemy_bars,
            ),
        );
    }
}
