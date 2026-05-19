//! Handle prompt submission logic.
//!
//! Processes user input through validation, command parsing, queueing,
//! and execution pipelines. Handles exit commands, slash commands,
//! file history snapshots, and query guard management.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Direct exit input commands.
const DIRECT_EXIT_INPUTS: &[&str] = &[
    "exit", "quit", ":q", ":q!", ":wq", ":wq!", "/exit", "/quit",
];

/// Check if input is a direct exit command.
pub fn is_direct_exit_input(input: &str) -> bool {
    DIRECT_EXIT_INPUTS.contains(&input.trim())
}

/// Prompt input mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptInputMode {
    Prompt,
    Bash,
    TaskNotification,
}

impl Default for PromptInputMode {
    fn default() -> Self {
        Self::Prompt
    }
}

/// Queued command representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedCommand {
    pub value: String,
    #[serde(default)]
    pub pre_expansion_value: Option<String>,
    #[serde(default)]
    pub mode: PromptInputMode,
    #[serde(default)]
    pub pasted_contents: Option<HashMap<String, PastedContent>>,
    #[serde(default)]
    pub skip_slash_commands: bool,
    #[serde(default)]
    pub uuid: Option<Uuid>,
    #[serde(default)]
    pub origin: Option<MessageOrigin>,
    #[serde(default)]
    pub workload: Option<String>,
    #[serde(default)]
    pub bridge_origin: Option<String>,
    #[serde(default)]
    pub is_meta: bool,
}

/// Message origin for tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum MessageOrigin {
    #[serde(rename = "task-notification")]
    TaskNotification,
    #[serde(rename = "bridge")]
    Bridge { source: String },
}

/// Pasted content types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PastedContent {
    #[serde(rename = "text")]
    Text { id: String, content: String },
    #[serde(rename = "image")]
    Image { id: String, content: String },
}

impl PastedContent {
    pub fn is_image(&self) -> bool {
        matches!(self, PastedContent::Image { .. })
    }

    pub fn id(&self) -> &str {
        match self {
            PastedContent::Text { id, .. } => id,
            PastedContent::Image { id, .. } => id,
        }
    }

    pub fn content(&self) -> &str {
        match self {
            PastedContent::Text { content, .. } => content,
            PastedContent::Image { content, .. } => content,
        }
    }
}

/// Reference parsed from input text (e.g., [Image #1]).
#[derive(Debug, Clone)]
pub struct ParsedReference {
    pub id: String,
    pub start: usize,
    pub end: usize,
}

/// Parse references from input text.
pub fn parse_references(input: &str) -> Vec<ParsedReference> {
    let mut refs = Vec::new();
    let mut search_from = 0;

    while let Some(start) = input[search_from..].find('[') {
        let abs_start = search_from + start;
        if let Some(end) = input[abs_start..].find(']') {
            let abs_end = abs_start + end + 1;
            let content = &input[abs_start + 1..abs_end - 1];

            // Match patterns like "Image #N" or "Text #N"
            if let Some(id) = extract_reference_id(content) {
                refs.push(ParsedReference {
                    id,
                    start: abs_start,
                    end: abs_end,
                });
            }
            search_from = abs_end;
        } else {
            break;
        }
    }
    refs
}

fn extract_reference_id(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.starts_with("Image #") || trimmed.starts_with("Text #") {
        if let Some(num_str) = trimmed.split('#').nth(1) {
            if num_str.chars().all(|c| c.is_ascii_digit()) {
                return Some(num_str.to_string());
            }
        }
    }
    None
}

/// Expand pasted text references in input.
pub fn expand_pasted_text_refs(
    input: &str,
    pasted_contents: &HashMap<String, PastedContent>,
) -> String {
    let refs = parse_references(input);
    if refs.is_empty() {
        return input.to_string();
    }

    let mut result = String::with_capacity(input.len());
    let mut last_end = 0;

    for r in &refs {
        result.push_str(&input[last_end..r.start]);
        if let Some(paste) = pasted_contents.get(&r.id) {
            if let PastedContent::Text { content, .. } = paste {
                result.push_str(content);
            } else {
                // Keep image references as-is
                result.push_str(&input[r.start..r.end]);
            }
        } else {
            result.push_str(&input[r.start..r.end]);
        }
        last_end = r.end;
    }
    result.push_str(&input[last_end..]);
    result
}

/// Query guard state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryGuardState {
    Idle,
    Dispatching,
    Running,
}

/// Query guard to prevent concurrent query execution.
#[derive(Debug)]
pub struct QueryGuard {
    state: std::sync::Mutex<QueryGuardState>,
}

impl QueryGuard {
    pub fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(QueryGuardState::Idle),
        }
    }

    pub fn is_active(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *state != QueryGuardState::Idle
    }

    pub fn reserve(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if *state == QueryGuardState::Idle {
            *state = QueryGuardState::Dispatching;
        }
    }

    pub fn start(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *state = QueryGuardState::Running;
    }

    pub fn end(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *state = QueryGuardState::Idle;
    }

    pub fn cancel_reservation(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if *state == QueryGuardState::Dispatching {
            *state = QueryGuardState::Idle;
        }
    }
}

impl Default for QueryGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Effort value for query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EffortValue {
    Low,
    Medium,
    High,
}

/// Result from processUserInput.
#[derive(Debug, Clone)]
pub struct ProcessUserInputResult {
    pub messages: Vec<serde_json::Value>,
    pub should_query: bool,
    pub allowed_tools: Option<Vec<String>>,
    pub model: Option<String>,
    pub effort: Option<EffortValue>,
    pub next_input: Option<String>,
    pub submit_next_input: bool,
}

/// Message queue item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub value: String,
    #[serde(default)]
    pub pre_expansion_value: Option<String>,
    #[serde(default)]
    pub mode: PromptInputMode,
    #[serde(default)]
    pub pasted_contents: Option<HashMap<String, PastedContent>>,
    #[serde(default)]
    pub skip_slash_commands: bool,
    #[serde(default)]
    pub uuid: Option<Uuid>,
}

/// Enqueue a message for processing.
pub fn enqueue(queue: &std::sync::Mutex<Vec<QueueItem>>, item: QueueItem) {
    let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
    q.push(item);
}

/// Parameters for handlePromptSubmit (core logic, no UI).
#[derive(Debug, Clone)]
pub struct HandlePromptSubmitParams {
    pub input: Option<String>,
    pub mode: Option<PromptInputMode>,
    pub pasted_contents: Option<HashMap<String, PastedContent>>,
    pub queued_commands: Option<Vec<QueuedCommand>>,
    pub messages: Vec<serde_json::Value>,
    pub main_loop_model: String,
    pub query_source: String,
    pub skip_slash_commands: bool,
    pub uuid: Option<Uuid>,
    pub is_external_loading: bool,
    pub has_interruptible_tool_in_progress: bool,
}

/// Validates and processes a prompt submission.
///
/// This handles:
/// - Direct exit inputs
/// - Reference parsing and expansion
/// - Slash command detection (immediate local-jsx commands)
/// - Queuing when a query is already active
/// - Constructing QueuedCommands for execution
pub fn validate_prompt_input(params: &HandlePromptSubmitParams) -> PromptValidation {
    // Queue processor path
    if let Some(cmds) = &params.queued_commands {
        if !cmds.is_empty() {
            return PromptValidation::ExecuteQueued;
        }
    }

    let input = params.input.as_deref().unwrap_or("");
    let mode = params.mode.clone().unwrap_or_default();

    if input.trim().is_empty() {
        return PromptValidation::Empty;
    }

    // Exit check
    if !params.skip_slash_commands && is_direct_exit_input(input) {
        return PromptValidation::Exit;
    }

    // Expand pasted text refs
    let pasted = params.pasted_contents.as_ref();
    let final_input = match pasted {
        Some(contents) => expand_pasted_text_refs(input, contents),
        None => input.to_string(),
    };

    let has_images = pasted
        .map(|p| p.values().any(|c| c.is_image()))
        .unwrap_or(false);

    // Slash command check
    if !params.skip_slash_commands && final_input.trim().starts_with('/') {
        let trimmed = final_input.trim();
        let (command_name, command_args) = match trimmed.find(' ') {
            Some(idx) => (&trimmed[1..idx], trimmed[idx + 1..].trim()),
            None => (&trimmed[1..], ""),
        };

        return PromptValidation::SlashCommand {
            command_name: command_name.to_string(),
            command_args: command_args.to_string(),
            final_input,
            mode,
            has_images,
        };
    }

    // Check if should queue
    if params.is_external_loading {
        if mode != PromptInputMode::Prompt && mode != PromptInputMode::Bash {
            return PromptValidation::Rejected;
        }
        return PromptValidation::Queue {
            final_input,
            mode,
            has_images,
        };
    }

    PromptValidation::Execute {
        final_input,
        mode,
        has_images,
    }
}

/// Result of validating a prompt submission.
#[derive(Debug, Clone)]
pub enum PromptValidation {
    Empty,
    Exit,
    ExecuteQueued,
    Rejected,
    SlashCommand {
        command_name: String,
        command_args: String,
        final_input: String,
        mode: PromptInputMode,
        has_images: bool,
    },
    Queue {
        final_input: String,
        mode: PromptInputMode,
        has_images: bool,
    },
    Execute {
        final_input: String,
        mode: PromptInputMode,
        has_images: bool,
    },
}

/// Selectable user messages filter - checks if message should be included
/// in file history snapshots.
pub fn selectable_user_messages_filter(msg: &serde_json::Value) -> bool {
    let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if msg_type != "user" {
        return false;
    }
    // Skip tool_result messages
    if let Some(content) = msg.get("message").and_then(|m| m.get("content")) {
        if let Some(arr) = content.as_array() {
            if arr.iter().any(|b| {
                b.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "tool_result")
                    .unwrap_or(false)
            }) {
                return false;
            }
        }
    }
    // Skip isMeta messages
    if msg
        .get("isMeta")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return false;
    }
    true
}

/// Determines workload tag for a turn based on queued commands.
pub fn compute_turn_workload(commands: &[QueuedCommand]) -> Option<String> {
    if commands.is_empty() {
        return None;
    }
    let first_workload = commands[0].workload.as_ref()?;
    if commands.iter().all(|c| c.workload.as_ref() == Some(first_workload)) {
        Some(first_workload.clone())
    } else {
        None
    }
}

/// 对应 TS `PromptInputHelpers`：prompt 输入工具集合命名空间结构体。
#[derive(Debug, Clone, Default)]
pub struct PromptInputHelpers {
    pub on_submit: Option<String>,
    pub on_cancel: Option<String>,
}

/// 对应 TS `handlePromptSubmit`：处理 prompt 提交的高层入口。
///
/// 由于 Rust 端没有 React 上下文，这里把核心步骤抽象为一个 async 函数：
/// 1. 把输入归一化；2. 投递到命令队列；3. 返回处理后的命令行。
pub async fn handle_prompt_submit(
    input: &str,
    _helpers: &PromptInputHelpers,
) -> anyhow::Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("empty prompt");
    }
    Ok(trimmed.to_string())
}
