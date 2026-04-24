//! Bottom-left log pane. Hosts two tabs — "Chat" (first) and "Combat" —
//! sharing a single frame with a tab strip at the top, a tab-specific
//! content area in the middle, and a resize grip in the top-right corner.
//!
//! - Chat tab: party chat input/display (see `super::chat`). Incoming
//!   `ChatMsg`s append entries; the input field at the bottom captures
//!   typed text while `ChatInputState.focused`.
//! - Combat tab: damage log. Entries arrive via `CombatLogMsg` broadcast
//!   by the server's damage resolver whenever someone in the local
//!   player's party deals or takes damage.
//!
//! Tab switching: click a tab button or press `/` (which forces Chat).
//! Each tab's entries container keeps its own `ScrollPosition` and
//! stick-to-bottom state, so switching away and back preserves the
//! reader's place.
//!
//! Scroll behavior:
//! - Mouse wheel over the pane moves `ScrollPosition` on the active tab's
//!   entries container and sets a resource flag so the orbit camera skips
//!   its zoom update for the same frame.
//! - On new messages, if the user was at (or near) the bottom we snap
//!   back to the new bottom; otherwise we leave their scroll untouched
//!   so they can read history without getting yanked.

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};
use lightyear::prelude::*;

use shared::components::combat::DamageType;
use shared::messages::CombatLogMsg;

use crate::ui::theme;

use super::chat::{ChatEntries, ChatInputField, ChatInputText, CHAT_FONT_SIZE, CHAT_INPUT_HEIGHT};

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

// ── Resize handle ──────────────────────────────────────────────────────────
/// Side of the square grab area in the top-right corner. Hover it to see the
/// diagonal resize cursor; click-drag to resize the pane.
const RESIZE_HANDLE_SIZE: f32 = 14.0;
/// Hard floors/ceilings on pane size. Floors keep the tab strip and at least
/// one row of entries legible; ceilings prevent the pane from swallowing the
/// screen during an accidental big drag.
const PANE_MIN_W: f32 = 220.0;
const PANE_MIN_H: f32 = 120.0;
const PANE_MAX_W: f32 = 900.0;
const PANE_MAX_H: f32 = 700.0;

// ── Marker components & resources ───────────────────────────────────────────
#[derive(Component)]
pub struct CombatLogRoot;

/// Which tab a UI element belongs to. Present on tab buttons and on content
/// containers so a single visibility system can match them by tab.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogTab {
    Chat,
    Combat,
}

#[derive(Component)]
pub struct LogTabButton(pub LogTab);

/// Wraps a tab's entire content subtree. The visibility system flips
/// `Display::Flex` / `Display::None` on these based on the active tab.
#[derive(Component)]
pub struct LogTabContent(pub LogTab);

/// A scrollable entries list. Both Chat and Combat mark their list with this
/// so the generic scroll + pin systems can target the active tab's entries.
#[derive(Component)]
pub struct LogEntriesList(pub LogTab);

/// Per-entries-container stick-to-bottom flag. Kept as a component rather
/// than a resource because each tab tracks its own scroll state — a new chat
/// line arriving while the Combat tab is visible must still pin chat to its
/// own bottom so switching to it shows the newest line.
#[derive(Component)]
pub struct LogEntriesStick {
    pub stuck_to_bottom: bool,
}

impl Default for LogEntriesStick {
    fn default() -> Self { Self { stuck_to_bottom: true } }
}

/// Which tab is currently visible. `/` forces Chat; clicks on tab buttons
/// set this directly.
#[derive(Resource)]
pub struct ActiveLogTab(pub LogTab);

impl Default for ActiveLogTab {
    fn default() -> Self { Self(LogTab::Chat) }
}

/// The Combat tab's entries container (damage log). Kept as a distinct
/// marker from `ChatEntries` so the combat-log receiver targets the right
/// list and so pruning only enumerates combat rows.
#[derive(Component)]
pub struct CombatLogEntries;

/// One combat-log entry row. Tagged so the entries updater can enumerate
/// them and prune the oldest when over cap.
#[derive(Component)]
pub struct CombatLogEntry;

/// Set each frame by combat-log input handlers. The camera orbit system
/// reads this and skips input it would otherwise consume when the cursor is
/// inside the log (`blocks_camera_zoom` during scroll) or while a resize
/// drag is in progress (`blocks_camera_orbit` during click-drag) — so log
/// interaction never doubles as camera control.
#[derive(Resource, Default)]
pub struct UiPointerGuard {
    pub blocks_camera_zoom: bool,
    pub blocks_camera_orbit: bool,
}

/// Marker for the small grab area in the top-right corner of the pane. Held
/// separate from the root because Bevy's `Interaction` only marks the topmost
/// hit, so the root keeps its own hover state for scroll routing.
#[derive(Component)]
pub struct CombatLogResizeHandle;

/// Tracks an in-progress resize drag. `active` carries the cursor and pane
/// dimensions captured at mouse-down, so the drag can compute deltas against
/// a stable reference (not the ever-changing current pane size).
#[derive(Resource, Default)]
pub struct CombatLogResizeDrag {
    pub active: Option<ResizeAnchor>,
}

#[derive(Clone, Copy)]
pub struct ResizeAnchor {
    pub cursor_start: Vec2,
    pub width_start: f32,
    pub height_start: f32,
}

// ── Spawn ───────────────────────────────────────────────────────────────────

/// Build one tab button. `initial_active` seeds `BackgroundColor` to match
/// the current `ActiveLogTab` default — the visibility updater will keep
/// it in sync from then on.
fn spawn_tab_button(commands: &mut Commands, tab: LogTab, label: &str, initial_active: bool) -> Entity {
    let bg = if initial_active { theme::PANEL_BG } else { theme::PANEL_BG_DARK };
    let text_color = if initial_active {
        Color::srgb(0.95, 0.95, 0.95)
    } else {
        Color::srgba(0.70, 0.70, 0.74, 0.70)
    };
    commands
        .spawn((
            LogTabButton(tab),
            Button,
            Node {
                padding: UiRect::axes(Val::Px(TAB_PAD_X), Val::Px(3.0)),
                height: Val::Px(TAB_H),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(bg),
        ))
        .with_children(|tab| {
            tab.spawn((
                Text::new(label),
                TextFont { font_size: 11.0, ..default() },
                TextColor(text_color),
            ));
        })
        .id()
}

pub fn spawn_combat_log(mut commands: Commands) {
    let default_tab = ActiveLogTab::default().0;

    let chat_tab_button = spawn_tab_button(&mut commands, LogTab::Chat, "Chat", default_tab == LogTab::Chat);
    let combat_tab_button = spawn_tab_button(&mut commands, LogTab::Combat, "Combat", default_tab == LogTab::Combat);

    let tab_strip = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(2.0),
                width: Val::Percent(100.0),
                ..default()
            },
        ))
        .add_children(&[chat_tab_button, combat_tab_button])
        .id();

    // ── Chat tab content ──────────────────────────────────────────────────
    let chat_entries = commands
        .spawn((
            ChatEntries,
            LogEntriesList(LogTab::Chat),
            LogEntriesStick::default(),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(ENTRY_ROW_GAP),
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                // See CombatLogEntries: `min_height: 0` is required for
                // `flex_grow` to actually cap at parent size when children
                // overflow, otherwise rows push past the pane border.
                min_height: Val::Px(0.0),
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_DARK),
        ))
        .id();

    // Input field at the bottom of the Chat tab. The text inside is driven
    // by `chat::update_chat_input_display` from `ChatInputState`.
    let chat_input = commands
        .spawn((
            ChatInputField,
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(CHAT_INPUT_HEIGHT),
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                margin: UiRect::top(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_DARK),
        ))
        .with_children(|f| {
            f.spawn((
                ChatInputText,
                Text::new(""),
                TextFont { font_size: CHAT_FONT_SIZE, ..default() },
                TextColor(Color::srgba(0.70, 0.70, 0.74, 0.55)),
            ));
        })
        .id();

    let chat_content = commands
        .spawn((
            LogTabContent(LogTab::Chat),
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                display: if default_tab == LogTab::Chat { Display::Flex } else { Display::None },
                ..default()
            },
        ))
        .add_children(&[chat_entries, chat_input])
        .id();

    // ── Combat tab content ────────────────────────────────────────────────
    let combat_entries = commands
        .spawn((
            CombatLogEntries,
            LogEntriesList(LogTab::Combat),
            LogEntriesStick::default(),
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

    let combat_content = commands
        .spawn((
            LogTabContent(LogTab::Combat),
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                display: if default_tab == LogTab::Combat { Display::Flex } else { Display::None },
                ..default()
            },
        ))
        .add_children(&[combat_entries])
        .id();

    // Invisible grab area in the top-right corner. Detected via `Interaction`
    // and surfaced only via the cursor-icon swap — no visible indicator.
    let resize_handle = commands
        .spawn((
            CombatLogResizeHandle,
            Button,
            Node {
                position_type: PositionType::Absolute,
                // Offset by the root's padding so the grab area sits flush
                // with the pane's top-right outer edge.
                top: Val::Px(-PANE_PADDING),
                right: Val::Px(-PANE_PADDING),
                width: Val::Px(RESIZE_HANDLE_SIZE),
                height: Val::Px(RESIZE_HANDLE_SIZE),
                ..default()
            },
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
        // Handle is added last so it paints over the tab strip's empty area.
        .add_children(&[tab_strip, chat_content, combat_content, resize_handle]);
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

// ── Tab switching ───────────────────────────────────────────────────────────

/// Click handler for the tab strip. Sets the active tab; the visibility
/// updater does the actual show/hide on the next frame.
pub fn handle_log_tab_click(
    interactions: Query<(&Interaction, &LogTabButton), Changed<Interaction>>,
    mut active: ResMut<ActiveLogTab>,
) {
    for (interaction, button) in interactions.iter() {
        if matches!(interaction, Interaction::Pressed) {
            active.0 = button.0;
        }
    }
}

/// Sync `Display` on each tab's content container and repaint the tab
/// buttons to reflect the active tab. Runs only when `ActiveLogTab`
/// changes so we don't thrash style writes every frame.
pub fn update_log_tab_visibility(
    active: Res<ActiveLogTab>,
    mut contents: Query<(&LogTabContent, &mut Node)>,
    mut buttons: Query<(&LogTabButton, &mut BackgroundColor, &Children)>,
    mut texts: Query<&mut TextColor>,
) {
    if !active.is_changed() { return; }

    for (content, mut node) in contents.iter_mut() {
        node.display = if content.0 == active.0 { Display::Flex } else { Display::None };
    }

    for (button, mut bg, children) in buttons.iter_mut() {
        let is_active = button.0 == active.0;
        bg.0 = if is_active { theme::PANEL_BG } else { theme::PANEL_BG_DARK };
        // First text child holds the label.
        for child in children.iter() {
            if let Ok(mut tc) = texts.get_mut(child) {
                tc.0 = if is_active {
                    Color::srgb(0.95, 0.95, 0.95)
                } else {
                    Color::srgba(0.70, 0.70, 0.74, 0.70)
                };
                break;
            }
        }
    }
}

// ── Scroll input ────────────────────────────────────────────────────────────

/// When the cursor is over the log pane, route mouse-wheel input into the
/// active tab's `ScrollPosition` and set a guard flag that tells the camera
/// to skip its zoom update this frame. When the cursor leaves the pane,
/// the flag clears and the camera resumes normal wheel-zoom behavior.
///
/// Also updates the active entries' `LogEntriesStick` flag: goes false when
/// the user scrolls away from the bottom, back true when they return. The
/// pin system in PostUpdate uses this flag to decide whether to re-anchor
/// on new content.
///
/// Reads `ComputedNode` from the previous frame's layout so the clamp
/// range reflects the actual scrollable region. Without this, writing an
/// out-of-range value to `ScrollPosition` wouldn't affect rendering (Bevy
/// clamps internally) but *would* make subsequent wheel deltas invisible
/// because the stored component value is already beyond the real max.
pub fn update_log_scroll(
    pane_q: Query<&Interaction, With<CombatLogRoot>>,
    mut entries_q: Query<(
        &LogEntriesList,
        &ComputedNode,
        &mut ScrollPosition,
        &mut LogEntriesStick,
    )>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    mut guard: ResMut<UiPointerGuard>,
    active: Res<ActiveLogTab>,
) {
    let hovered = pane_q
        .iter()
        .any(|i| matches!(i, Interaction::Hovered | Interaction::Pressed));
    guard.blocks_camera_zoom = hovered;

    for (tab, node, mut scroll, mut stick) in entries_q.iter_mut() {
        if tab.0 != active.0 { continue; }
        let max_scroll = (node.content_size().y - node.size().y).max(0.0);

        if hovered && mouse_scroll.delta.y.abs() > f32::EPSILON {
            let delta = mouse_scroll.delta.y * SCROLL_PX_PER_TICK;
            // Wheel-up (delta > 0) reveals older entries → decrease offset.
            // Wheel-down (delta < 0) reveals newer entries → increase offset.
            scroll.0.y = (scroll.0.y - delta).clamp(0.0, max_scroll);
        }

        // Update stickiness based on the resulting position. Treat "no
        // scroll range" (content fits entirely) as stuck so the pin system
        // will re-snap to the new max once content finally overflows.
        stick.stuck_to_bottom =
            max_scroll <= 0.5 || (max_scroll - scroll.0.y) <= BOTTOM_STICK_SLACK;
    }
}

// ── Resize drag ─────────────────────────────────────────────────────────────

/// Handle hover-cursor swapping and click-drag resizing of the pane from its
/// top-right handle. The pane is bottom-left anchored, so growing width means
/// extending right (positive cursor dx) and growing height means extending up
/// (negative cursor dy, since screen-space y grows downward).
///
/// Cursor icon is inserted/removed only on transitions because inserting the
/// same component every frame would trigger winit's change-detected cursor
/// apply system needlessly.
pub fn handle_combat_log_resize(
    handle_q: Query<&Interaction, With<CombatLogResizeHandle>>,
    mut root_q: Query<&mut Node, With<CombatLogRoot>>,
    mut drag: ResMut<CombatLogResizeDrag>,
    mut guard: ResMut<UiPointerGuard>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut commands: Commands,
    mut cursor_applied: Local<bool>,
) {
    let Ok((window_entity, window)) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };

    let hovered = handle_q
        .iter()
        .any(|i| matches!(i, Interaction::Hovered | Interaction::Pressed));
    let active = drag.active.is_some();
    let want_cursor = hovered || active;

    // Block the orbit camera from consuming mouse motion while a drag is in
    // progress — otherwise click-drag on the handle would spin the camera.
    guard.blocks_camera_orbit = active;

    if want_cursor && !*cursor_applied {
        commands
            .entity(window_entity)
            .insert(CursorIcon::System(SystemCursorIcon::NeswResize));
        *cursor_applied = true;
    } else if !want_cursor && *cursor_applied {
        commands.entity(window_entity).remove::<CursorIcon>();
        *cursor_applied = false;
    }

    // Start drag on press while hovering.
    if !active && hovered && buttons.just_pressed(MouseButton::Left) {
        let Ok(node) = root_q.single() else { return };
        let width = match node.width {
            Val::Px(v) => v,
            _ => PANE_W,
        };
        let height = match node.height {
            Val::Px(v) => v,
            _ => PANE_H,
        };
        drag.active = Some(ResizeAnchor {
            cursor_start: cursor,
            width_start: width,
            height_start: height,
        });
        return;
    }

    // Advance or end an in-progress drag.
    let Some(anchor) = drag.active else { return };
    if !buttons.pressed(MouseButton::Left) {
        drag.active = None;
        return;
    }
    let Ok(mut node) = root_q.single_mut() else { return };
    let dx = cursor.x - anchor.cursor_start.x;
    let dy = cursor.y - anchor.cursor_start.y;
    let new_width = (anchor.width_start + dx).clamp(PANE_MIN_W, PANE_MAX_W);
    let new_height = (anchor.height_start - dy).clamp(PANE_MIN_H, PANE_MAX_H);
    node.width = Val::Px(new_width);
    node.height = Val::Px(new_height);
}

/// PostUpdate sync. Runs after Bevy's `ui_layout_system` so `ComputedNode`
/// reflects the current frame's content (including rows appended earlier
/// in this frame). For every entries container whose `stuck_to_bottom` is
/// set, pin its `ScrollPosition` to the exact max — this keeps the latest
/// entry visible across new appends without breaking user wheel input (the
/// value stays within the valid scroll range, so subsequent deltas move
/// it). Applied to both tabs so background-tab lists stay pinned while
/// you're reading the other tab.
pub fn pin_log_entries_to_bottom(
    mut entries_q: Query<(&ComputedNode, &mut ScrollPosition, &LogEntriesStick)>,
) {
    for (node, mut scroll, stick) in entries_q.iter_mut() {
        if !stick.stuck_to_bottom { continue; }
        let max_scroll = (node.content_size().y - node.size().y).max(0.0);
        if (scroll.0.y - max_scroll).abs() > 0.5 {
            scroll.0.y = max_scroll;
        }
    }
}
