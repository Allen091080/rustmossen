//! Extract memories service — background memory extraction from conversations.

pub mod prompts;

use std::time::Duration;
use tracing::debug;

/// Configuration for the memory extraction agent.
#[derive(Debug, Clone)]
pub struct ExtractMemoriesConfig {
    pub enabled: bool,
    pub min_new_messages: usize,
    pub max_turns: usize,
    pub timeout: Duration,
    pub skip_index: bool,
    pub team_memory_enabled: bool,
}

impl Default for ExtractMemoriesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_new_messages: 3,
            max_turns: 4,
            timeout: Duration::from_secs(60),
            skip_index: false,
            team_memory_enabled: false,
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
    let has_writes = messages[last_extraction_index..]
        .iter()
        .any(|msg| is_memory_write_message(msg));

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
                    let is_tool_use =
                        block.get("type").and_then(|t| t.as_str()) == Some("tool_use");
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
    let prompt = build_extraction_prompt_for_config(new_message_count, existing_memories, config);

    debug!(
        "extract-memories: starting extraction with {} new messages ({} total, prompt len {})",
        new_message_count,
        messages.len(),
        prompt.len()
    );

    // In production, this would fork the conversation and run a limited agent
    // with write access only to memory directories. The agent analyzes recent
    // messages and writes/updates memory files.
    Ok(Vec::new())
}

fn build_extraction_prompt_for_config(
    new_message_count: usize,
    existing_memories: &str,
    config: &ExtractMemoriesConfig,
) -> String {
    if config.team_memory_enabled {
        prompts::build_extract_combined_prompt(
            new_message_count,
            existing_memories,
            config.skip_index,
        )
    } else {
        prompts::build_extract_auto_only_prompt(
            new_message_count,
            existing_memories,
            config.skip_index,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extraction_prompt_uses_combined_memory_when_team_memory_enabled() {
        let config = ExtractMemoriesConfig {
            team_memory_enabled: true,
            ..ExtractMemoriesConfig::default()
        };

        let prompt = build_extraction_prompt_for_config(5, "- MEMORY.md", &config);

        assert!(prompt.contains("shared team memories"));
        assert!(prompt.contains("MUST avoid saving sensitive data"));
    }

    #[test]
    fn extraction_prompt_uses_auto_only_memory_by_default() {
        let prompt =
            build_extraction_prompt_for_config(5, "- MEMORY.md", &ExtractMemoriesConfig::default());

        assert!(!prompt.contains("shared team memories"));
        assert!(!prompt.contains("MUST avoid saving sensitive data"));
    }

    #[test]
    fn should_extract_memories_skips_after_memory_write() {
        let messages = vec![json!({
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "name": "Write",
                "input": {
                    "file_path": "/tmp/project/memory/team/MEMORY.md"
                }
            }]
        })];
        let config = ExtractMemoriesConfig {
            min_new_messages: 1,
            ..ExtractMemoriesConfig::default()
        };

        assert!(!should_extract_memories(&messages, 0, &config));
    }
}
