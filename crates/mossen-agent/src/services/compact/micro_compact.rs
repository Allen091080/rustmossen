//! Microcompact — in-place content clearing of old tool results to reduce context size
//! without a full compaction API call.

use std::collections::HashSet;
use std::sync::Mutex;
use tracing::debug;

use crate::token_estimation::rough_token_count_estimation;
use mossen_types::{ContentBlock, Message, Role, ToolResultContent};

use super::compact_warning_state::{clear_compact_warning_suppression, suppress_compact_warning};
use super::time_based_mc_config::{get_time_based_mc_config, TimeBasedMCConfig};

/// Sentinel message for cleared tool results.
pub const TIME_BASED_MC_CLEARED_MESSAGE: &str = "[Old tool result content cleared]";

const IMAGE_MAX_TOKEN_SIZE: usize = 2000;

/// Tool names that are eligible for microcompaction.
const COMPACTABLE_TOOLS: &[&str] = &[
    "Read",
    "Bash",
    "Execute",
    "Grep",
    "Glob",
    "WebSearch",
    "WebFetch",
    "Edit",
    "Write",
];

/// Result from microcompaction.
#[derive(Debug, Clone)]
pub struct MicrocompactResult {
    pub messages: Vec<Message>,
    pub compaction_info: Option<MicrocompactInfo>,
}

/// Diagnostic info about what was compacted.
#[derive(Debug, Clone)]
pub struct MicrocompactInfo {
    pub tokens_saved: usize,
    pub tool_results_cleared: usize,
    pub trigger: String,
}

/// Tracks registered tool caches and compact state.
#[derive(Debug, Default)]
struct MicrocompactState {
    registered_tools: HashSet<String>,
    tool_order: Vec<String>,
    deleted_refs: HashSet<String>,
    pinned_edits: HashSet<String>,
    tools_sent_to_api: bool,
}

static MC_STATE: Mutex<Option<MicrocompactState>> = Mutex::new(None);
static PENDING_CACHE_EDITS: Mutex<Option<Vec<CacheEdit>>> = Mutex::new(None);

/// A pending cache edit request.
#[derive(Debug, Clone)]
pub struct CacheEdit {
    pub tool_use_id: String,
    pub new_content: String,
}

/// Initialize the microcompact subsystem.
pub fn init_microcompact_state() {
    let mut state = MC_STATE.lock().unwrap();
    *state = Some(MicrocompactState::default());
}

/// Register a tool result as compactable.
pub fn register_tool_for_compact(tool_use_id: &str) {
    let mut state = MC_STATE.lock().unwrap();
    if let Some(s) = state.as_mut() {
        if s.registered_tools.insert(tool_use_id.to_string()) {
            s.tool_order.push(tool_use_id.to_string());
        }
    }
}

/// Mark tools as sent to the API.
pub fn mark_tools_sent() {
    let mut state = MC_STATE.lock().unwrap();
    if let Some(s) = state.as_mut() {
        s.tools_sent_to_api = true;
    }
}

/// Pin an edit (prevent compaction of that tool result).
pub fn pin_edit(tool_use_id: &str) {
    let mut state = MC_STATE.lock().unwrap();
    if let Some(s) = state.as_mut() {
        s.pinned_edits.insert(tool_use_id.to_string());
    }
}

/// Mark a tool result as deleted (already compacted).
pub fn mark_deleted(tool_use_id: &str) {
    let mut state = MC_STATE.lock().unwrap();
    if let Some(s) = state.as_mut() {
        s.deleted_refs.insert(tool_use_id.to_string());
    }
}

/// Reset microcompact state for a fresh cycle.
pub fn reset_microcompact_state() {
    let mut state = MC_STATE.lock().unwrap();
    if let Some(s) = state.as_mut() {
        s.registered_tools.clear();
        s.tool_order.clear();
        s.deleted_refs.clear();
        s.pinned_edits.clear();
        s.tools_sent_to_api = false;
    }
    let mut edits = PENDING_CACHE_EDITS.lock().unwrap();
    *edits = None;
}

/// Calculate tool result tokens from content text.
fn calculate_tool_result_tokens(content: &str) -> usize {
    rough_token_count_estimation(content, 4) as usize
}

/// Estimate token count for messages by extracting text content.
/// Pads estimate by 4/3 to be conservative.
pub fn estimate_message_tokens(messages: &[Message]) -> usize {
    let mut total_tokens = 0usize;

    for message in messages {
        if message.role != Role::User && message.role != Role::Assistant {
            continue;
        }

        for block in &message.content {
            match block {
                ContentBlock::Text(t) => {
                    total_tokens += rough_token_count_estimation(&t.text, 4) as usize;
                }
                ContentBlock::ToolResult(tr) => {
                    match &tr.content {
                        ToolResultContent::Text(text) => {
                            total_tokens += rough_token_count_estimation(text, 4) as usize;
                        }
                        ToolResultContent::Blocks(blocks) => {
                            for b in blocks {
                                if let ContentBlock::Text(t) = b {
                                    total_tokens += rough_token_count_estimation(&t.text, 4) as usize;
                                }
                            }
                        }
                    }
                }
                ContentBlock::Image(_) => {
                    total_tokens += IMAGE_MAX_TOKEN_SIZE;
                }
                ContentBlock::Thinking(t) => {
                    total_tokens += rough_token_count_estimation(&t.thinking, 4) as usize;
                }
                ContentBlock::ToolUse(tu) => {
                    let input_str = tu.input.to_string();
                    total_tokens += rough_token_count_estimation(
                        &format!("{}{}", tu.name, input_str),
                        4,
                    ) as usize;
                }
            }
        }
    }

    // Pad estimate by 4/3 to be conservative
    (total_tokens * 4 + 2) / 3
}

/// Walk messages and collect tool_use IDs whose tool name is in COMPACTABLE_TOOLS.
fn collect_compactable_tool_ids(messages: &[Message]) -> Vec<String> {
    let compactable: HashSet<&str> = COMPACTABLE_TOOLS.iter().copied().collect();
    let mut ids = Vec::new();

    for message in messages {
        if message.role == Role::Assistant {
            for block in &message.content {
                if let ContentBlock::ToolUse(tu) = block {
                    if compactable.contains(tu.name.as_str()) {
                        ids.push(tu.id.clone());
                    }
                }
            }
        }
    }

    ids
}

/// Prefix-match check for main thread source.
fn is_main_thread_source(query_source: Option<&str>) -> bool {
    match query_source {
        None => true,
        Some(s) => s.starts_with("repl_main_thread"),
    }
}

/// Evaluate whether the time-based trigger should fire.
pub fn evaluate_time_based_trigger(
    messages: &[Message],
    query_source: Option<&str>,
) -> Option<(f64, TimeBasedMCConfig)> {
    let config = get_time_based_mc_config();

    if !config.enabled {
        return None;
    }
    match query_source {
        None => return None,
        Some(s) if !s.starts_with("repl_main_thread") => return None,
        _ => {}
    }

    // Find last assistant message and check timestamp from extra metadata
    let last_assistant = messages.iter().rev().find(|m| m.role == Role::Assistant)?;

    let timestamp = last_assistant
        .extra
        .get("timestamp")
        .and_then(|v| v.as_f64())?;
    let now = chrono::Utc::now().timestamp_millis() as f64;
    let gap_minutes = (now - timestamp) / 60_000.0;

    if !gap_minutes.is_finite() || gap_minutes < config.gap_threshold_minutes as f64 {
        return None;
    }

    Some((gap_minutes, config))
}

/// Time-based microcompact: content-clear all but the most recent N compactable tool results.
fn maybe_time_based_microcompact(
    messages: &[Message],
    query_source: Option<&str>,
) -> Option<MicrocompactResult> {
    let (gap_minutes, config) = evaluate_time_based_trigger(messages, query_source)?;

    let compactable_ids = collect_compactable_tool_ids(messages);
    let keep_recent = std::cmp::max(1, config.keep_recent);
    let keep_start = compactable_ids.len().saturating_sub(keep_recent);
    let keep_set: HashSet<&str> = compactable_ids[keep_start..]
        .iter()
        .map(|s| s.as_str())
        .collect();
    let clear_set: HashSet<&str> = compactable_ids
        .iter()
        .filter(|id| !keep_set.contains(id.as_str()))
        .map(|s| s.as_str())
        .collect();

    if clear_set.is_empty() {
        return None;
    }

    let mut tokens_saved = 0usize;
    let result: Vec<Message> = messages
        .iter()
        .map(|message| {
            if message.role != Role::User {
                return message.clone();
            }

            let mut touched = false;
            let new_content: Vec<ContentBlock> = message
                .content
                .iter()
                .map(|block| {
                    if let ContentBlock::ToolResult(tr) = block {
                        if clear_set.contains(tr.tool_use_id.as_str()) {
                            if let ToolResultContent::Text(text) = &tr.content {
                                if text != TIME_BASED_MC_CLEARED_MESSAGE {
                                    tokens_saved += calculate_tool_result_tokens(text);
                                    touched = true;
                                    return ContentBlock::ToolResult(mossen_types::ToolResultBlock {
                                        tool_use_id: tr.tool_use_id.clone(),
                                        content: ToolResultContent::Text(
                                            TIME_BASED_MC_CLEARED_MESSAGE.to_string(),
                                        ),
                                        is_error: tr.is_error,
                                    });
                                }
                            }
                        }
                    }
                    block.clone()
                })
                .collect();

            if !touched {
                return message.clone();
            }
            let mut new_msg = message.clone();
            new_msg.content = new_content;
            new_msg
        })
        .collect();

    if tokens_saved == 0 {
        return None;
    }

    debug!(
        "[TIME-BASED MC] gap {:.0}min > {}min, cleared {} tool results (~{} tokens), kept last {}",
        gap_minutes,
        config.gap_threshold_minutes,
        clear_set.len(),
        tokens_saved,
        keep_set.len()
    );

    suppress_compact_warning();
    reset_microcompact_state();

    Some(MicrocompactResult {
        messages: result,
        compaction_info: Some(MicrocompactInfo {
            tokens_saved,
            tool_results_cleared: clear_set.len(),
            trigger: format!("time_based_{}min", gap_minutes.round() as i64),
        }),
    })
}

/// Main entry point for microcompacting messages before an API call.
pub async fn microcompact_messages(
    messages: &[Message],
    query_source: Option<&str>,
) -> MicrocompactResult {
    // Clear suppression flag at start of new microcompact attempt
    clear_compact_warning_suppression();

    // Time-based trigger runs first and short-circuits.
    if let Some(result) = maybe_time_based_microcompact(messages, query_source) {
        return result;
    }

    // Legacy microcompact path removed — for contexts where cached microcompact
    // is not available, no compaction happens here; autocompact handles context pressure.
    MicrocompactResult {
        messages: messages.to_vec(),
        compaction_info: None,
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/compact/microCompact.ts` additional exports.
// ---------------------------------------------------------------------------

use once_cell::sync::Lazy;

/// `microCompact.ts` `PinnedCacheEdits` mirror.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct PinnedCacheEdits {
    pub message_id: String,
    pub keep_ids: Vec<String>,
}

static PINNED_CACHE_EDITS_TS: Lazy<Mutex<Vec<PinnedCacheEdits>>> =
    Lazy::new(|| Mutex::new(Vec::new()));
static TOOLS_SENT_TO_API: Lazy<std::sync::atomic::AtomicBool> =
    Lazy::new(|| std::sync::atomic::AtomicBool::new(false));

/// `microCompact.ts` `consumePendingCacheEdits` (TS-mirror).
pub fn consume_pending_cache_edits_ts() -> Vec<PinnedCacheEdits> {
    std::mem::take(&mut *PINNED_CACHE_EDITS_TS.lock().unwrap())
}

/// `microCompact.ts` `getPinnedCacheEdits`.
pub fn get_pinned_cache_edits() -> Vec<PinnedCacheEdits> {
    PINNED_CACHE_EDITS_TS.lock().unwrap().clone()
}

/// `microCompact.ts` `pinCacheEdits`.
pub fn pin_cache_edits(record: PinnedCacheEdits) {
    PINNED_CACHE_EDITS_TS.lock().unwrap().push(record);
}

/// `microCompact.ts` `markToolsSentToAPIState`.
pub fn mark_tools_sent_to_api_state() {
    TOOLS_SENT_TO_API.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// TS `consumePendingCacheEdits` — drain the pending cache-edit queue and
/// return ownership of the entries to the caller.
pub fn consume_pending_cache_edits() -> Vec<CacheEdit> {
    let mut guard = PENDING_CACHE_EDITS.lock().unwrap();
    guard.take().unwrap_or_default()
}
