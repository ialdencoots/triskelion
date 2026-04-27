use bevy::prelude::*;
use bevy::prelude::UiMaterialPlugin;

use crate::plugin::AppState;
use crate::ui::arc::ArcMaterial;

pub mod action_bar;
pub mod chat;
pub mod combat_log;
pub mod damage_numbers;
pub mod enemy_bars;
pub mod frames;
pub mod group_frame;
pub mod heal_numbers;
pub mod health_bar;
pub mod instance_button;
pub mod minigame_anchor;
pub mod selection_indicator;
pub mod target_panel;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<ArcMaterial>::default());
        app.init_resource::<target_panel::ThreatDisplayData>();
        app.init_resource::<action_bar::SlotClickPulse>();
        app.init_resource::<combat_log::UiPointerGuard>();
        app.init_resource::<combat_log::ActiveLogTab>();
        app.init_resource::<combat_log::CombatLogResizeDrag>();
        app.init_resource::<chat::ChatInputState>();
        app.add_systems(
            OnEnter(AppState::InGame),
            (
                frames::spawn_frames,
                action_bar::spawn_action_bar,
                minigame_anchor::spawn_minigame_root,
                // Arc overlay must run after minigame_anchor so MinigameRoot exists.
                crate::ui::arc::spawn_arc_overlay
                    .after(minigame_anchor::spawn_minigame_root),
                crate::ui::cube::spawn_cube_overlay
                    .after(minigame_anchor::spawn_minigame_root),
                crate::ui::grid::spawn_grid_overlay
                    .after(minigame_anchor::spawn_minigame_root),
                crate::ui::streak::spawn_streak_counters
                    .after(minigame_anchor::spawn_minigame_root),
                group_frame::spawn_group_frame,
                instance_button::spawn_instance_button,
                selection_indicator::spawn_selection_indicator,
                target_panel::spawn_target_panel,
                combat_log::spawn_combat_log,
            ),
        );
        app.add_observer(enemy_bars::on_enemy_bar_added);
        app.add_observer(enemy_bars::on_enemy_bar_removed);
        app.add_observer(crate::ui::arc::on_arc_state_added);
        app.add_observer(crate::ui::arc::on_secondary_arc_state_added);
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
                frames::update_player_health_fill,
                enemy_bars::update_enemy_bars,
                damage_numbers::spawn_damage_numbers,
                damage_numbers::update_damage_numbers,
                selection_indicator::update_selection_indicator,
                group_frame::update_party_rows,
                group_frame::update_party_row_fade,
                group_frame::handle_party_row_interaction,
                action_bar::update_stance_highlight,
                action_bar::update_keybind_labels,
                action_bar::handle_action_slot_click,
                instance_button::handle_instance_button,
                // Threat panel: compute first, then apply.
                target_panel::compute_threat_display,
                target_panel::apply_threat_panel.after(target_panel::compute_threat_display),
                target_panel::handle_tot_interaction,
            ),
        );
        // Mitigation pool bar + tier markers + heal numbers live in their
        // own add_systems because the main HUD tuple above is at Bevy's
        // tuple-arity limit.
        app.add_systems(
            Update,
            (
                frames::update_player_mitigation_bar,
                frames::update_player_mitigation_markers,
                heal_numbers::spawn_heal_numbers,
                heal_numbers::update_heal_numbers,
            ),
        );

        // Log pane (Chat + Combat tabs) lives in its own add_systems call
        // because the main HUD tuple is already at Bevy's tuple-arity limit.
        app.add_systems(
            Update,
            (
                combat_log::update_log_scroll,
                combat_log::receive_combat_log_msgs,
                combat_log::prune_combat_log_entries,
                chat::receive_chat_msgs,
                chat::prune_chat_entries,
                combat_log::handle_log_tab_click,
                // Chat input runs before tab visibility so pressing `/`
                // selects the Chat tab on the same frame it focuses.
                chat::handle_chat_input,
                combat_log::update_log_tab_visibility
                    .after(chat::handle_chat_input)
                    .after(combat_log::handle_log_tab_click),
                chat::update_chat_input_display.after(chat::handle_chat_input),
            ),
        );
        // Runs before the orbit camera so the drag-active guard flag it sets
        // is visible on the same frame — otherwise the mouse-down frame would
        // leak motion into camera rotation.
        app.add_systems(
            Update,
            combat_log::handle_combat_log_resize
                .before(crate::world::camera::update_orbit_camera),
        );
        // Pin-to-bottom sync runs after Bevy's UI layout so ComputedNode
        // reflects the current frame's appended rows. Using PostUpdate means
        // the stored ScrollPosition matches the real scrollable max, which
        // keeps subsequent wheel deltas effective.
        app.add_systems(
            PostUpdate,
            combat_log::pin_log_entries_to_bottom
                .after(bevy::ui::UiSystems::Layout),
        );
    }
}
