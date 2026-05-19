//! Memory extraction service

use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::prompts::build_extraction_prompt;

/// Extracted memory entry
#[derive(Debug, Clone)]
pub struct ExtractedMemory {
    pub content: String,
    pub source: String,
    pub timestamp: u64,
}

/// Run memory extraction on recent conversation
///
/// Analyzes recent messages and extracts memorable information.
/// Returns a list of new memories to be persisted.
pub async fn extract_memories(
    messages_summary: &str,
    existing_memories: &[String],
    memory_dir: &PathBuf,
    cancel_token: CancellationToken,
) -> Result<Vec<ExtractedMemory>, String> {
    let prompt = build_extraction_prompt(messages_summary, existing_memories);
    debug!("Running memory extraction (prompt len: {})", prompt.len());

    // In full implementation: run forked agent with extraction prompt
    // Parse output lines prefixed with "- " as new memories
    tokio::select! {
        _ = cancel_token.cancelled() => {
            return Err("Cancelled".to_string());
        }
        result = async {
            // Simulate extraction - in production this calls the model
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Ok(Vec::new())
        } => {
            return result;
        }
    }
}

/// Parse extraction output into memory entries
pub fn parse_extraction_output(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("- ") {
                Some(trimmed[2..].to_string())
            } else if trimmed.starts_with("* ") {
                Some(trimmed[2..].to_string())
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty() && s.len() > 10) // Skip trivially short entries
        .collect()
}

/// Save extracted memories to the memory directory
pub async fn save_memories(
    memories: &[ExtractedMemory],
    memory_dir: &PathBuf,
) -> Result<(), String> {
    if memories.is_empty() {
        return Ok(());
    }

    tokio::fs::create_dir_all(memory_dir)
        .await
        .map_err(|e| format!("Failed to create memory dir: {}", e))?;

    let memory_file = memory_dir.join("extracted_memories.md");
    let mut content = String::new();

    // Load existing content
    if let Ok(existing) = tokio::fs::read_to_string(&memory_file).await {
        content = existing;
    }

    // Append new memories
    for memory in memories {
        content.push_str(&format!("- {}\n", memory.content));
    }

    tokio::fs::write(&memory_file, &content)
        .await
        .map_err(|e| format!("Failed to write memories: {}", e))?;

    info!("Saved {} new memories", memories.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/extractMemories/extractMemories.ts` exports.
// ---------------------------------------------------------------------------

/// `extractMemories.ts` `createAutoMemCanUseTool`.
pub fn create_auto_mem_can_use_tool(memory_path: &str) -> String {
    format!("auto-mem:{}", memory_path)
}

/// `extractMemories.ts` `initExtractMemories`.
pub async fn init_extract_memories() {
    debug!("init_extract_memories");
}

/// `extractMemories.ts` `drainPendingExtraction`.
pub async fn drain_pending_extraction() -> usize {
    0
}
