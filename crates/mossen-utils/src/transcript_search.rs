//! Transcript search — flattens renderable messages to searchable text.
//!
//! WeakMap-equivalent caching is not available in Rust, so this module
//! provides stateless functions. Callers should cache results if needed.

use std::collections::HashSet;

/// Interrupt sentinel messages that are rendered as UI components, not text.
const INTERRUPT_MESSAGE: &str = "I need to interrupt you";
const INTERRUPT_MESSAGE_FOR_TOOL_USE: &str =
    "I need to interrupt this tool use to tell you something";
const SYSTEM_REMINDER_CLOSE: &str = "</system-reminder>";

fn rendered_as_sentinel() -> HashSet<&'static str> {
    let mut set = HashSet::new();
    set.insert(INTERRUPT_MESSAGE);
    set.insert(INTERRUPT_MESSAGE_FOR_TOOL_USE);
    set
}

/// Content block type for messages.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolResult {
        content: serde_json::Value,
    },
    ToolUse {
        name: String,
        input: serde_json::Value,
    },
    Thinking {
        thinking: String,
    },
    Image,
}

/// Attachment data.
#[derive(Debug, Clone)]
pub enum AttachmentData {
    RelevantMemories {
        memories: Vec<MemoryItem>,
    },
    QueuedCommand {
        prompt: PromptContent,
        command_mode: String,
        is_meta: bool,
    },
    Other,
}

/// Memory item for relevant_memories attachment.
#[derive(Debug, Clone)]
pub struct MemoryItem {
    pub content: String,
}

/// Prompt content — string or blocks.
#[derive(Debug, Clone)]
pub enum PromptContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Renderable message types for search.
#[derive(Debug, Clone)]
pub enum RenderableMessage {
    User {
        content: UserContent,
        tool_use_result: Option<serde_json::Value>,
    },
    Assistant {
        content: Vec<ContentBlock>,
    },
    Attachment {
        attachment: AttachmentData,
    },
    CollapsedReadSearch {
        relevant_memories: Option<Vec<MemoryItem>>,
    },
    System,
    GroupedToolUse,
}

/// User message content — string or blocks.
#[derive(Debug, Clone)]
pub enum UserContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Flatten a RenderableMessage to lowercased searchable text.
pub fn renderable_search_text(msg: &RenderableMessage) -> String {
    compute_search_text(msg).to_lowercase()
}

fn compute_search_text(msg: &RenderableMessage) -> String {
    let sentinels = rendered_as_sentinel();
    let raw = match msg {
        RenderableMessage::User {
            content,
            tool_use_result,
        } => match content {
            UserContent::Text(c) => {
                if sentinels.contains(c.as_str()) {
                    String::new()
                } else {
                    c.clone()
                }
            }
            UserContent::Blocks(blocks) => {
                let mut parts = Vec::new();
                for b in blocks {
                    match b {
                        ContentBlock::Text { text } if !sentinels.contains(text.as_str()) => {
                            parts.push(text.clone());
                        }
                        ContentBlock::ToolResult { .. } => {
                            if let Some(ref result) = tool_use_result {
                                parts.push(tool_result_search_text(result));
                            }
                        }
                        _ => {}
                    }
                }
                parts.join("\n")
            }
        },
        RenderableMessage::Assistant { content } => {
            let texts: Vec<String> = content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.clone()),
                    ContentBlock::ToolUse { input, .. } => Some(tool_use_search_text(input)),
                    _ => None,
                })
                .collect();
            texts.join("\n")
        }
        RenderableMessage::Attachment { attachment } => match attachment {
            AttachmentData::RelevantMemories { memories } => memories
                .iter()
                .map(|m| m.content.clone())
                .collect::<Vec<_>>()
                .join("\n"),
            AttachmentData::QueuedCommand {
                prompt,
                command_mode,
                is_meta,
            } => {
                if command_mode == "task-notification" || *is_meta {
                    String::new()
                } else {
                    match prompt {
                        PromptContent::Text(s) => s.clone(),
                        PromptContent::Blocks(blocks) => blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => Some(text.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    }
                }
            }
            AttachmentData::Other => String::new(),
        },
        RenderableMessage::CollapsedReadSearch { relevant_memories } => {
            if let Some(memories) = relevant_memories {
                memories
                    .iter()
                    .map(|m| m.content.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                String::new()
            }
        }
        RenderableMessage::System | RenderableMessage::GroupedToolUse => String::new(),
    };

    strip_system_reminders(&raw)
}

/// Strip <system-reminder>...</system-reminder> tags from text.
fn strip_system_reminders(text: &str) -> String {
    let mut t = text.to_string();
    loop {
        let open = match t.find("<system-reminder>") {
            Some(pos) => pos,
            None => break,
        };
        let close = match t[open..].find(SYSTEM_REMINDER_CLOSE) {
            Some(pos) => open + pos,
            None => break,
        };
        t = format!(
            "{}{}",
            &t[..open],
            &t[close + SYSTEM_REMINDER_CLOSE.len()..]
        );
    }
    t
}

/// Extract searchable text from tool use input fields.
///
/// Known fields: command, pattern, file_path, path, prompt, description,
/// query, url, skill, args, files.
pub fn tool_use_search_text(input: &serde_json::Value) -> String {
    let obj = match input.as_object() {
        Some(o) => o,
        None => return String::new(),
    };

    let mut parts = Vec::new();
    for key in &[
        "command",
        "pattern",
        "file_path",
        "path",
        "prompt",
        "description",
        "query",
        "url",
        "skill",
    ] {
        if let Some(serde_json::Value::String(v)) = obj.get(*key) {
            parts.push(v.clone());
        }
    }
    for key in &["args", "files"] {
        if let Some(serde_json::Value::Array(arr)) = obj.get(*key) {
            if arr.iter().all(|x| x.is_string()) {
                let joined: String = arr
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                parts.push(joined);
            }
        }
    }
    parts.join("\n")
}

/// Duck-type the tool's native Out for searchable text.
///
/// Known shapes: {stdout, stderr} (Bash/Shell), {content} (Grep),
/// {file: {content}} (Read), {filenames: []} (Glob), {output} (generic).
pub fn tool_result_search_text(r: &serde_json::Value) -> String {
    let obj = match r.as_object() {
        Some(o) => o,
        None => {
            return if let Some(s) = r.as_str() {
                s.to_string()
            } else {
                String::new()
            };
        }
    };

    // Known shapes first
    if let Some(serde_json::Value::String(stdout)) = obj.get("stdout") {
        let stderr = obj.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
        return if stderr.is_empty() {
            stdout.clone()
        } else {
            format!("{stdout}\n{stderr}")
        };
    }

    if let Some(file_obj) = obj.get("file").and_then(|v| v.as_object()) {
        if let Some(serde_json::Value::String(content)) = file_obj.get("content") {
            return content.clone();
        }
    }

    let mut parts = Vec::new();
    for key in &["content", "output", "result", "text", "message"] {
        if let Some(serde_json::Value::String(v)) = obj.get(*key) {
            parts.push(v.clone());
        }
    }
    for key in &["filenames", "lines", "results"] {
        if let Some(serde_json::Value::Array(arr)) = obj.get(*key) {
            if arr.iter().all(|x| x.is_string()) {
                let joined: String = arr
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                parts.push(joined);
            }
        }
    }
    parts.join("\n")
}
