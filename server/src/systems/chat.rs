//! Party chat routing. Clients send `ChatSendMsg`; the server stamps the
//! sender's `PlayerName` onto a fresh `ChatMsg` and echoes it to every
//! connected player whose `GroupId` matches the sender's group (including
//! the sender themselves, so their own line appears in their pane).

use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::components::player::{GroupId, PlayerName};
use shared::messages::{ChatMsg, ChatSendMsg};

use super::connection::PlayerEntityLink;

/// Maximum bytes we keep from a client-supplied chat line before dropping it.
/// Long enough for casual messages; keeps a misbehaving client from flooding.
const MAX_CHAT_LEN: usize = 240;

pub fn process_chat_messages(
    mut receivers: Query<(&PlayerEntityLink, &mut MessageReceiver<ChatSendMsg>)>,
    player_names: Query<&PlayerName>,
    groups: Query<&GroupId>,
    mut senders: Query<(&PlayerEntityLink, &mut MessageSender<ChatMsg>)>,
) {
    // Drain all pending chats first so we don't hold both a mut receiver and
    // mut sender query on the same entity.
    let mut outgoing: Vec<(u32, ChatMsg)> = Vec::new();

    for (link, mut rx) in receivers.iter_mut() {
        for msg in rx.receive() {
            let text = msg.text.trim();
            if text.is_empty() { continue; }
            let trimmed: String = text.chars().take(MAX_CHAT_LEN).collect();

            let Ok(name) = player_names.get(link.0) else { continue };
            let Ok(group) = groups.get(link.0) else { continue };

            outgoing.push((
                group.0,
                ChatMsg { sender_name: name.0.clone(), text: trimmed },
            ));
        }
    }

    if outgoing.is_empty() { return; }

    for (sender_group, payload) in outgoing {
        for (link, mut tx) in senders.iter_mut() {
            if let Ok(g) = groups.get(link.0) {
                if g.0 == sender_group {
                    tx.send::<GameChannel>(payload.clone());
                }
            }
        }
    }
}
