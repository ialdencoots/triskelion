//! Bottom-left combat-log pane. Shows damage dealt to/from any member of
//! the local player's party. Entries arrive via `CombatLogMsg` broadcast by
//! the server's damage resolver.
//!
//! The pane is scaffolded with a tab strip even though there's only one tab
//! ("Combat") today — a "Chat" tab will slot in later. The tab is a plain
//! styled `Node` for now; selection logic lives with the future tab system.
//!
//! Scroll behavior:
//! - Mouse wheel over the pane moves `ScrollPosition` on the entries
//!   container and sets a resource flag so the orbit camera skips its zoom
//!   update for the same frame.
//! - On new messages, if the user was at (or near) the bottom we snap back
//!   to the new bottom; otherwise we leave their scroll untouched so they
//!   can read history without getting yanked.

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::DamageType;
use shared::messages::CombatLogMsg;

use crate::ui::theme;

// ── Layout constants ────────────────────────────────────────────────────────
const PANE_W:          f32 = 340.0;
const PANE_H:          f32 = 200.0;
const PANE_BOTTOM:     f32 = 10.0;
const PANE_LEFT:       f32 = 10.0;
const PANE_PADDING:    f32 = 6.0;
const TAB_H:           f32 = 20.0;
const TAB_PAD_X:       f32 = 10.0;
const ENTRY_FONT_SIZE: f32 = 11.0;
const ENTRY_ROW_GAP:   f32 = 2.0;
/// Pixels scrolled per mouse-wheel unit.
const SCROLL_PX_PER_TICK: f32 = 24.0;
/// Pixel threshold within which the user is considered "at the bottom",
/// which enables the stick-to-bottom behavior on new entries.
const BOTTOM_STICK_SLACK: f32 = 4.0;
/// Maximum number of log entries kept visible. Oldest rows are despawned when
/// the list grows past this.
const MAX_ENTRIES:     usize = 40;

// ── Marker components & resources ───────────────────────────────────────────
#[derive(Component)]
pub struct CombatLogRoot;

/// The inner container that holds the log entry rows. Children are added in
/// arrival order — newest at the bottom via `FlexDirection::Column` + natural
/// append. When the child count exceeds `MAX_ENTRIES`, the oldest (first)
/// child is despawned.
#[derive(Component)]
pub struct CombatLogEntries;

/// One log entry row. Tagged so the entries updater can enumerate them and
/// prune the oldest when over cap.
#[derive(Component)]
pub struct CombatLogEntry;

/// Set by `update_combat_log_scroll` each frame. The camera orbit system
/// reads this and skips its mouse-wheel zoom update when the cursor is
/// inside the log, so scrolling the log doesn't also dolly the camera.
#[derive(Resource, Default)]
pub struct UiPointerGuard {
    pub blocks_camera_zoom: bool,
}

/// Tracks whether the user is currently "pinned" to the bottom of the log.
/// True at startup and after any scroll that lands (or stays) at the bottom.
/// Flipped to false when the user scrolls up. While true, a PostUpdate
/// system re-anchors `ScrollPosition` to the newest row after every layout
/// pass — so new messages don't leave the view stale or require manual
/// follow-down, but history reading isn't yanked when new hits arrive.
#[derive(Resource)]
pub struct CombatLogScrollState {
    pub stuck_to_bottom: bool,
}

impl Default for CombatLogScrollState {
    fn default() -> Self {
        Self { stuck_to_bottom: true }
    }
}

// ── Spawn ───────────────────────────────────────────────────────────────────

pub fn spawn_combat_log(mut commands: Commands) {
    let combat_tab = commands
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(TAB_PAD_X), Val::Px(3.0)),
                height: Val::Px(TAB_H),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(theme::PANEL_BG),
        ))
        .with_children(|tab| {
            tab.spawn((
                Text::new("Combat"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.95, 0.95, 0.95)),
            ));
        })
        .id();

    let tab_strip = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(2.0),
                width: Val::Percent(100.0),
                ..default()
            },
        ))
        .add_children(&[combat_tab])
        .id();

    let entries = commands
        .spawn((
            CombatLogEntries,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(ENTRY_ROW_GAP),
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                // Without min_height: 0, flex items default to a content-based
                // minimum size, which prevents flex_grow from ever shrinking
                // the container below the total child height — rows would
                // spill past the pane border. Zero lets the scroll container
                // actually cap at its parent-granted size.
                min_height: Val::Px(0.0),
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_DARK),
        ))
        .id();

    commands
        .spawn((
            CombatLogRoot,
            // Button marker adds `Interaction`, which is how we detect hover
            // for routing mouse-wheel scroll into the log instead of the
            // camera. We don't have any click behavior on the pane itself.
            Button,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(PANE_BOTTOM),
                left: Val::Px(PANE_LEFT),
                width: Val::Px(PANE_W),
                height: Val::Px(PANE_H),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(PANE_PADDING)),
                row_gap: Val::Px(0.0),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG),
        ))
        .add_children(&[tab_strip, entries]);
}

// ── Colors ──────────────────────────────────────────────────────────────────

/// Base text color for the non-numeric part of a log line. Outgoing (a party
/// member is the attacker) uses the damage-type palette, softened a touch for
/// readability as body text. Incoming (enemy attacker) uses muted red.
fn sentence_color(ty: DamageType, attacker_is_player: bool) -> Color {
    if attacker_is_player {
        match ty {
            DamageType::Physical => Color::srgb(0.92, 0.88, 0.78),
            DamageType::Arcane   => Color::srgb(0.88, 0.85, 0.95),
            DamageType::Nature   => Color::srgb(0.82, 0.92, 0.82),
        }
    } else {
        Color::srgb(0.95, 0.78, 0.78)
    }
}

/// Saturated color for the damage number itself. Matches the floating-number
/// fill palette so an at-a-glance read (type → hue) is consistent between
/// the two surfaces.
fn damage_color(ty: DamageType) -> Color {
    match ty {
        DamageType::Physical => Color::srgb(1.00, 0.72, 0.30),
        DamageType::Arcane   => Color::srgb(0.70, 0.48, 1.00),
        DamageType::Nature   => Color::srgb(0.44, 0.88, 0.48),
    }
}

// ── Receive + render ────────────────────────────────────────────────────────

/// Consume every queued `CombatLogMsg` and append a new row. Each row is a
/// `Text` node with one `TextSpan` child so the damage number can render in
/// a distinct color inline. Scroll stickiness is handled in the PostUpdate
/// sync system (`pin_combat_log_to_bottom`) rather than here, because this
/// system reads the previous frame's layout and can't know the new max yet.
pub fn receive_combat_log_msgs(
    mut link_query: Query<&mut MessageReceiver<CombatLogMsg>>,
    entries_query: Query<Entity, With<CombatLogEntries>>,
    mut commands: Commands,
) {
    let Ok(mut receiver) = link_query.single_mut() else { return };
    let Ok(entries_entity) = entries_query.single() else { return };

    for msg in receiver.receive() {
        let amount = msg.amount.round().max(0.0) as i32;
        let crit_mark = if msg.is_crit { "!" } else { "" };
        let sentence = format!("{} hits {} for ", msg.attacker_name, msg.target_name);
        let number = format!("{amount}{crit_mark}");
        let sent_color = sentence_color(msg.ty, msg.attacker_is_player);
        let num_color = damage_color(msg.ty);

        let row = commands
            .spawn((
                CombatLogEntry,
                Text::new(sentence),
                TextFont { font_size: ENTRY_FONT_SIZE, ..default() },
                TextColor(sent_color),
            ))
            .id();
        let number_span = commands
            .spawn((
                TextSpan::new(number),
                TextFont { font_size: ENTRY_FONT_SIZE, ..default() },
                TextColor(num_color),
            ))
            .id();
        commands.entity(row).add_child(number_span);
        commands.entity(entries_entity).add_child(row);
    }
}

/// After new entries are appended, prune the oldest rows if the total
/// exceeds `MAX_ENTRIES`. Runs after `receive_combat_log_msgs` so both
/// append and prune land in the same frame.
pub fn prune_combat_log_entries(
    entries_query: Query<&Children, With<CombatLogEntries>>,
    entry_query: Query<(), With<CombatLogEntry>>,
    mut commands: Commands,
) {
    let Ok(children) = entries_query.single() else { return };
    let log_children: Vec<Entity> = children.iter().filter(|c| entry_query.contains(*c)).collect();
    let excess = log_children.len().saturating_sub(MAX_ENTRIES);
    for &old in log_children.iter().take(excess) {
        commands.entity(old).despawn();
    }
}

// ── Scroll input ────────────────────────────────────────────────────────────

/// When the cursor is over the combat log pane, route mouse-wheel input into
/// the log's `ScrollPosition` and set a guard flag that tells the camera to
/// skip its zoom update this frame. When the cursor leaves the pane, the
/// flag clears and the camera resumes normal wheel-zoom behavior.
///
/// Also updates `stuck_to_bottom`: goes false when the user scrolls away
/// from the bottom, back true when they return. The pin system in
/// PostUpdate uses this flag to decide whether to re-anchor on new content.
///
/// Reads `ComputedNode` from the previous frame's layout so the clamp range
/// reflects the actual scrollable region. Without this, writing an
/// out-of-range value to `ScrollPosition` wouldn't affect rendering (Bevy
/// clamps internally) but *would* make subsequent wheel deltas invisible
/// because the stored component value is already beyond the real max.
pub fn update_combat_log_scroll(
    pane_q: Query<&Interaction, With<CombatLogRoot>>,
    mut entries_q: Query<(&ComputedNode, &mut ScrollPosition), With<CombatLogEntries>>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    mut guard: ResMut<UiPointerGuard>,
    mut state: ResMut<CombatLogScrollState>,
) {
    let hovered = pane_q
        .iter()
        .any(|i| matches!(i, Interaction::Hovered | Interaction::Pressed));
    guard.blocks_camera_zoom = hovered;

    let Ok((node, mut scroll)) = entries_q.single_mut() else { return };
    let max_scroll = (node.content_size().y - node.size().y).max(0.0);

    if hovered && mouse_scroll.delta.y.abs() > f32::EPSILON {
        let delta = mouse_scroll.delta.y * SCROLL_PX_PER_TICK;
        // Wheel-up (delta > 0) reveals older entries → decrease offset.
        // Wheel-down (delta < 0) reveals newer entries → increase offset.
        scroll.0.y = (scroll.0.y - delta).clamp(0.0, max_scroll);
    }

    // Update stickiness based on the resulting position. Treat "no scroll
    // range" (content fits entirely) as stuck so the pin system will
    // re-snap to the new max once content finally overflows.
    state.stuck_to_bottom =
        max_scroll <= 0.5 || (max_scroll - scroll.0.y) <= BOTTOM_STICK_SLACK;
}

/// PostUpdate sync. Runs after Bevy's `ui_layout_system` so `ComputedNode`
/// reflects the current frame's content (including rows appended earlier
/// in this frame). If the user is stuck to the bottom, pin `ScrollPosition`
/// to the exact max — this keeps the latest entry visible across new
/// appends without breaking user wheel input (the value stays within the
/// valid scroll range, so subsequent deltas move it).
pub fn pin_combat_log_to_bottom(
    state: Res<CombatLogScrollState>,
    mut entries_q: Query<(&ComputedNode, &mut ScrollPosition), With<CombatLogEntries>>,
) {
    if !state.stuck_to_bottom { return; }
    let Ok((node, mut scroll)) = entries_q.single_mut() else { return };
    let max_scroll = (node.content_size().y - node.size().y).max(0.0);
    if (scroll.0.y - max_scroll).abs() > 0.5 {
        scroll.0.y = max_scroll;
    }
}
