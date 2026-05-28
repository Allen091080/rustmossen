//! Groups messages at API-round boundaries for compaction.

use mossen_types::{Message, Role};

/// Groups messages at API-round boundaries: one group per API round-trip.
/// A boundary fires when a NEW assistant response begins (different
/// message uuid from the prior assistant).
///
/// Replaces the prior human-turn grouping with finer-grained API-round grouping,
/// allowing reactive compact to operate on single-prompt agentic sessions.
pub fn group_messages_by_api_round(messages: &[Message]) -> Vec<Vec<Message>> {
    let mut groups: Vec<Vec<Message>> = Vec::new();
    let mut current: Vec<Message> = Vec::new();
    let mut last_assistant_id: Option<String> = None;

    for msg in messages {
        if msg.role == Role::Assistant {
            let msg_id = msg.uuid.as_deref();
            let is_new_assistant = match (&last_assistant_id, msg_id) {
                (Some(last), Some(current_id)) => last != current_id,
                (None, Some(_)) => false,
                _ => false,
            };

            if is_new_assistant && !current.is_empty() {
                groups.push(std::mem::take(&mut current));
                current.push(msg.clone());
            } else {
                current.push(msg.clone());
            }

            if let Some(id) = msg_id {
                last_assistant_id = Some(id.to_string());
            }
        } else {
            current.push(msg.clone());
        }
    }

    if !current.is_empty() {
        groups.push(current);
    }

    groups
}
