//! MCP content validation and truncation.
//!
//! Validates and truncates MCP tool output to stay within token limits,
//! including text and image content blocks.

use serde::{Deserialize, Serialize};

/// Factor for threshold-based pre-check before expensive token counting.
pub const MCP_TOKEN_COUNT_THRESHOLD_FACTOR: f64 = 0.5;
/// Estimated tokens per image.
pub const IMAGE_TOKEN_ESTIMATE: u64 = 1600;
/// Default maximum MCP output tokens.
const DEFAULT_MAX_MCP_OUTPUT_TOKENS: u64 = 25000;

/// Content block types for MCP results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
}

/// Image source data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    pub data: String,
    pub media_type: String,
    #[serde(rename = "type")]
    pub source_type: String,
}

/// MCP tool result — string, content blocks, or empty.
#[derive(Debug, Clone)]
pub enum McpToolResult {
    Text(String),
    Blocks(Vec<ContentBlock>),
    Empty,
}

/// Resolve the MCP output token cap. Precedence:
///   1. MAX_MCP_OUTPUT_TOKENS env var (explicit user override)
///   2. Feature flag override (via `flag_override` parameter)
///   3. Hardcoded default
pub fn get_max_mcp_output_tokens(flag_override: Option<u64>) -> u64 {
    if let Ok(env_val) = std::env::var("MAX_MCP_OUTPUT_TOKENS") {
        if let Ok(parsed) = env_val.parse::<u64>() {
            if parsed > 0 {
                return parsed;
            }
        }
    }

    if let Some(ov) = flag_override {
        if ov > 0 {
            return ov;
        }
    }

    DEFAULT_MAX_MCP_OUTPUT_TOKENS
}

/// Rough token count estimation (chars / 4).
fn rough_token_count_estimation(text: &str) -> u64 {
    (text.len() as u64 + 3) / 4
}

/// Get a content size estimate in tokens.
pub fn get_content_size_estimate(content: &McpToolResult) -> u64 {
    match content {
        McpToolResult::Empty => 0,
        McpToolResult::Text(text) => rough_token_count_estimation(text),
        McpToolResult::Blocks(blocks) => blocks.iter().fold(0u64, |total, block| match block {
            ContentBlock::Text { text } => total + rough_token_count_estimation(text),
            ContentBlock::Image { .. } => total + IMAGE_TOKEN_ESTIMATE,
        }),
    }
}

fn get_max_mcp_output_chars(max_tokens: u64) -> u64 {
    max_tokens * 4
}

fn get_truncation_message(max_tokens: u64) -> String {
    format!(
        "\n\n[OUTPUT TRUNCATED - exceeded {} token limit]\n\n\
        The tool output was truncated. If this MCP server provides pagination or filtering tools, \
        use them to retrieve specific portions of the data. If pagination is not available, inform \
        the user that you are working with truncated output and results may be incomplete.",
        max_tokens
    )
}

fn truncate_string(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        return content.to_string();
    }
    // Ensure we don't split a multi-byte char
    let safe_end = content
        .char_indices()
        .take_while(|(i, _)| *i < max_chars)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    content[..safe_end].to_string()
}

/// Truncate content blocks to fit within max_chars.
pub async fn truncate_content_blocks(
    blocks: &[ContentBlock],
    max_chars: u64,
    compress_image_fn: Option<&dyn AsyncCompressImage>,
) -> Vec<ContentBlock> {
    let mut result = Vec::new();
    let mut current_chars: u64 = 0;

    for block in blocks {
        match block {
            ContentBlock::Text { text } => {
                let remaining = max_chars.saturating_sub(current_chars);
                if remaining == 0 {
                    break;
                }
                if (text.len() as u64) <= remaining {
                    result.push(block.clone());
                    current_chars += text.len() as u64;
                } else {
                    let truncated = truncate_string(text, remaining as usize);
                    result.push(ContentBlock::Text { text: truncated });
                    break;
                }
            }
            ContentBlock::Image { source: _ } => {
                let image_chars = IMAGE_TOKEN_ESTIMATE * 4;
                if current_chars + image_chars <= max_chars {
                    result.push(block.clone());
                    current_chars += image_chars;
                } else {
                    let remaining = max_chars.saturating_sub(current_chars);
                    if remaining > 0 {
                        let remaining_bytes = (remaining as f64 * 0.75) as u64;
                        if let Some(compress_fn) = compress_image_fn {
                            if let Ok(compressed) =
                                compress_fn.compress(block, remaining_bytes).await
                            {
                                if let ContentBlock::Image { ref source } = compressed {
                                    current_chars += source.data.len() as u64;
                                } else {
                                    current_chars += image_chars;
                                }
                                result.push(compressed);
                            }
                            // If compression fails, skip the image
                        }
                    }
                }
            }
        }
    }

    result
}

/// Check if MCP content needs truncation.
pub async fn mcp_content_needs_truncation(
    content: &McpToolResult,
    max_tokens: u64,
    count_tokens_fn: Option<&dyn AsyncCountTokens>,
) -> bool {
    match content {
        McpToolResult::Empty => false,
        _ => {
            let size_estimate = get_content_size_estimate(content);
            if (size_estimate as f64) <= (max_tokens as f64 * MCP_TOKEN_COUNT_THRESHOLD_FACTOR) {
                return false;
            }

            // Try precise token counting if available
            if let Some(counter) = count_tokens_fn {
                match counter.count(content).await {
                    Ok(Some(count)) => return count > max_tokens,
                    Ok(None) => {}
                    Err(e) => {
                        tracing::error!("Token counting failed: {}", e);
                    }
                }
            }

            // Fallback: heuristic comparison
            size_estimate > max_tokens
        }
    }
}

/// Truncate MCP content.
pub async fn truncate_mcp_content(
    content: &McpToolResult,
    max_tokens: u64,
    compress_image_fn: Option<&dyn AsyncCompressImage>,
) -> McpToolResult {
    let max_chars = get_max_mcp_output_chars(max_tokens);
    let truncation_msg = get_truncation_message(max_tokens);

    match content {
        McpToolResult::Empty => McpToolResult::Empty,
        McpToolResult::Text(text) => {
            let truncated = truncate_string(text, max_chars as usize);
            McpToolResult::Text(truncated + &truncation_msg)
        }
        McpToolResult::Blocks(blocks) => {
            let mut truncated = truncate_content_blocks(blocks, max_chars, compress_image_fn).await;
            truncated.push(ContentBlock::Text {
                text: truncation_msg,
            });
            McpToolResult::Blocks(truncated)
        }
    }
}

/// Truncate MCP content only if needed.
pub async fn truncate_mcp_content_if_needed(
    content: &McpToolResult,
    max_tokens: u64,
    count_tokens_fn: Option<&dyn AsyncCountTokens>,
    compress_image_fn: Option<&dyn AsyncCompressImage>,
) -> McpToolResult {
    if !mcp_content_needs_truncation(content, max_tokens, count_tokens_fn).await {
        return content.clone();
    }
    truncate_mcp_content(content, max_tokens, compress_image_fn).await
}

/// Trait for async image compression.
#[async_trait::async_trait]
pub trait AsyncCompressImage: Send + Sync {
    async fn compress(&self, block: &ContentBlock, max_bytes: u64) -> anyhow::Result<ContentBlock>;
}

/// Trait for async token counting.
#[async_trait::async_trait]
pub trait AsyncCountTokens: Send + Sync {
    async fn count(&self, content: &McpToolResult) -> anyhow::Result<Option<u64>>;
}
