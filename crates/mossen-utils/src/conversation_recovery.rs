use std::path::Path;
use serde::{Deserialize, Serialize};

/// Turn interruption state.
#[derive(Debug, Clone)]
pub enum TurnInterruptionState {
    None,
    InterruptedPrompt { message: serde_json::Value },
}

/// Deserialization result.
#[derive(Debug, Clone)]
pub struct DeserializeResult {
    pub messages: Vec<serde_json::Value>,
    pub turn_interruption_state: TurnInterruptionState,
}

/// Teleport remote response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeleportRemoteResponse {
    pub log: Vec<serde_json::Value>,
    pub branch: Option<String>,
}

/// Permission modes.
const PERMISSION_MODES: &[&str] = &[
    "default",
    "plan",
    "bypassPermissions",
    "autoApprove",
];

/// Transforms legacy attachment types to current types for backward compatibility.
fn migrate_legacy_attachment_types(message: &mut serde_json::Value, cwd: &str) {
    let msg_type = message.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if msg_type != "attachment" {
        return;
    }

    let attachment_type = message
        .get("attachment")
        .and_then(|a| a.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    match attachment_type.as_str() {
        "new_file" => {
            if let Some(attachment) = message.get_mut("attachment") {
                attachment["type"] = serde_json::Value::String("file".to_string());
                if let Some(filename) = attachment.get("filename").and_then(|f| f.as_str()) {
                    let display_path = make_relative(filename, cwd);
                    attachment["displayPath"] = serde_json::Value::String(display_path);
                }
            }
        }
        "new_directory" => {
            if let Some(attachment) = message.get_mut("attachment") {
                attachment["type"] = serde_json::Value::String("directory".to_string());
                if let Some(path) = attachment.get("path").and_then(|p| p.as_str()) {
                    let display_path = make_relative(path, cwd);
                    attachment["displayPath"] = serde_json::Value::String(display_path);
                }
            }
        }
        _ => {
            // Backfill displayPath for attachments from old sessions
            if let Some(attachment) = message.get_mut("attachment") {
                if attachment.get("displayPath").is_none() {
                    let path = attachment
                        .get("filename")
                        .or_else(|| attachment.get("path"))
                        .or_else(|| attachment.get("skillDir"))
                        .and_then(|v| v.as_str())
                        .map(|p| make_relative(p, cwd));
                    if let Some(dp) = path {
                        attachment["displayPath"] = serde_json::Value::String(dp);
                    }
                }
            }
        }
    }
}

/// Deserialize messages from a log file into the format expected by the REPL.
pub fn deserialize_messages(
    serialized_messages: Vec<serde_json::Value>,
    cwd: &str,
) -> Vec<serde_json::Value> {
    deserialize_messages_with_interrupt_detection(serialized_messages, cwd).messages
}

/// Like deserialize_messages, but also detects whether the session was interrupted mid-turn.
pub fn deserialize_messages_with_interrupt_detection(
    mut serialized_messages: Vec<serde_json::Value>,
    cwd: &str,
) -> DeserializeResult {
    // Transform legacy attachment types
    for msg in &mut serialized_messages {
        migrate_legacy_attachment_types(msg, cwd);
    }

    // Strip invalid permissionMode values
    let valid_modes: std::collections::HashSet<&str> = PERMISSION_MODES.iter().copied().collect();
    for msg in &mut serialized_messages {
        if msg.get("type").and_then(|v| v.as_str()) == Some("user") {
            if let Some(mode) = msg.get("permissionMode").and_then(|v| v.as_str()) {
                if !valid_modes.contains(mode) {
                    msg.as_object_mut()
                        .map(|o| o.remove("permissionMode"));
                }
            }
        }
    }

    // Filter out unresolved tool uses
    let filtered = filter_unresolved_tool_uses(&serialized_messages);

    // Filter out orphaned thinking-only assistant messages
    let filtered = filter_orphaned_thinking_only_messages(&filtered);

    // Filter out whitespace-only assistant messages
    let filtered = filter_whitespace_only_assistant_messages(&filtered);

    // Detect turn interruption
    let interruption = detect_turn_interruption(&filtered);

    let mut messages = filtered;
    let turn_interruption_state = match interruption {
        InternalInterruptionState::None => TurnInterruptionState::None,
        InternalInterruptionState::InterruptedTurn => {
            // Append synthetic continuation message
            let continuation = serde_json::json!({
                "type": "user",
                "message": {"content": "Continue from where you left off."},
                "isMeta": true,
                "uuid": uuid::Uuid::new_v4().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let state = TurnInterruptionState::InterruptedPrompt {
                message: continuation.clone(),
            };
            messages.push(continuation);
            state
        }
        InternalInterruptionState::InterruptedPrompt { message } => {
            TurnInterruptionState::InterruptedPrompt { message }
        }
    };

    // Append synthetic assistant sentinel after last user message
    let last_relevant_idx = messages
        .iter()
        .rposition(|m| {
            let t = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
            t != "system" && t != "progress"
        });

    if let Some(idx) = last_relevant_idx {
        let msg_type = messages[idx]
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if msg_type == "user" {
            let sentinel = serde_json::json!({
                "type": "assistant",
                "message": {"content": [{"type": "text", "text": "(no response requested)"}]},
                "uuid": uuid::Uuid::new_v4().to_string(),
            });
            messages.insert(idx + 1, sentinel);
        }
    }

    DeserializeResult {
        messages,
        turn_interruption_state,
    }
}

/// Internal interruption state.
enum InternalInterruptionState {
    None,
    InterruptedTurn,
    InterruptedPrompt { message: serde_json::Value },
}

/// Detect turn interruption based on last message.
fn detect_turn_interruption(messages: &[serde_json::Value]) -> InternalInterruptionState {
    if messages.is_empty() {
        return InternalInterruptionState::None;
    }

    // Find last turn-relevant message
    let last_msg = messages
        .iter()
        .rev()
        .find(|m| {
            let t = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if t == "system" || t == "progress" {
                return false;
            }
            if t == "assistant" && m.get("isApiErrorMessage").and_then(|v| v.as_bool()) == Some(true) {
                return false;
            }
            true
        });

    let last_msg = match last_msg {
        Some(m) => m,
        None => return InternalInterruptionState::None,
    };

    let msg_type = last_msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "assistant" => InternalInterruptionState::None,
        "user" => {
            let is_meta = last_msg.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false);
            let is_compact = last_msg
                .get("isCompactSummary")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_meta || is_compact {
                return InternalInterruptionState::None;
            }

            // Check if it's a tool_result message
            if is_tool_use_result_message(last_msg) {
                return InternalInterruptionState::InterruptedTurn;
            }

            // Plain text user prompt
            InternalInterruptionState::InterruptedPrompt {
                message: last_msg.clone(),
            }
        }
        "attachment" => InternalInterruptionState::InterruptedTurn,
        _ => InternalInterruptionState::None,
    }
}

/// Check if a message is a tool_result message.
fn is_tool_use_result_message(msg: &serde_json::Value) -> bool {
    if let Some(content) = msg.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
        content.iter().all(|block| {
            block.get("type").and_then(|t| t.as_str()) == Some("tool_result")
        }) && !content.is_empty()
    } else {
        false
    }
}

/// Filter unresolved tool uses from messages.
fn filter_unresolved_tool_uses(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    // Collect all tool_result IDs
    let mut resolved_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for msg in messages {
        if msg.get("type").and_then(|v| v.as_str()) == Some("user") {
            if let Some(content) = msg.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                for block in content {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                        if let Some(id) = block.get("tool_use_id").and_then(|v| v.as_str()) {
                            resolved_ids.insert(id.to_string());
                        }
                    }
                }
            }
        }
    }

    // Filter out assistant messages with unresolved tool_uses
    let mut result = Vec::new();
    for msg in messages {
        if msg.get("type").and_then(|v| v.as_str()) == Some("assistant") {
            if let Some(content) = msg.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                let has_unresolved = content.iter().any(|block| {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                            return !resolved_ids.contains(id);
                        }
                    }
                    false
                });
                if has_unresolved {
                    // Filter content to remove unresolved tool_uses
                    let mut filtered_msg = msg.clone();
                    if let Some(content_arr) = filtered_msg
                        .get_mut("message")
                        .and_then(|m| m.get_mut("content"))
                        .and_then(|c| c.as_array_mut())
                    {
                        content_arr.retain(|block| {
                            if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                                    return resolved_ids.contains(id);
                                }
                            }
                            true
                        });
                        if content_arr.is_empty() {
                            continue;
                        }
                    }
                    result.push(filtered_msg);
                    continue;
                }
            }
        }
        result.push(msg.clone());
    }
    result
}

/// Filter orphaned thinking-only assistant messages.
fn filter_orphaned_thinking_only_messages(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .filter(|msg| {
            if msg.get("type").and_then(|v| v.as_str()) != Some("assistant") {
                return true;
            }
            if let Some(content) = msg.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                // Keep if has any non-thinking content
                content.iter().any(|block| {
                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    block_type != "thinking" && block_type != "redacted_thinking"
                })
            } else {
                true
            }
        })
        .cloned()
        .collect()
}

/// Filter whitespace-only assistant messages.
fn filter_whitespace_only_assistant_messages(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .filter(|msg| {
            if msg.get("type").and_then(|v| v.as_str()) != Some("assistant") {
                return true;
            }
            if let Some(content) = msg.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                // Keep if any block has non-whitespace content
                content.iter().any(|block| {
                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            return !text.trim().is_empty();
                        }
                    }
                    true
                })
            } else {
                true
            }
        })
        .cloned()
        .collect()
}

/// Restore skill state from invoked_skills attachments in messages.
pub fn restore_skill_state_from_messages(
    messages: &[serde_json::Value],
    add_skill: &dyn Fn(&str, &str, &str),
) {
    for message in messages {
        if message.get("type").and_then(|v| v.as_str()) != Some("attachment") {
            continue;
        }
        if let Some(attachment) = message.get("attachment") {
            if attachment.get("type").and_then(|v| v.as_str()) == Some("invoked_skills") {
                if let Some(skills) = attachment.get("skills").and_then(|s| s.as_array()) {
                    for skill in skills {
                        let name = skill.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let path = skill.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let content = skill.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        if !name.is_empty() && !path.is_empty() && !content.is_empty() {
                            add_skill(name, path, content);
                        }
                    }
                }
            }
        }
    }
}

/// Load conversation for resume from various sources.
pub async fn load_conversation_for_resume(
    source: Option<&str>,
    source_jsonl_file: Option<&str>,
    cwd: &str,
) -> Option<DeserializeResult> {
    // In a real implementation, would load from session storage
    // This is a stub that returns None when no source is available
    if source.is_none() && source_jsonl_file.is_none() {
        return None;
    }

    // Load messages from source
    let messages: Vec<serde_json::Value> = Vec::new();
    if messages.is_empty() {
        return None;
    }

    Some(deserialize_messages_with_interrupt_detection(messages, cwd))
}

/// Make a path relative to cwd.
fn make_relative(path: &str, cwd: &str) -> String {
    if let Ok(rel) = Path::new(path).strip_prefix(cwd) {
        rel.to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

/// 加载结果（对应 TS `loadMessagesFromJsonlPath` 返回的对象）。
#[derive(Debug, Clone, Default)]
pub struct LoadedMessages {
    pub messages: Vec<serde_json::Value>,
    pub session_id: Option<String>,
    pub project_path: Option<String>,
}

/// 对应 TS `loadMessagesFromJsonlPath`：从 jsonl 文件加载并解析出最近一条会话
/// 链上的消息序列。
pub async fn load_messages_from_jsonl_path(path: &str) -> LoadedMessages {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(_) => return LoadedMessages::default(),
    };
    let mut messages: Vec<serde_json::Value> = Vec::new();
    let mut session_id: Option<String> = None;
    let mut project_path: Option<String> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if session_id.is_none() {
            session_id = value
                .get("sessionId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        if project_path.is_none() {
            project_path = value
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        messages.push(value);
    }
    LoadedMessages { messages, session_id, project_path }
}
