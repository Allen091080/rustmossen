//! Extract memories service — background memory extraction from conversations.

pub mod prompts;

use std::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for the memory extraction agent.
#[derive(Debug, Clone)]
pub struct ExtractMemoriesConfig {
    pub enabled: bool,
    pub min_new_messages: usize,
    pub max_turns: usize,
    pub timeout: Duration,
    pub skip_index: bool,
}

impl Default for ExtractMemoriesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_new_messages: 3,
            max_turns: 4,
            timeout: Duration::from_secs(60),
            skip_index: false,
        }
    }
}

/// Check if memory extraction should run for this turn.
pub fn should_extract_memories(
    messages: &[serde_json::Value],
    last_extraction_index: usize,
    config: &ExtractMemoriesConfig,
) -> bool {
    if !config.enabled {
        return false;
    }

    let new_messages = messages.len().saturating_sub(last_extraction_index);
    if new_messages < config.min_new_messages {
        return false;
    }

    // Check if there are already memory writes since last extraction
    // (main agent already handled it)
    let has_writes = messages[last_extraction_index..].iter().any(|msg| {
        is_memory_write_message(msg)
    });

    !has_writes
}

/// Check if a message contains a memory write tool use.
fn is_memory_write_message(msg: &serde_json::Value) -> bool {
    msg.get("role")
        .and_then(|r| r.as_str())
        .map(|r| r == "assistant")
        .unwrap_or(false)
        && msg
            .get("content")
            .and_then(|c| c.as_array())
            .map(|blocks| {
                blocks.iter().any(|block| {
                    let is_tool_use = block.get("type").and_then(|t| t.as_str()) == Some("tool_use");
                    let is_write = block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|n| n == "Write" || n == "Edit")
                        .unwrap_or(false);
                    let targets_memory = block
                        .get("input")
                        .and_then(|i| i.get("file_path"))
                        .and_then(|p| p.as_str())
                        .map(|p| p.contains("memory") || p.contains("MEMORY"))
                        .unwrap_or(false);
                    is_tool_use && is_write && targets_memory
                })
            })
            .unwrap_or(false)
}

/// Run the memory extraction agent as a forked conversation.
pub async fn run_extraction(
    messages: &[serde_json::Value],
    new_message_count: usize,
    existing_memories: &str,
    config: &ExtractMemoriesConfig,
) -> Result<Vec<serde_json::Value>, String> {
    let prompt = if config.skip_index {
        prompts::build_extract_auto_only_prompt(new_message_count, existing_memories, true)
    } else {
        prompts::build_extract_auto_only_prompt(new_message_count, existing_memories, false)
    };

    debug!("extract-memories: starting extraction with {} new messages", new_message_count);

    // In production, this would fork the conversation and run a limited agent
    // with write access only to memory directories. The agent analyzes recent
    // messages and writes/updates memory files.
    Ok(Vec::new())
}
