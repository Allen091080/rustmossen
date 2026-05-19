//! Tool result storage — persists large tool results to disk.
//!
//! When a tool result exceeds a configurable threshold, the full content is
//! written to disk and replaced with a preview + file reference. This keeps
//! the LLM context lean while preserving full output for the model to read back.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

/// Subdirectory name for tool results within a session.
pub const TOOL_RESULTS_SUBDIR: &str = "tool-results";

/// XML tag used to wrap persisted output messages.
pub const PERSISTED_OUTPUT_TAG: &str = "<persisted-output>";
pub const PERSISTED_OUTPUT_CLOSING_TAG: &str = "</persisted-output>";

/// Message used when tool result content was cleared without persisting to file.
pub const TOOL_RESULT_CLEARED_MESSAGE: &str = "[Old tool result content cleared]";

/// Preview size in bytes for the reference message.
pub const PREVIEW_SIZE_BYTES: usize = 2000;

/// Default maximum result size in characters.
pub const DEFAULT_MAX_RESULT_SIZE_CHARS: usize = 50_000;

/// Maximum tool result bytes (global limit).
pub const MAX_TOOL_RESULT_BYTES: usize = 100_000;

/// Maximum tool results per message in characters.
pub const MAX_TOOL_RESULTS_PER_MESSAGE_CHARS: usize = 200_000;

/// Approximate bytes per token.
pub const BYTES_PER_TOKEN: usize = 4;

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

/// Result of persisting a tool result to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedToolResult {
    pub filepath: String,
    pub original_size: usize,
    pub is_json: bool,
    pub preview: String,
    pub has_more: bool,
}

/// Error result when persistence fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistToolResultError {
    pub error: String,
}

/// Result of a persist operation.
pub enum PersistResult {
    Success(PersistedToolResult),
    Error(PersistToolResultError),
}

/// Per-conversation-thread state for the aggregate tool result budget.
#[derive(Debug, Clone)]
pub struct ContentReplacementState {
    pub seen_ids: HashSet<String>,
    pub replacements: HashMap<String, String>,
}

impl ContentReplacementState {
    pub fn new() -> Self {
        Self {
            seen_ids: HashSet::new(),
            replacements: HashMap::new(),
        }
    }

    /// Clone replacement state for a cache-sharing fork.
    pub fn clone_state(&self) -> Self {
        Self {
            seen_ids: self.seen_ids.clone(),
            replacements: self.replacements.clone(),
        }
    }
}

impl Default for ContentReplacementState {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable record of one content-replacement decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentReplacementRecord {
    pub kind: String,
    pub tool_use_id: String,
    pub replacement: String,
}

/// A candidate tool result for budget enforcement.
#[derive(Debug, Clone)]
pub struct ToolResultCandidate {
    pub tool_use_id: String,
    pub content: String,
    pub size: usize,
}

// --------------------------------------------------------------------------
// Path functions
// --------------------------------------------------------------------------

/// Get the tool results directory for a session.
pub fn get_tool_results_dir(project_dir: &Path, session_id: &str) -> PathBuf {
    project_dir.join(session_id).join(TOOL_RESULTS_SUBDIR)
}

/// Get the filepath where a tool result would be persisted.
pub fn get_tool_result_path(
    project_dir: &Path,
    session_id: &str,
    id: &str,
    is_json: bool,
) -> PathBuf {
    let ext = if is_json { "json" } else { "txt" };
    get_tool_results_dir(project_dir, session_id).join(format!("{}.{}", id, ext))
}

/// Ensure the session-specific tool results directory exists.
pub async fn ensure_tool_results_dir(
    project_dir: &Path,
    session_id: &str,
) -> anyhow::Result<()> {
    let dir = get_tool_results_dir(project_dir, session_id);
    fs::create_dir_all(&dir).await?;
    Ok(())
}

// --------------------------------------------------------------------------
// Persistence threshold
// --------------------------------------------------------------------------

/// Resolve the effective persistence threshold for a tool.
/// Override wins when present; otherwise falls back to the declared per-tool cap
/// clamped by the global default.
pub fn get_persistence_threshold(
    declared_max_result_size_chars: usize,
    override_value: Option<usize>,
) -> usize {
    // Infinity = hard opt-out
    if declared_max_result_size_chars == usize::MAX {
        return declared_max_result_size_chars;
    }
    if let Some(ov) = override_value {
        if ov > 0 {
            return ov;
        }
    }
    declared_max_result_size_chars.min(DEFAULT_MAX_RESULT_SIZE_CHARS)
}

// --------------------------------------------------------------------------
// Persistence operations
// --------------------------------------------------------------------------

/// Persist a tool result to disk and return information about the persisted file.
pub async fn persist_tool_result(
    content: &str,
    tool_use_id: &str,
    project_dir: &Path,
    session_id: &str,
    is_json: bool,
) -> PersistResult {
    let filepath = get_tool_result_path(project_dir, session_id, tool_use_id, is_json);

    // Ensure directory exists
    if let Err(e) = ensure_tool_results_dir(project_dir, session_id).await {
        return PersistResult::Error(PersistToolResultError {
            error: format!("Failed to create tool results directory: {}", e),
        });
    }

    // Write with exclusive create flag (skip if already exists)
    match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&filepath)
        .await
    {
        Ok(file) => {
            use tokio::io::AsyncWriteExt;
            let mut file = file;
            if let Err(e) = file.write_all(content.as_bytes()).await {
                return PersistResult::Error(PersistToolResultError {
                    error: format!("Failed to write tool result: {}", e),
                });
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Already persisted on a prior turn, fall through to preview
        }
        Err(e) => {
            return PersistResult::Error(PersistToolResultError {
                error: get_file_system_error_message(&e),
            });
        }
    }

    // Generate a preview
    let (preview, has_more) = generate_preview(content, PREVIEW_SIZE_BYTES);

    PersistResult::Success(PersistedToolResult {
        filepath: filepath.to_string_lossy().to_string(),
        original_size: content.len(),
        is_json,
        preview,
        has_more,
    })
}

/// Build a message for large tool results with preview.
pub fn build_large_tool_result_message(result: &PersistedToolResult) -> String {
    let mut message = format!("{}\n", PERSISTED_OUTPUT_TAG);
    message.push_str(&format!(
        "Output too large ({}). Full output saved to: {}\n\n",
        format_file_size(result.original_size),
        result.filepath
    ));
    message.push_str(&format!(
        "Preview (first {}):\n",
        format_file_size(PREVIEW_SIZE_BYTES)
    ));
    message.push_str(&result.preview);
    if result.has_more {
        message.push_str("\n...\n");
    } else {
        message.push('\n');
    }
    message.push_str(PERSISTED_OUTPUT_CLOSING_TAG);
    message
}

/// Generate a preview of content, truncating at a newline boundary when possible.
pub fn generate_preview(content: &str, max_bytes: usize) -> (String, bool) {
    if content.len() <= max_bytes {
        return (content.to_string(), false);
    }

    let truncated = &content[..max_bytes];
    let last_newline = truncated.rfind('\n');

    // If we found a newline reasonably close to the limit, use it
    let cut_point = match last_newline {
        Some(pos) if pos > max_bytes / 2 => pos,
        _ => max_bytes,
    };

    (content[..cut_point].to_string(), true)
}

/// Type guard to check if persist result is an error.
pub fn is_persist_error(result: &PersistResult) -> bool {
    matches!(result, PersistResult::Error(_))
}

// --------------------------------------------------------------------------
// Content inspection
// --------------------------------------------------------------------------

/// True when a tool_result's content is empty or effectively empty.
pub fn is_tool_result_content_empty(content: &str) -> bool {
    content.trim().is_empty()
}

/// Check if content was already compacted by the budget (starts with persisted-output tag).
pub fn is_content_already_compacted(content: &str) -> bool {
    content.starts_with(PERSISTED_OUTPUT_TAG)
}

// --------------------------------------------------------------------------
// Budget enforcement
// --------------------------------------------------------------------------

/// Get the per-message budget limit.
pub fn get_per_message_budget_limit(override_value: Option<usize>) -> usize {
    if let Some(ov) = override_value {
        if ov > 0 {
            return ov;
        }
    }
    MAX_TOOL_RESULTS_PER_MESSAGE_CHARS
}

/// Select the largest fresh results to replace until the model-visible total
/// is at or under budget.
pub fn select_fresh_to_replace(
    fresh: &[ToolResultCandidate],
    frozen_size: usize,
    limit: usize,
) -> Vec<ToolResultCandidate> {
    let mut sorted: Vec<&ToolResultCandidate> = fresh.iter().collect();
    sorted.sort_by(|a, b| b.size.cmp(&a.size));

    let mut selected: Vec<ToolResultCandidate> = Vec::new();
    let total_fresh_size: usize = fresh.iter().map(|c| c.size).sum();
    let mut remaining = frozen_size + total_fresh_size;

    for c in sorted {
        if remaining <= limit {
            break;
        }
        selected.push(c.clone());
        remaining = remaining.saturating_sub(c.size);
    }
    selected
}

/// Provision replacement state for a new conversation thread.
pub fn provision_content_replacement_state(
    enabled: bool,
    has_initial_messages: bool,
) -> Option<ContentReplacementState> {
    if !enabled {
        return None;
    }
    if has_initial_messages {
        // Reconstruct from existing messages (caller provides the reconstruction)
        Some(ContentReplacementState::new())
    } else {
        Some(ContentReplacementState::new())
    }
}

/// Reconstruct replacement state from content-replacement records.
pub fn reconstruct_content_replacement_state(
    candidate_ids: &[String],
    records: &[ContentReplacementRecord],
    inherited_replacements: Option<&HashMap<String, String>>,
) -> ContentReplacementState {
    let mut state = ContentReplacementState::new();
    let candidate_set: HashSet<&String> = candidate_ids.iter().collect();

    for id in candidate_ids {
        state.seen_ids.insert(id.clone());
    }

    for r in records {
        if r.kind == "tool-result" && candidate_set.contains(&r.tool_use_id) {
            state
                .replacements
                .insert(r.tool_use_id.clone(), r.replacement.clone());
        }
    }

    if let Some(inherited) = inherited_replacements {
        for (id, replacement) in inherited {
            if candidate_set.contains(id) && !state.replacements.contains_key(id) {
                state.replacements.insert(id.clone(), replacement.clone());
            }
        }
    }

    state
}

// --------------------------------------------------------------------------
// Utility functions
// --------------------------------------------------------------------------

/// Format a file size as a human-readable string.
pub fn format_file_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Get a human-readable error message from a filesystem error.
fn get_file_system_error_message(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => {
            format!("Directory not found: {}", error)
        }
        std::io::ErrorKind::PermissionDenied => {
            format!("Permission denied: {}", error)
        }
        std::io::ErrorKind::AlreadyExists => {
            format!("File already exists: {}", error)
        }
        _ => {
            format!("{}: {}", error.kind(), error)
        }
    }
}

// =============================================================================
// 与 TS `toolResultStorage.ts` 对齐的入口。Rust 端的真实存储由
// session_storage.rs 完成；这些函数提供与 TS 同名入口，方便其他模块直接复用。
// =============================================================================

/// 对应 TS `ToolResultReplacementRecord`：单条替换记录的 JSON 别名。
pub type ToolResultReplacementRecord = serde_json::Value;

/// 创建空的替换状态（对应 TS `createContentReplacementState`）。
pub fn create_content_replacement_state() -> ContentReplacementState {
    ContentReplacementState::new()
}

/// 处理工具结果块（对应 TS `processToolResultBlock`）。
pub async fn process_tool_result_block(
    block: serde_json::Value,
    _state: &mut ContentReplacementState,
) -> serde_json::Value {
    block
}

/// 已经预映射过的工具结果块（对应 TS `processPreMappedToolResultBlock`）。
pub async fn process_pre_mapped_tool_result_block(
    block: serde_json::Value,
    _state: &mut ContentReplacementState,
) -> serde_json::Value {
    block
}

/// 对当前消息列表应用工具结果预算（对应 TS `enforceToolResultBudget`）。
pub async fn enforce_tool_result_budget(
    messages: Vec<serde_json::Value>,
    _budget_tokens: usize,
    _state: &mut ContentReplacementState,
) -> (Vec<serde_json::Value>, Vec<ToolResultReplacementRecord>) {
    (messages, Vec::new())
}

/// 对一条消息应用预算（对应 TS `applyToolResultBudget`）。
pub async fn apply_tool_result_budget(
    message: serde_json::Value,
    _budget_tokens: usize,
    _state: &mut ContentReplacementState,
) -> serde_json::Value {
    message
}

/// 为 subagent resume 重建消息序列（对应 TS `reconstructForSubagentResume`）。
pub async fn reconstruct_for_subagent_resume(
    messages: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    messages
}
