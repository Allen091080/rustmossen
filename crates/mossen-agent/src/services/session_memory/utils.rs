//! Session memory utilities — helper functions for memory operations.

use std::path::{Path, PathBuf};

/// Get the default session memory configuration directory.
pub fn get_session_memory_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".mossen").join("memory")
}

/// Check if auto-memory extraction is enabled.
pub fn is_auto_memory_enabled() -> bool {
    if let Ok(val) = std::env::var("MOSSEN_CODE_DISABLE_AUTO_MEMORY") {
        if val == "1" || val.eq_ignore_ascii_case("true") {
            return false;
        }
    }
    true
}

/// Check if there have been memory writes since a given message index.
pub fn has_memory_writes_since(
    messages: &[serde_json::Value],
    since_index: usize,
) -> bool {
    messages[since_index..].iter().any(|msg| {
        msg.get("role")
            .and_then(|r| r.as_str())
            .map(|r| r == "assistant")
            .unwrap_or(false)
            && msg
                .get("content")
                .and_then(|c| c.as_array())
                .map(|blocks| {
                    blocks.iter().any(|block| {
                        block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                            && block
                                .get("name")
                                .and_then(|n| n.as_str())
                                .map(|n| n == "Write" || n == "Edit")
                                .unwrap_or(false)
                            && block
                                .get("input")
                                .and_then(|i| i.get("file_path"))
                                .and_then(|p| p.as_str())
                                .map(|p| p.contains("memory") || p.contains("MEMORY"))
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
    })
}

/// Count new messages since the last extraction.
pub fn count_new_messages_since(messages: &[serde_json::Value], last_index: usize) -> usize {
    if last_index >= messages.len() {
        return 0;
    }
    messages.len() - last_index
}

/// Format existing memory files as a manifest string.
pub fn format_existing_memories(memory_files: &[(String, String)]) -> String {
    if memory_files.is_empty() {
        return String::new();
    }
    memory_files
        .iter()
        .map(|(path, _content)| format!("- {}", path))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/SessionMemory/sessionMemoryUtils.ts` exports.
// ---------------------------------------------------------------------------

use std::sync::{Mutex, OnceLock};
use serde::{Deserialize, Serialize};

/// `sessionMemoryUtils.ts` `SessionMemoryConfig` — extended utility-layer
/// variant (mod.rs has the simpler shape under the same TS name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMemoryUtilsConfig {
    pub min_tokens_to_init: u64,
    pub min_tokens_to_update: u64,
    pub min_tool_calls_between_updates: u32,
    pub auto_extract_enabled: bool,
}

impl Default for SessionMemoryUtilsConfig {
    fn default() -> Self {
        Self {
            min_tokens_to_init: 8_000,
            min_tokens_to_update: 4_000,
            min_tool_calls_between_updates: 5,
            auto_extract_enabled: true,
        }
    }
}

/// `sessionMemoryUtils.ts` `DEFAULT_SESSION_MEMORY_CONFIG`.
pub fn default_session_memory_config() -> SessionMemoryUtilsConfig {
    SessionMemoryUtilsConfig::default()
}

#[derive(Debug, Default)]
struct SessionMemoryState {
    last_summarized_message_id: Option<String>,
    extraction_in_progress: bool,
    initialized: bool,
    config: SessionMemoryUtilsConfig,
    last_extraction_token_count: u64,
    tool_calls_since_last_update: u32,
}

fn state_cell() -> &'static Mutex<SessionMemoryState> {
    static S: OnceLock<Mutex<SessionMemoryState>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(SessionMemoryState::default()))
}

/// `sessionMemoryUtils.ts` `getLastSummarizedMessageId`.
pub fn get_last_summarized_message_id() -> Option<String> {
    state_cell()
        .lock()
        .unwrap()
        .last_summarized_message_id
        .clone()
}

/// `sessionMemoryUtils.ts` `setLastSummarizedMessageId`.
pub fn set_last_summarized_message_id(id: Option<String>) {
    state_cell().lock().unwrap().last_summarized_message_id = id;
}

/// `sessionMemoryUtils.ts` `markExtractionStarted`.
pub fn mark_extraction_started() {
    state_cell().lock().unwrap().extraction_in_progress = true;
}

/// `sessionMemoryUtils.ts` `markExtractionCompleted`.
pub fn mark_extraction_completed() {
    state_cell().lock().unwrap().extraction_in_progress = false;
}

/// `sessionMemoryUtils.ts` `waitForSessionMemoryExtraction`.
pub async fn wait_for_session_memory_extraction() {
    loop {
        let in_progress = state_cell().lock().unwrap().extraction_in_progress;
        if !in_progress {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// `sessionMemoryUtils.ts` `getSessionMemoryContent` — placeholder reader.
pub async fn get_session_memory_content() -> Option<String> {
    None
}

/// `sessionMemoryUtils.ts` `setSessionMemoryConfig`.
pub fn set_session_memory_config(cfg: SessionMemoryUtilsConfig) {
    state_cell().lock().unwrap().config = cfg;
}

/// `sessionMemoryUtils.ts` `getSessionMemoryConfig`.
pub fn get_session_memory_config() -> SessionMemoryUtilsConfig {
    state_cell().lock().unwrap().config.clone()
}

/// `sessionMemoryUtils.ts` `recordExtractionTokenCount`.
pub fn record_extraction_token_count(current_token_count: u64) {
    let mut s = state_cell().lock().unwrap();
    s.last_extraction_token_count = current_token_count;
    s.tool_calls_since_last_update = 0;
}

/// `sessionMemoryUtils.ts` `isSessionMemoryInitialized`.
pub fn is_session_memory_initialized() -> bool {
    state_cell().lock().unwrap().initialized
}

/// `sessionMemoryUtils.ts` `markSessionMemoryInitialized`.
pub fn mark_session_memory_initialized() {
    state_cell().lock().unwrap().initialized = true;
}

/// `sessionMemoryUtils.ts` `hasMetInitializationThreshold`.
pub fn has_met_initialization_threshold(current_token_count: u64) -> bool {
    let s = state_cell().lock().unwrap();
    !s.initialized && current_token_count >= s.config.min_tokens_to_init
}

/// `sessionMemoryUtils.ts` `hasMetUpdateThreshold`.
pub fn has_met_update_threshold(current_token_count: u64) -> bool {
    let s = state_cell().lock().unwrap();
    s.initialized
        && current_token_count.saturating_sub(s.last_extraction_token_count)
            >= s.config.min_tokens_to_update
}

/// `sessionMemoryUtils.ts` `getToolCallsBetweenUpdates`.
pub fn get_tool_calls_between_updates() -> u32 {
    state_cell().lock().unwrap().tool_calls_since_last_update
}

/// Increment the tool-call counter (auxiliary).
pub fn record_tool_call_for_session_memory() {
    state_cell().lock().unwrap().tool_calls_since_last_update += 1;
}

/// `sessionMemoryUtils.ts` `resetSessionMemoryState`.
pub fn reset_session_memory_state() {
    *state_cell().lock().unwrap() = SessionMemoryState::default();
}
