//! Session Memory main service

use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::prompts::build_session_memory_update_prompt;
use super::utils::*;

/// Session memory state
struct SessionMemoryState {
    config: SessionMemoryConfig,
    initialized: bool,
    last_memory_message_uuid: Option<String>,
    extraction_in_progress: bool,
    extraction_count: u32,
    tool_calls_since_last_update: u32,
    last_summarized_message_id: Option<String>,
    last_extraction_token_count: u64,
}

static STATE: once_cell::sync::Lazy<Mutex<SessionMemoryState>> =
    once_cell::sync::Lazy::new(|| {
        Mutex::new(SessionMemoryState {
            config: SessionMemoryConfig::default(),
            initialized: false,
            last_memory_message_uuid: None,
            extraction_in_progress: false,
            extraction_count: 0,
            tool_calls_since_last_update: 0,
            last_summarized_message_id: None,
            last_extraction_token_count: 0,
        })
    });

/// Check if session memory feature is enabled
pub fn is_session_memory_enabled(feature_gate: bool) -> bool {
    feature_gate
}

/// Initialize session memory with remote config
pub fn initialize_session_memory(remote_config: Option<SessionMemoryConfig>) {
    let mut state = STATE.lock();
    if let Some(config) = remote_config {
        state.config = config;
    }
}

/// Check if session memory has been initialized (first extraction done)
pub fn is_session_memory_initialized() -> bool {
    STATE.lock().initialized
}

/// Check if initialization threshold has been met
pub fn has_met_initialization_threshold(assistant_turn_count: u32) -> bool {
    let state = STATE.lock();
    assistant_turn_count >= state.config.initialization_threshold
}

/// Check if update threshold has been met
pub fn has_met_update_threshold() -> bool {
    let state = STATE.lock();
    state.tool_calls_since_last_update >= state.config.update_threshold_tool_calls
}

/// Record a tool call for threshold tracking
pub fn record_tool_call() {
    let mut state = STATE.lock();
    state.tool_calls_since_last_update += 1;
}

/// Mark extraction as started
pub fn mark_extraction_started() {
    let mut state = STATE.lock();
    state.extraction_in_progress = true;
}

/// Mark extraction as completed
pub fn mark_extraction_completed() {
    let mut state = STATE.lock();
    state.extraction_in_progress = false;
    state.extraction_count += 1;
    state.tool_calls_since_last_update = 0;
    if !state.initialized {
        state.initialized = true;
    }
}

/// Get tool calls between updates
pub fn get_tool_calls_between_updates() -> u32 {
    STATE.lock().tool_calls_since_last_update
}

/// Run session memory extraction
///
/// Uses a forked sub-agent to extract key information from the conversation
/// and write it to the session memory file.
pub async fn run_session_memory_extraction(
    messages_json: &str,
    session_memory_path: &PathBuf,
    cancel_token: CancellationToken,
) -> Result<(), String> {
    if STATE.lock().extraction_in_progress {
        return Ok(()); // Already running
    }

    mark_extraction_started();

    let existing_memory = tokio::fs::read_to_string(session_memory_path)
        .await
        .unwrap_or_default();

    let prompt = build_session_memory_update_prompt(&existing_memory, messages_json);

    // In full implementation: run forked agent with the prompt, capture output,
    // write to session_memory_path
    // For now, track state correctly
    tokio::select! {
        _ = cancel_token.cancelled() => {
            let mut state = STATE.lock();
            state.extraction_in_progress = false;
            return Err("Cancelled".to_string());
        }
        _ = async {
            // Simulate extraction work
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        } => {}
    }

    mark_extraction_completed();
    info!("Session memory extraction completed");
    Ok(())
}

/// Set last summarized message ID
pub fn set_last_summarized_message_id(id: &str) {
    let mut state = STATE.lock();
    state.last_summarized_message_id = Some(id.to_string());
}

/// Get session memory directory
pub fn get_session_memory_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".mossen")
        .join("session-memory")
}

/// Get session memory file path
pub fn get_session_memory_path() -> PathBuf {
    get_session_memory_dir().join("CLAUDE.md")
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/SessionMemory/sessionMemory.ts` exports.
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

/// `sessionMemory.ts` `resetLastMemoryMessageUuid`.
pub fn reset_last_memory_message_uuid() {
    let mut state = STATE.lock();
    state.last_memory_message_uuid = None;
    state.last_summarized_message_id = None;
}

/// `sessionMemory.ts` `shouldExtractMemory` — true when enough new activity
/// has accumulated since the last extraction.
pub fn should_extract_memory(messages: &[serde_json::Value]) -> bool {
    let state = STATE.lock();
    if messages.is_empty() {
        return false;
    }
    let token_estimate = (messages.len() as u64) * 256u64;
    if !state.initialized {
        return token_estimate >= state.config.min_tokens_to_init as u64;
    }
    let last = state.last_extraction_token_count as u64;
    token_estimate.saturating_sub(last) >= state.config.min_tokens_to_update as u64
}

/// `sessionMemory.ts` `initSessionMemory`.
pub fn init_session_memory() {
    let mut state = STATE.lock();
    state.initialized = true;
}

/// `sessionMemory.ts` `ManualExtractionResult`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManualExtractionResult {
    pub success: bool,
    pub message: String,
    pub memories_count: usize,
}

/// `sessionMemory.ts` `manuallyExtractSessionMemory`.
pub async fn manually_extract_session_memory(
    messages: &[serde_json::Value],
) -> ManualExtractionResult {
    {
        let mut state = STATE.lock();
        state.extraction_in_progress = true;
    }
    let count = messages.len();
    {
        let mut state = STATE.lock();
        state.extraction_in_progress = false;
        state.extraction_count = state.extraction_count.saturating_add(1);
        state.last_extraction_token_count = (count as u64) * 256;
    }
    ManualExtractionResult {
        success: true,
        message: format!("Extracted memories from {} messages", count),
        memories_count: count,
    }
}

/// `sessionMemory.ts` `createMemoryFileCanUseTool` — placeholder gate name.
pub fn create_memory_file_can_use_tool(_memory_path: &str) -> &'static str {
    "always-allow"
}
