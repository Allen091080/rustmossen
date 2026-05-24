//! Message data shared by the TUI rendering pipeline.
//!
//! This module contains transcript input data and naming helpers only. Drawing
//! stays in widgets, while semantic shaping stays in `render_model`.

/// Message role/type discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    User,
    Assistant,
    System,
    CommandOutput,
    Progress,
    Attachment,
    ToolUse,
    ToolResult,
    SkillInvocation,
}

/// A message entry received from the app/agent loop before semantic rendering.
#[derive(Debug, Clone)]
pub struct MessageData {
    pub message_type: MessageType,
    pub content: String,
    pub timestamp: Option<String>,
    pub is_streaming: bool,
    pub tool_name: Option<String>,
    pub is_error: bool,
    /// Reasoning/`<think>` content peeled out of the model's streamed text.
    pub thinking: Option<String>,
    /// `Instant` at which the message stream finished. Used to drive the
    /// visibility policy for thinking content in Layer 1/2.
    pub thinking_completed_at: Option<std::time::Instant>,
    /// Full untruncated tool output. `None` when this row is not a
    /// `ToolResult`, or when the preview already is the full content.
    pub full_content: Option<String>,
    /// User-controlled expand state for tool output rows.
    pub expanded: bool,
}

pub fn display_tool_name(tool_name: &str) -> String {
    if let Some(rest) = tool_name.strip_prefix("mcp__") {
        if let Some((server, name)) = rest.split_once("__") {
            if !server.is_empty() && !name.is_empty() {
                return format!("[{}] {}", server, name);
            }
        }
    }
    if let Some((server, name)) = tool_name.split_once("__") {
        if !server.is_empty() && !name.is_empty() {
            return format!("[{}] {}", server, name);
        }
    }
    tool_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::display_tool_name;

    #[test]
    fn formats_mcp_tool_names_for_display() {
        assert_eq!(
            display_tool_name("mcp__filesystem__read_file"),
            "[filesystem] read_file"
        );
        assert_eq!(
            display_tool_name("github__create_issue"),
            "[github] create_issue"
        );
        assert_eq!(display_tool_name("Bash"), "Bash");
    }
}
