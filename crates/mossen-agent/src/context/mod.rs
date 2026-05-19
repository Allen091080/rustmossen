//! # context — 上下文管理与窗口裁剪
//!
//! 对应 TS `context.ts` + `context/` 目录，负责消息预处理管线、
//! 自动压缩、微压缩、上下文窗口计算等。

pub mod notifications;
pub mod stats;
pub mod overlay;
pub mod prompt_overlay;
pub mod voice_state;
pub mod queued_message;
pub mod modal;
pub mod mailbox;
pub mod fps_metrics;

use std::collections::HashMap;

use tracing::{debug, warn};

use crate::types::{AutoCompactTracking, MicrocompactStrategy, TurnEnvironment};
use mossen_types::{ContentBlock, Message, Role};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 自动压缩摘要最大输出 token。
const MAX_OUTPUT_TOKENS_FOR_SUMMARY: u32 = 20_000;
/// 自动压缩缓冲 token。
const AUTOCOMPACT_BUFFER_TOKENS: u32 = 13_000;
/// 警告阈值缓冲。
const WARNING_THRESHOLD_BUFFER: u32 = 20_000;
/// 错误阈值缓冲。
const ERROR_THRESHOLD_BUFFER: u32 = 20_000;
/// 手动压缩缓冲。
const MANUAL_COMPACT_BUFFER: u32 = 3_000;
/// 最大连续自动压缩失败次数。
const MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES: u32 = 3;
/// 微压缩时间间隔阈值（分钟）。
const MICROCOMPACT_TIME_GAP_MINUTES: f64 = 15.0;
/// 微压缩保留最近消息数。
const MICROCOMPACT_KEEP_RECENT: usize = 4;

// ---------------------------------------------------------------------------
// 上下文窗口计算
// ---------------------------------------------------------------------------

/// 获取有效上下文窗口大小。
pub fn effective_context_window(context_window: u64, max_output_tokens: u32) -> u64 {
    let reserved = (max_output_tokens).min(MAX_OUTPUT_TOKENS_FOR_SUMMARY) as u64;
    // 允许环境变量覆盖
    let window = if let Ok(override_val) = std::env::var("MOSSEN_CODE_AUTO_COMPACT_WINDOW") {
        if let Ok(v) = override_val.parse::<u64>() {
            context_window.min(v)
        } else {
            context_window
        }
    } else {
        context_window
    };
    window.saturating_sub(reserved)
}

/// 获取自动压缩触发阈值。
pub fn auto_compact_threshold(context_window: u64, max_output_tokens: u32) -> u64 {
    let effective = effective_context_window(context_window, max_output_tokens);
    effective.saturating_sub(AUTOCOMPACT_BUFFER_TOKENS as u64)
}

/// 获取警告阈值。
pub fn warning_threshold(context_window: u64, max_output_tokens: u32) -> u64 {
    let effective = effective_context_window(context_window, max_output_tokens);
    effective.saturating_sub(WARNING_THRESHOLD_BUFFER as u64)
}

/// 获取错误阈值。
pub fn error_threshold(context_window: u64, max_output_tokens: u32) -> u64 {
    let effective = effective_context_window(context_window, max_output_tokens);
    effective.saturating_sub(ERROR_THRESHOLD_BUFFER as u64)
}

// ---------------------------------------------------------------------------
// 消息预处理管线
// ---------------------------------------------------------------------------

/// 消息预处理结果。
#[derive(Debug)]
pub struct PreparedMessages {
    /// 处理后的消息列表。
    pub messages: Vec<Message>,
    /// 估算的 token 计数。
    pub estimated_tokens: u64,
    /// 是否执行了微压缩。
    pub micro_compacted: bool,
}

/// 消息预处理管线（同步版本）。
///
/// 执行顺序：snip → context collapse → 估算 token。
/// **不执行**微压缩——微压缩需要异步 API 摘要调用，请使用
/// [`prepare_messages_async`] 来获得完整管线（包含 microcompact）。
/// 同步版本仍然保留是因为很多调用方（测试、合成入口、非交互工具）
/// 不需要 microcompact 也不应付出异步代价。
pub fn prepare_messages(messages: &[Message], _env: &TurnEnvironment) -> PreparedMessages {
    let mut result = messages.to_vec();

    // 1. Snip: 截断过长的工具结果
    snip_long_tool_results(&mut result);

    // 2. 估算 token 计数（不含微压缩）
    let estimated_tokens = estimate_token_count(&result);

    PreparedMessages {
        messages: result,
        estimated_tokens,
        micro_compacted: false,
    }
}

/// 完整消息预处理管线（包含微压缩，需要 async 上下文）。
///
/// 执行顺序：snip → microcompact → 估算 token。
/// 对应 TS `prepareMessages()` + `microCompact.ts::microcompactMessages()`
/// 的组合调用。dialogue.rs 的主循环走这条路径，让长会话能在不主动
/// 触发自动压缩的前提下持续释放 tool_result 占用的 token。
pub async fn prepare_messages_async(
    messages: &[Message],
    _env: &TurnEnvironment,
    query_source: Option<&str>,
) -> PreparedMessages {
    let mut result = messages.to_vec();

    // 1. Snip: 截断过长的工具结果（同步 — 仅字符串操作）
    snip_long_tool_results(&mut result);

    // 2. Microcompact: 基于时间间隔触发；命中时把旧的 tool_result 内容
    //    替换为占位摘要，释放 token。失败/未触发时 messages 原样返回。
    let mc = crate::services::compact::micro_compact::microcompact_messages(
        &result,
        query_source,
    )
    .await;
    let micro_compacted = mc.compaction_info.is_some();
    result = mc.messages;
    if let Some(info) = mc.compaction_info {
        debug!(
            tokens_saved = info.tokens_saved,
            tool_results_cleared = info.tool_results_cleared,
            trigger = %info.trigger,
            "microcompact applied"
        );
    }

    // 3. 估算 token 计数（在微压缩之后，以便上层取到压缩后的真实值）
    let estimated_tokens = estimate_token_count(&result);

    PreparedMessages {
        messages: result,
        estimated_tokens,
        micro_compacted,
    }
}

/// 截断过长的工具结果。
fn snip_long_tool_results(messages: &mut [Message]) {
    const MAX_TOOL_RESULT_CHARS: usize = 30_000;

    for msg in messages.iter_mut() {
        if msg.role != Role::User {
            continue;
        }
        for block in msg.content.iter_mut() {
            if let ContentBlock::ToolResult(ref mut result) = block {
                match &mut result.content {
                    mossen_types::ToolResultContent::Text(ref mut text) => {
                        if text.len() > MAX_TOOL_RESULT_CHARS {
                            let truncated = &text[..MAX_TOOL_RESULT_CHARS];
                            *text = format!(
                                "{}...\n[truncated {} chars]",
                                truncated,
                                text.len() - MAX_TOOL_RESULT_CHARS
                            );
                        }
                    }
                    mossen_types::ToolResultContent::Blocks(ref mut blocks) => {
                        for block in blocks.iter_mut() {
                            if let ContentBlock::Text(ref mut text_block) = block {
                                if text_block.text.len() > MAX_TOOL_RESULT_CHARS {
                                    let original_len = text_block.text.len();
                                    text_block.text = format!(
                                        "{}...\n[truncated {} chars]",
                                        &text_block.text[..MAX_TOOL_RESULT_CHARS],
                                        original_len - MAX_TOOL_RESULT_CHARS
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 简单估算消息列表的 token 计数。
///
/// 粗略估算：每 4 个字符约 1 个 token。
pub fn estimate_token_count(messages: &[Message]) -> u64 {
    let total_chars: usize = messages
        .iter()
        .map(|msg| {
            msg.content
                .iter()
                .map(|block| match block {
                    ContentBlock::Text(t) => t.text.len(),
                    ContentBlock::ToolUse(t) => t.name.len() + t.input.to_string().len(),
                    ContentBlock::ToolResult(t) => match &t.content {
                        mossen_types::ToolResultContent::Text(s) => s.len(),
                        mossen_types::ToolResultContent::Blocks(blocks) => blocks
                            .iter()
                            .map(|b| match b {
                                ContentBlock::Text(t) => t.text.len(),
                                _ => 50,
                            })
                            .sum(),
                    },
                    ContentBlock::Thinking(t) => t.thinking.len(),
                    ContentBlock::Image(_) => 1000, // 图像按固定 token 估算
                })
                .sum::<usize>()
        })
        .sum();

    (total_chars as u64) / 4
}

// ---------------------------------------------------------------------------
// 自动压缩
// ---------------------------------------------------------------------------

/// 自动压缩结果。
#[derive(Debug)]
pub enum AutoCompactResult {
    /// 未触发压缩。
    NotNeeded,
    /// 已跳过（断路器生效）。
    Skipped,
    /// 压缩成功。
    Compacted {
        before_tokens: u64,
        after_tokens: u64,
        summary: String,
    },
    /// 压缩失败。
    Failed { error: String },
}

/// 检查并执行自动压缩。
pub async fn auto_compact_if_needed(
    messages: &[Message],
    estimated_tokens: u64,
    context_window: u64,
    max_output_tokens: u32,
    tracking: &mut Option<AutoCompactTracking>,
) -> AutoCompactResult {
    let threshold = auto_compact_threshold(context_window, max_output_tokens);

    // 未达到阈值
    if estimated_tokens < threshold {
        return AutoCompactResult::NotNeeded;
    }

    // 断路器检查
    if let Some(ref t) = tracking {
        if t.consecutive_failures >= MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES {
            warn!(
                consecutive_failures = t.consecutive_failures,
                "Auto-compact circuit breaker triggered"
            );
            return AutoCompactResult::Skipped;
        }
    }

    debug!(
        estimated_tokens,
        threshold, "Auto-compact threshold reached, compaction needed"
    );

    // 调用 services::compact::compact_conversation 执行实际压缩。
    // 该函数内部会通过 queryModelWithStreaming 调用 LLM 生成摘要。
    let result = crate::services::compact::compact::compact_conversation(messages, "Read").await;
    if !result.success {
        let error = result.error.unwrap_or_else(|| "compaction failed".into());
        if let Some(t) = tracking.as_mut() {
            t.consecutive_failures = t.consecutive_failures.saturating_add(1);
        }
        return AutoCompactResult::Failed { error };
    }

    if let Some(t) = tracking.as_mut() {
        t.consecutive_failures = 0;
    }

    let after_tokens = estimate_token_count(&result.new_messages);
    AutoCompactResult::Compacted {
        before_tokens: estimated_tokens,
        after_tokens,
        summary: format!(
            "Compacted {} message(s); kept {} (~{} tokens)",
            result.compacted_message_count,
            result.new_messages.len(),
            after_tokens
        ),
    }
}

// ---------------------------------------------------------------------------
// 微压缩
// ---------------------------------------------------------------------------

/// 确定微压缩策略。
///
/// 对应 TS `services/compact/microCompact.ts::evaluateTimeBasedTrigger`。
/// 当距离最后一条 assistant 消息的时间间隔超过阈值（默认 15 分钟）时，
/// 触发基于时间的微压缩。
pub fn determine_microcompact_strategy(
    messages: &[Message],
    turn_count: u32,
) -> MicrocompactStrategy {
    if messages.len() < 4 || turn_count < 2 {
        return MicrocompactStrategy::None;
    }

    // 基于时间的微压缩——检查最后一条 assistant 消息的时间间隔。
    let last_assistant_ts = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::Assistant)
        .and_then(|m| m.timestamp.as_deref())
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok());

    let Some(last_ts) = last_assistant_ts else {
        return MicrocompactStrategy::None;
    };

    let now = chrono::Utc::now();
    let gap_minutes = (now
        .signed_duration_since(last_ts.with_timezone(&chrono::Utc))
        .num_seconds() as f64)
        / 60.0;

    if !gap_minutes.is_finite() || gap_minutes < MICROCOMPACT_TIME_GAP_MINUTES {
        return MicrocompactStrategy::None;
    }

    MicrocompactStrategy::TimeBased {
        gap_minutes,
        keep_recent: MICROCOMPACT_KEEP_RECENT,
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 为孤立的 tool_use 块生成错误 tool_result。
///
/// 对应 TS `yieldMissingToolResultBlocks()`。
pub fn yield_missing_tool_result_blocks(messages: &[Message]) -> Vec<Message> {
    use std::collections::HashSet;

    // 收集所有 tool_use ID
    let mut tool_use_ids: Vec<String> = Vec::new();
    let mut answered_ids: HashSet<String> = HashSet::new();

    for msg in messages {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse(tu) => {
                    tool_use_ids.push(tu.id.clone());
                }
                ContentBlock::ToolResult(tr) => {
                    answered_ids.insert(tr.tool_use_id.clone());
                }
                _ => {}
            }
        }
    }

    // 找出没有 result 的 tool_use
    let missing: Vec<String> = tool_use_ids
        .into_iter()
        .filter(|id| !answered_ids.contains(id))
        .collect();

    if missing.is_empty() {
        return Vec::new();
    }

    // 为每个 missing 生成 tool_result
    let content: Vec<ContentBlock> = missing
        .into_iter()
        .map(|id| {
            ContentBlock::ToolResult(mossen_types::ToolResultBlock {
                tool_use_id: id,
                content: mossen_types::ToolResultContent::Text(
                    "Error: tool execution was interrupted".to_string(),
                ),
                is_error: Some(true),
            })
        })
        .collect();

    vec![Message {
        role: Role::User,
        content,
        uuid: Some(uuid::Uuid::new_v4().to_string()),
        is_meta: Some(true),
        origin: None,
        timestamp: Some(chrono::Utc::now().to_rfc3339()),
        extra: HashMap::new(),
    }]
}

/// 获取助手消息的可见文本。
///
/// 对应 TS `getAssistantVisibleText()`。
pub fn assistant_visible_text(message: &Message) -> String {
    message
        .content
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(t) = block {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

/// 检测是否应恢复 action promise。
///
/// 对应 TS `shouldRecoverActionPromise()`。
pub fn should_recover_action_promise(text: &str) -> bool {
    let patterns = [
        "I'll",
        "I will",
        "Let me",
        "I'm going to",
        "I need to",
        "I should",
    ];

    let action_verbs = [
        "create",
        "write",
        "update",
        "modify",
        "edit",
        "fix",
        "add",
        "remove",
        "delete",
        "implement",
        "refactor",
        "run",
        "execute",
        "install",
    ];

    let has_promise = patterns.iter().any(|p| text.contains(p));
    let has_action = action_verbs.iter().any(|v| text.to_lowercase().contains(v));

    has_promise && has_action
}

/// 检测是否为被扣留的 max_output_tokens 错误。
///
/// 对应 TS `isWithheldMaxOutputTokens()`。
pub fn is_withheld_max_output_tokens(message: &Message) -> bool {
    if message.role != Role::Assistant {
        return false;
    }
    message
        .extra
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .map(|s| s == "max_tokens")
        .unwrap_or(false)
}
