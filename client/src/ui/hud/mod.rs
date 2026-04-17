use bevy::prelude::*;

pub mod action_bar;
pub mod enemy_bars;
pub mod frames;
pub mod group_frame;
pub mod instance_button;
pub mod minigame_anchor;
pub mod selection_indicator;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                frames::spawn_frames,
                action_bar::spawn_action_bar,
                minigame_anchor::spawn_minigame_root,
                group_frame::spawn_group_frame,
                instance_button::spawn_instance_button,
                selection_indicator::spawn_selection_indicator,
            ),
        );
        app.add_observer(enemy_bars::on_enemy_bar_added);
        app.add_observer(group_frame::on_party_member_added);
        app.add_observer(group_frame::on_party_member_removed);
        app.add_systems(
            Update,
            (
                frames::update_player_name,
                frames::update_target_avatar,
                frames::update_target_frame_visibility,
                frames::update_target_name,
                frames::update_target_health_fill,
                enemy_bars::update_enemy_bars,
                selection_indicator::update_selection_indicator,
                group_frame::update_party_rows,
                group_frame::update_party_row_fade,
                group_frame::handle_party_row_interaction,
                action_bar::update_stance_highlight,
                instance_button::handle_instance_button,
            ),
        );
    }
}
