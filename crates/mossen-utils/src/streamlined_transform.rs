//! Streamlined output transform for SDK messages.
//!
//! "Distillation-resistant" output format that keeps text intact,
//! summarizes tool calls with cumulative counts, omits thinking content.

use std::collections::HashSet;

/// Tool count categories.
#[derive(Debug, Clone, Default)]
pub struct ToolCounts {
    pub searches: u32,
    pub reads: u32,
    pub writes: u32,
    pub commands: u32,
    pub other: u32,
}

/// Tool category names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    Searches,
    Reads,
    Writes,
    Commands,
    Other,
}

/// Well-known search tool names.
const SEARCH_TOOLS: &[&str] = &["Grep", "Glob", "WebSearch", "LSP"];
/// Well-known read tool names.
const READ_TOOLS: &[&str] = &["Read", "ListMcpResources"];
/// Well-known write tool names.
const WRITE_TOOLS: &[&str] = &["Write", "Edit", "NotebookEdit"];
/// Well-known command tool names.
const COMMAND_TOOLS: &[&str] = &["Bash", "Shell", "Tmux", "TaskStop"];

/// Categorize a tool name into a count category.
pub fn categorize_tool_name(tool_name: &str) -> ToolCategory {
    if SEARCH_TOOLS.iter().any(|t| tool_name.starts_with(t)) {
        ToolCategory::Searches
    } else if READ_TOOLS.iter().any(|t| tool_name.starts_with(t)) {
        ToolCategory::Reads
    } else if WRITE_TOOLS.iter().any(|t| tool_name.starts_with(t)) {
        ToolCategory::Writes
    } else if COMMAND_TOOLS.iter().any(|t| tool_name.starts_with(t)) {
        ToolCategory::Commands
    } else {
        ToolCategory::Other
    }
}

impl ToolCounts {
    fn increment(&mut self, category: ToolCategory) {
        match category {
            ToolCategory::Searches => self.searches += 1,
            ToolCategory::Reads => self.reads += 1,
            ToolCategory::Writes => self.writes += 1,
            ToolCategory::Commands => self.commands += 1,
            ToolCategory::Other => self.other += 1,
        }
    }
}

/// Capitalize the first character of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let mut result = c.to_uppercase().to_string();
            result.push_str(chars.as_str());
            result
        }
    }
}

/// Generate a summary text for tool counts.
pub fn get_tool_summary_text(counts: &ToolCounts) -> Option<String> {
    let mut parts = Vec::new();

    if counts.searches > 0 {
        let word = if counts.searches == 1 {
            "pattern"
        } else {
            "patterns"
        };
        parts.push(format!("searched {} {}", counts.searches, word));
    }
    if counts.reads > 0 {
        let word = if counts.reads == 1 { "file" } else { "files" };
        parts.push(format!("read {} {}", counts.reads, word));
    }
    if counts.writes > 0 {
        let word = if counts.writes == 1 { "file" } else { "files" };
        parts.push(format!("wrote {} {}", counts.writes, word));
    }
    if counts.commands > 0 {
        let word = if counts.commands == 1 {
            "command"
        } else {
            "commands"
        };
        parts.push(format!("ran {} {}", counts.commands, word));
    }
    if counts.other > 0 {
        let word = if counts.other == 1 { "tool" } else { "tools" };
        parts.push(format!("{} other {}", counts.other, word));
    }

    if parts.is_empty() {
        None
    } else {
        Some(capitalize(&parts.join(", ")))
    }
}

/// Content block for assistant messages.
#[derive(Debug, Clone)]
pub enum AssistantContentBlock {
    Text { text: String },
    ToolUse { name: String },
    Thinking,
}

/// Message types for streamlined transform.
#[derive(Debug, Clone)]
pub enum StdoutMessage {
    Assistant {
        content: Vec<AssistantContentBlock>,
        session_id: String,
        uuid: String,
    },
    Result {
        data: serde_json::Value,
    },
    System,
    User,
    StreamEvent,
    ToolProgress,
    AuthStatus,
    RateLimitEvent,
    ControlResponse,
    ControlRequest,
    ControlCancelRequest,
    KeepAlive,
}

/// Streamlined output message types.
#[derive(Debug, Clone)]
pub enum StreamlinedMessage {
    Text {
        text: String,
        session_id: String,
        uuid: String,
    },
    ToolUseSummary {
        tool_summary: String,
        session_id: String,
        uuid: String,
    },
    Result {
        data: serde_json::Value,
    },
}

/// Count tool uses in content blocks and add to existing counts.
fn accumulate_tool_uses(content: &[AssistantContentBlock], counts: &mut ToolCounts) {
    for block in content {
        if let AssistantContentBlock::ToolUse { name } = block {
            let category = categorize_tool_name(name);
            counts.increment(category);
        }
    }
}

/// Extract text content from assistant content blocks.
fn extract_text_content(content: &[AssistantContentBlock]) -> String {
    content
        .iter()
        .filter_map(|b| match b {
            AssistantContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Create a stateful transformer that accumulates tool counts between text messages.
pub struct StreamlinedTransformer {
    cumulative_counts: ToolCounts,
}

impl StreamlinedTransformer {
    pub fn new() -> Self {
        Self {
            cumulative_counts: ToolCounts::default(),
        }
    }

    /// Transform a message. Returns None if the message should be filtered out.
    pub fn transform(&mut self, message: StdoutMessage) -> Option<StreamlinedMessage> {
        match message {
            StdoutMessage::Assistant {
                content,
                session_id,
                uuid,
            } => {
                let text = extract_text_content(&content);
                accumulate_tool_uses(&content, &mut self.cumulative_counts);

                if !text.is_empty() {
                    self.cumulative_counts = ToolCounts::default();
                    Some(StreamlinedMessage::Text {
                        text,
                        session_id,
                        uuid,
                    })
                } else {
                    let summary = get_tool_summary_text(&self.cumulative_counts)?;
                    Some(StreamlinedMessage::ToolUseSummary {
                        tool_summary: summary,
                        session_id,
                        uuid,
                    })
                }
            }
            StdoutMessage::Result { data } => Some(StreamlinedMessage::Result { data }),
            StdoutMessage::System
            | StdoutMessage::User
            | StdoutMessage::StreamEvent
            | StdoutMessage::ToolProgress
            | StdoutMessage::AuthStatus
            | StdoutMessage::RateLimitEvent
            | StdoutMessage::ControlResponse
            | StdoutMessage::ControlRequest
            | StdoutMessage::ControlCancelRequest
            | StdoutMessage::KeepAlive => None,
        }
    }
}

impl Default for StreamlinedTransformer {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a message should be included in streamlined output.
pub fn should_include_in_streamlined(message: &StdoutMessage) -> bool {
    matches!(message, StdoutMessage::Assistant { .. } | StdoutMessage::Result { .. })
}

/// 对应 TS `createStreamlinedTransformer`：构造 streamlined-output 转换器。
pub fn create_streamlined_transformer() -> impl Fn(StdoutMessage) -> Option<StdoutMessage> {
    |msg: StdoutMessage| {
        if should_include_in_streamlined(&msg) {
            Some(msg)
        } else {
            None
        }
    }
}
