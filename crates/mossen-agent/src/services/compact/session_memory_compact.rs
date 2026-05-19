//! Session memory compaction — uses session memory as a summary instead of an API call.

use std::collections::HashSet;
use std::sync::Mutex;
use tracing::debug;

use mossen_types::{ContentBlock, Message, Role};

use super::compact::{CompactionResult, CompactMetadata, PreservedSegment};
use super::micro_compact::estimate_message_tokens;
use super::prompt::get_compact_user_summary_message;

/// Configuration for session memory compaction thresholds.
#[derive(Debug, Clone)]
pub struct SessionMemoryCompactConfig {
    /// Minimum tokens to preserve after compaction.
    pub min_tokens: usize,
    /// Minimum number of messages with text blocks to keep.
    pub min_text_block_messages: usize,
    /// Maximum tokens to preserve after compaction (hard cap).
    pub max_tokens: usize,
}

/// Default configuration values.
pub const DEFAULT_SM_COMPACT_CONFIG: SessionMemoryCompactConfig = SessionMemoryCompactConfig {
    min_tokens: 10_000,
    min_text_block_messages: 5,
    max_tokens: 40_000,
};

static SM_COMPACT_CONFIG: Mutex<Option<SessionMemoryCompactConfig>> = Mutex::new(None);
static CONFIG_INITIALIZED: Mutex<bool> = Mutex::new(false);

/// Set the session memory compact configuration.
pub fn set_session_memory_compact_config(config: SessionMemoryCompactConfig) {
    let mut slot = SM_COMPACT_CONFIG.lock().unwrap();
    *slot = Some(config);
}

/// Get the current session memory compact configuration.
pub fn get_session_memory_compact_config() -> SessionMemoryCompactConfig {
    let slot = SM_COMPACT_CONFIG.lock().unwrap();
    slot.clone().unwrap_or(SessionMemoryCompactConfig {
        min_tokens: DEFAULT_SM_COMPACT_CONFIG.min_tokens,
        min_text_block_messages: DEFAULT_SM_COMPACT_CONFIG.min_text_block_messages,
        max_tokens: DEFAULT_SM_COMPACT_CONFIG.max_tokens,
    })
}

/// Reset config state (useful for testing).
pub fn reset_session_memory_compact_config() {
    let mut slot = SM_COMPACT_CONFIG.lock().unwrap();
    *slot = None;
    let mut init = CONFIG_INITIALIZED.lock().unwrap();
    *init = false;
}

/// Check if a message contains text blocks.
pub fn has_text_blocks(message: &Message) -> bool {
    if message.role == Role::Assistant || message.role == Role::User {
        return message.content.iter().any(|b| matches!(b, ContentBlock::Text(_)));
    }
    false
}

/// Check if a message contains tool_result blocks and return their tool_use_ids.
fn get_tool_result_ids(message: &Message) -> Vec<String> {
    if message.role != Role::User {
        return Vec::new();
    }
    message
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::ToolResult(tr) = b {
                Some(tr.tool_use_id.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Check if a message contains tool_use blocks with any of the given ids.
fn has_tool_use_with_ids(message: &Message, tool_use_ids: &HashSet<String>) -> bool {
    if message.role != Role::Assistant {
        return false;
    }
    message.content.iter().any(|b| {
        if let ContentBlock::ToolUse(tu) = b {
            tool_use_ids.contains(&tu.id)
        } else {
            false
        }
    })
}

/// Adjust the start index to ensure we don't split tool_use/tool_result pairs
/// or thinking blocks that share the same message uuid with kept assistant messages.
pub fn adjust_index_to_preserve_api_invariants(
    messages: &[Message],
    start_index: usize,
) -> usize {
    if start_index == 0 || start_index >= messages.len() {
        return start_index;
    }

    let mut adjusted_index = start_index;

    // Step 1: Handle tool_use/tool_result pairs
    let mut all_tool_result_ids: Vec<String> = Vec::new();
    for i in start_index..messages.len() {
        all_tool_result_ids.extend(get_tool_result_ids(&messages[i]));
    }

    if !all_tool_result_ids.is_empty() {
        // Collect tool_use IDs already in the kept range
        let mut tool_use_ids_in_kept: HashSet<String> = HashSet::new();
        for i in adjusted_index..messages.len() {
            if messages[i].role == Role::Assistant {
                for block in &messages[i].content {
                    if let ContentBlock::ToolUse(tu) = block {
                        tool_use_ids_in_kept.insert(tu.id.clone());
                    }
                }
            }
        }

        // Only look for tool_uses NOT already in the kept range
        let mut needed_ids: HashSet<String> = all_tool_result_ids
            .into_iter()
            .filter(|id| !tool_use_ids_in_kept.contains(id))
            .collect();

        // Find the assistant message(s) with matching tool_use blocks
        let mut i = adjusted_index;
        while i > 0 && !needed_ids.is_empty() {
            i -= 1;
            if has_tool_use_with_ids(&messages[i], &needed_ids) {
                adjusted_index = i;
                if messages[i].role == Role::Assistant {
                    for block in &messages[i].content {
                        if let ContentBlock::ToolUse(tu) = block {
                            needed_ids.remove(&tu.id);
                        }
                    }
                }
            }
        }
    }

    // Step 2: Handle thinking blocks that share message uuid
    let mut message_ids_in_kept: HashSet<String> = HashSet::new();
    for i in adjusted_index..messages.len() {
        if messages[i].role == Role::Assistant {
            if let Some(id) = &messages[i].uuid {
                message_ids_in_kept.insert(id.clone());
            }
        }
    }

    let mut i = adjusted_index;
    while i > 0 {
        i -= 1;
        if messages[i].role == Role::Assistant {
            if let Some(id) = &messages[i].uuid {
                if message_ids_in_kept.contains(id) {
                    adjusted_index = i;
                }
            }
        }
    }

    adjusted_index
}

/// Calculate the starting index for messages to keep after compaction.
pub fn calculate_messages_to_keep_index(
    messages: &[Message],
    last_summarized_index: i64,
) -> usize {
    if messages.is_empty() {
        return 0;
    }

    let config = get_session_memory_compact_config();

    // Start from the message after lastSummarizedIndex
    let mut start_index = if last_summarized_index >= 0 {
        (last_summarized_index as usize) + 1
    } else {
        messages.len()
    };

    // Calculate current tokens and text-block message count
    let mut total_tokens = 0usize;
    let mut text_block_message_count = 0usize;
    for i in start_index..messages.len() {
        total_tokens += estimate_message_tokens(&[messages[i].clone()]);
        if has_text_blocks(&messages[i]) {
            text_block_message_count += 1;
        }
    }

    // Check if we already hit the max cap
    if total_tokens >= config.max_tokens {
        return adjust_index_to_preserve_api_invariants(messages, start_index);
    }

    // Check if we already meet both minimums
    if total_tokens >= config.min_tokens && text_block_message_count >= config.min_text_block_messages {
        return adjust_index_to_preserve_api_invariants(messages, start_index);
    }

    // Find the floor (last compact boundary)
    let floor = messages
        .iter()
        .rposition(|m| is_compact_boundary_message(m))
        .map(|idx| idx + 1)
        .unwrap_or(0);

    // Expand backwards until we meet both minimums or hit max cap
    while start_index > floor {
        start_index -= 1;
        let msg_tokens = estimate_message_tokens(&[messages[start_index].clone()]);
        total_tokens += msg_tokens;
        if has_text_blocks(&messages[start_index]) {
            text_block_message_count += 1;
        }

        if total_tokens >= config.max_tokens {
            break;
        }
        if total_tokens >= config.min_tokens && text_block_message_count >= config.min_text_block_messages {
            break;
        }
    }

    adjust_index_to_preserve_api_invariants(messages, start_index)
}

/// Check if we should use session memory for compaction.
pub fn should_use_session_memory_compaction() -> bool {
    if std::env::var("MOSSEN_CODE_ENABLE_SM_COMPACT")
        .ok()
        .map_or(false, |v| v == "1" || v.to_lowercase() == "true")
    {
        return true;
    }
    if std::env::var("MOSSEN_CODE_DISABLE_SM_COMPACT")
        .ok()
        .map_or(false, |v| v == "1" || v.to_lowercase() == "true")
    {
        return false;
    }
    // In production, check feature flags
    false
}

/// Check if a message is a compact boundary message (system message with compact_metadata).
fn is_compact_boundary_message(message: &Message) -> bool {
    // System role messages with compact_metadata in extra
    message.extra.contains_key("compact_metadata")
}

/// Try to use session memory for compaction instead of traditional compaction.
/// Returns None if session memory compaction cannot be used.
pub async fn try_session_memory_compaction(
    messages: &[Message],
    _agent_id: Option<&str>,
    auto_compact_threshold: Option<usize>,
) -> Option<CompactionResult> {
    if !should_use_session_memory_compaction() {
        return None;
    }

    // In production, would read session memory content from disk
    // and build a CompactionResult. For now, return None as the
    // feature requires integration with the session memory system.
    None
}
