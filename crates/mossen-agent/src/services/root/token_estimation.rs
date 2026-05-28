//! Token estimation utilities

use serde_json::Value;

const TOKEN_COUNT_THINKING_BUDGET: u32 = 1024;
const TOKEN_COUNT_MAX_TOKENS: u32 = 2048;

/// Rough token count estimation based on byte length
pub fn rough_token_count_estimation(content: &str, bytes_per_token: f64) -> usize {
    (content.len() as f64 / bytes_per_token).round() as usize
}

/// Default rough token count (4 bytes per token)
pub fn rough_token_count(content: &str) -> usize {
    rough_token_count_estimation(content, 4.0)
}

/// Get bytes-per-token ratio for a file extension
pub fn bytes_per_token_for_file_type(file_extension: &str) -> f64 {
    match file_extension {
        "json" | "jsonl" | "jsonc" => 2.0,
        _ => 4.0,
    }
}

/// Rough estimation with file-type-aware ratio
pub fn rough_token_count_for_file_type(content: &str, file_extension: &str) -> usize {
    rough_token_count_estimation(content, bytes_per_token_for_file_type(file_extension))
}

/// Content block type for token estimation
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text(String),
    ToolUse { name: String, input: Value },
    ToolResult { content: Vec<ContentBlock> },
    Image,
    Document,
    Thinking(String),
    RedactedThinking(String),
    Other(Value),
}

/// Estimate tokens for a content block
pub fn rough_token_count_for_block(block: &ContentBlock) -> usize {
    match block {
        ContentBlock::Text(text) => rough_token_count(text),
        ContentBlock::ToolUse { name, input } => {
            let input_str = serde_json::to_string(input).unwrap_or_default();
            rough_token_count(&format!("{}{}", name, input_str))
        }
        ContentBlock::ToolResult { content } => {
            content.iter().map(|b| rough_token_count_for_block(b)).sum()
        }
        ContentBlock::Image | ContentBlock::Document => 2000,
        ContentBlock::Thinking(text) => rough_token_count(text),
        ContentBlock::RedactedThinking(data) => rough_token_count(data),
        ContentBlock::Other(val) => {
            let s = serde_json::to_string(val).unwrap_or_default();
            rough_token_count(&s)
        }
    }
}

/// Estimate tokens for a message content (string or blocks)
pub fn rough_token_count_for_content(content: &MessageContent) -> usize {
    match content {
        MessageContent::Text(s) => rough_token_count(s),
        MessageContent::Blocks(blocks) => {
            blocks.iter().map(|b| rough_token_count_for_block(b)).sum()
        }
    }
}

/// Message content can be either a string or blocks
#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Check if messages contain thinking blocks
pub fn has_thinking_blocks(messages: &[MessageForTokenCount]) -> bool {
    for msg in messages {
        if msg.role == "assistant" {
            if let MessageContent::Blocks(blocks) = &msg.content {
                for block in blocks {
                    if matches!(
                        block,
                        ContentBlock::Thinking(_) | ContentBlock::RedactedThinking(_)
                    ) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// A simplified message type for token counting
#[derive(Debug, Clone)]
pub struct MessageForTokenCount {
    pub role: String,
    pub content: MessageContent,
}

/// Estimate tokens for a list of messages
pub fn rough_token_count_for_messages(messages: &[MessageForTokenCount]) -> usize {
    messages
        .iter()
        .map(|m| rough_token_count_for_content(&m.content))
        .sum()
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/tokenEstimation.ts` additional exports.
// ---------------------------------------------------------------------------

/// `tokenEstimation.ts` `roughTokenCountEstimationForFileType`.
pub fn rough_token_count_estimation_for_file_type(content: &str, ext: &str) -> usize {
    let chars = content.chars().count();
    let divisor = match ext.to_lowercase().as_str() {
        "json" | "yaml" | "yml" | "toml" => 3,
        "md" | "txt" | "markdown" => 5,
        _ => 4,
    };
    (chars + divisor - 1) / divisor
}

/// `tokenEstimation.ts` `countTokensViaSmallFastFallback`.
pub fn count_tokens_via_small_fast_fallback(content: &str) -> usize {
    rough_token_count(content)
}

/// `tokenEstimation.ts` `roughTokenCountEstimationForMessage`.
pub fn rough_token_count_estimation_for_message(content: &str) -> usize {
    rough_token_count(content)
}
