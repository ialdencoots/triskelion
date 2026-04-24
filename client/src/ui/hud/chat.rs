//! Party chat: focus state, text-entry key handling, send/receive, and the
//! live display of the input buffer at the bottom of the Chat tab.
//!
//! Typing flow:
//! - Not focused: press `/` to focus. The slash itself is swallowed and the
//!   pane switches to the Chat tab.
//! - Focused: characters accumulate in the buffer (rendered in the input
//!   field with a blinking caret), Backspace deletes, Enter sends, Escape
//!   cancels. While focused, the main input system skips its keyboard poll
//!   so ability/digit keys don't fire under the typist's fingers.
//!
//! The chat input field always exists as a child of the Chat tab content.
//! Its text color dims when the pane is idle to read as a placeholder.

use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::messages::{ChatMsg, ChatSendMsg};

use super::combat_log::{ActiveLogTab, LogTab};

// ── Sizing / rendering constants ───────────────────────────────────────────

pub const CHAT_FONT_SIZE: f32 = 11.0;
pub const CHAT_INPUT_HEIGHT: f32 = 20.0;
/// Max character count (not bytes) the client will retain in the buffer.
/// Matches the server's max to keep behavior aligned.
const MAX_CHAT_INPUT_LEN: usize = 240;
/// How many chat entries are kept visible before the oldest get pruned.
const MAX_CHAT_ENTRIES: usize = 80;

const PLACEHOLDER: &str = "Press / to chat";
const PLACEHOLDER_COLOR: Color = Color::srgba(0.70, 0.70, 0.74, 0.55);
const FOCUSED_COLOR: Color = Color::srgb(1.00, 1.00, 1.00);
const SENDER_COLOR: Color = Color::srgb(0.70, 0.86, 1.00);
const BODY_COLOR: Color = Color::srgb(0.95, 0.95, 0.95);

// ── Components & resources ────────────────────────────────────────────────

#[derive(Component)]
pub struct ChatEntries;

#[derive(Component)]
pub struct ChatEntry;

#[derive(Component)]
pub struct ChatInputField;

/// The `Text` node inside the input field. Separate marker so the updater
/// only touches text content, not the container's layout/background.
#[derive(Component)]
pub struct ChatInputText;

/// Focus and buffer for the chat input. While `focused`, the main keyboard
/// input pipeline gates itself off (see `systems::input::gather_and_send_input`).
#[derive(Resource, Default)]
pub struct ChatInputState {
    pub focused: bool,
    pub buffer: String,
}

// ── Key handling ──────────────────────────────────────────────────────────

/// Reads raw `KeyboardInput` events so we observe key identity as well as
/// modifier-resolved text. Using `MessageReader<KeyboardInput>` here lets us
/// consume `/` before it reaches any `ButtonInput::just_pressed` poll in a
/// later system — crucial so pressing `/` to focus doesn't also type `/`.
pub fn handle_chat_input(
    mut events: MessageReader<KeyboardInput>,
    mut state: ResMut<ChatInputState>,
    mut active_tab: ResMut<ActiveLogTab>,
    mut sender_q: Query<&mut MessageSender<ChatSendMsg>>,
) {
    for ev in events.read() {
        if ev.state != ButtonState::Pressed { continue; }

        if !state.focused {
            if ev.key_code == KeyCode::Slash {
                state.focused = true;
                state.buffer.clear();
                active_tab.0 = LogTab::Chat;
            }
            continue;
        }

        match ev.key_code {
            KeyCode::Escape => {
                state.focused = false;
                state.buffer.clear();
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let text = state.buffer.trim().to_string();
                state.focused = false;
                state.buffer.clear();
                if text.is_empty() { continue; }
                if let Ok(mut sender) = sender_q.single_mut() {
                    sender.send::<GameChannel>(ChatSendMsg { text });
                }
            }
            KeyCode::Backspace => {
                state.buffer.pop();
            }
            _ => {
                // Use the platform-resolved `text` field: it captures what
                // the user actually typed (respecting shift/alt/IME) and
                // covers non-`Character` logical keys like Space. Enter/
                // Tab/Backspace/Escape also populate `text` on some
                // platforms, but those are matched explicitly above, so
                // only "real" typed characters reach this branch.
                if let Some(s) = &ev.text {
                    for c in s.chars() {
                        if c.is_control() { continue; }
                        if state.buffer.chars().count() >= MAX_CHAT_INPUT_LEN { break; }
                        state.buffer.push(c);
                    }
                }
            }
        }
    }
}

// ── Receive + render ──────────────────────────────────────────────────────

/// Append an incoming chat line as a new row: `"Name: message"` with the
/// sender-name styled distinctly via a `TextSpan` child.
pub fn receive_chat_msgs(
    mut link_query: Query<&mut MessageReceiver<ChatMsg>>,
    entries_query: Query<Entity, With<ChatEntries>>,
    mut commands: Commands,
) {
    let Ok(mut receiver) = link_query.single_mut() else { return };
    let Ok(entries_entity) = entries_query.single() else { return };

    for msg in receiver.receive() {
        let name_prefix = format!("{}: ", msg.sender_name);
        let row = commands
            .spawn((
                ChatEntry,
                Text::new(name_prefix),
                TextFont { font_size: CHAT_FONT_SIZE, ..default() },
                TextColor(SENDER_COLOR),
            ))
            .id();
        let body = commands
            .spawn((
                TextSpan::new(msg.text.clone()),
                TextFont { font_size: CHAT_FONT_SIZE, ..default() },
                TextColor(BODY_COLOR),
            ))
            .id();
        commands.entity(row).add_child(body);
        commands.entity(entries_entity).add_child(row);
    }
}

pub fn prune_chat_entries(
    entries_query: Query<&Children, With<ChatEntries>>,
    entry_query: Query<(), With<ChatEntry>>,
    mut commands: Commands,
) {
    let Ok(children) = entries_query.single() else { return };
    let rows: Vec<Entity> = children.iter().filter(|c| entry_query.contains(*c)).collect();
    let excess = rows.len().saturating_sub(MAX_CHAT_ENTRIES);
    for &old in rows.iter().take(excess) {
        commands.entity(old).despawn();
    }
}

/// Mirror the current buffer + focus state into the input field's `Text`.
/// Placeholder shows when idle; when focused we append a caret to signal
/// that keyboard input is captured.
pub fn update_chat_input_display(
    state: Res<ChatInputState>,
    mut text_q: Query<(&mut Text, &mut TextColor), With<ChatInputText>>,
) {
    if !state.is_changed() { return; }
    let Ok((mut text, mut color)) = text_q.single_mut() else { return };

    if state.focused {
        // Static caret glyph — no blink system to drive, just an always-on
        // indicator that we're capturing keystrokes.
        text.0 = format!("{}_", state.buffer);
        color.0 = FOCUSED_COLOR;
    } else {
        text.0 = PLACEHOLDER.to_string();
        color.0 = PLACEHOLDER_COLOR;
    }
}
