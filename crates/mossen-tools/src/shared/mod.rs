//! Shared utilities used across multiple tools.
//!
//! - `git_operation_tracking`: Git operation detection and analytics tracking
//! - `spawn_multi_agent`: Teammate spawning logic (split-pane, separate window, in-process)

pub mod git_operation_tracking;
pub mod spawn_multi_agent;

// ---------------------------------------------------------------------------
// TS-mirror — `tools/utils.ts` exports.
// ---------------------------------------------------------------------------

/// `tools/utils.ts` `tagMessagesWithToolUseID`.
pub fn tag_messages_with_tool_use_id(messages: &mut [serde_json::Value], tool_use_id: &str) {
    for msg in messages.iter_mut() {
        if let Some(map) = msg.as_object_mut() {
            map.insert(
                "tool_use_id".to_string(),
                serde_json::Value::String(tool_use_id.to_string()),
            );
        }
    }
}

/// `tools/utils.ts` `getToolUseIDFromParentMessage`.
pub fn get_tool_use_id_from_parent_message(parent_message: &serde_json::Value) -> Option<String> {
    let content = parent_message.get("content").and_then(|c| c.as_array())?;
    for block in content.iter().rev() {
        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
            if let Some(id) = block.get("id").and_then(|i| i.as_str()) {
                return Some(id.to_string());
            }
        }
    }
    None
}
