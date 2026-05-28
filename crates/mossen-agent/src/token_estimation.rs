//! # token_estimation — Token 计数估算
//!
//! 对应 TS `services/tokenEstimation.ts`，提供粗略 token 估算和
//! 文件类型感知估算。API 精确计数由 api_client 模块处理。

use mossen_types::{ContentBlock, Message};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// API 约束：思考模式下 token 计数的最小预算。
pub const TOKEN_COUNT_THINKING_BUDGET: u32 = 1024;
/// API 约束：思考模式下的最大 token 数。
pub const TOKEN_COUNT_MAX_TOKENS: u32 = 2048;

// ---------------------------------------------------------------------------
// 粗略估算
// ---------------------------------------------------------------------------

/// 粗略 token 计数估算。
///
/// 默认每 4 字符约 1 token。
/// 对应 TS `roughTokenCountEstimation()`。
pub fn rough_token_count_estimation(content: &str, bytes_per_token: u32) -> u64 {
    if content.is_empty() {
        return 0;
    }
    (content.len() as f64 / bytes_per_token as f64).round() as u64
}

/// 默认粗略估算（4 字符/token）。
pub fn rough_estimate(content: &str) -> u64 {
    rough_token_count_estimation(content, 4)
}

/// 根据文件类型获取 bytes-per-token 比率。
///
/// JSON 类文件含有大量单字符 token（`{}``:``"`），实际比率接近 2。
/// 对应 TS `bytesPerTokenForFileType()`。
pub fn bytes_per_token_for_file_type(extension: &str) -> u32 {
    match extension {
        "json" | "jsonl" | "jsonc" => 2,
        _ => 4,
    }
}

/// 文件类型感知的粗略 token 估算。
///
/// 对应 TS `roughTokenCountEstimationForFileType()`。
pub fn rough_estimate_for_file_type(content: &str, extension: &str) -> u64 {
    rough_token_count_estimation(content, bytes_per_token_for_file_type(extension))
}

// ---------------------------------------------------------------------------
// 消息级别估算
// ---------------------------------------------------------------------------

/// 估算单个内容块的 token 数。
///
/// 对应 TS `roughTokenCountEstimationForBlock()`。
pub fn estimate_block_tokens(block: &ContentBlock) -> u64 {
    match block {
        ContentBlock::Text(t) => rough_estimate(&t.text),
        ContentBlock::ToolUse(tu) => {
            let input_str = tu.input.to_string();
            rough_estimate(&format!("{}{}", tu.name, input_str))
        }
        ContentBlock::ToolResult(tr) => match &tr.content {
            mossen_types::ToolResultContent::Text(s) => rough_estimate(s),
            mossen_types::ToolResultContent::Blocks(blocks) => {
                blocks.iter().map(|b| estimate_block_tokens(b)).sum()
            }
        },
        ContentBlock::Thinking(t) => rough_estimate(&t.thinking),
        ContentBlock::Image(_) => {
            // 图像 / 文档按保守估计 2000 token
            // 对应 TS 中的 IMAGE_MAX_TOKEN_SIZE
            2000
        }
    }
}

/// 估算单条消息的 token 数。
pub fn estimate_message_tokens(message: &Message) -> u64 {
    message
        .content
        .iter()
        .map(|b| estimate_block_tokens(b))
        .sum()
}

/// 估算消息列表的 token 数。
///
/// 对应 TS `roughTokenCountEstimationForMessages()`。
pub fn estimate_messages_tokens(messages: &[Message]) -> u64 {
    messages.iter().map(|m| estimate_message_tokens(m)).sum()
}

// ---------------------------------------------------------------------------
// 思考块检测
// ---------------------------------------------------------------------------

/// 检查消息列表是否包含思考块。
///
/// 对应 TS `hasThinkingBlocks()`。
pub fn has_thinking_blocks(messages: &[Message]) -> bool {
    for msg in messages {
        if msg.role != mossen_types::Role::Assistant {
            continue;
        }
        for block in &msg.content {
            if matches!(block, ContentBlock::Thinking(_)) {
                return true;
            }
        }
    }
    false
}
