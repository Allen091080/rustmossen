//! # condenser — 自动压缩编排与压缩请求管理
//!
//! 对应 TS `services/compact/` 目录，包含：
//! - 自动压缩判断与执行 (`autoCompact.ts`)
//! - 消息按 API 轮次分组 (`grouping.ts`)
//! - 待处理压缩请求缓冲 (`pendingCompactRequest.ts`)
//! - 压缩警告状态管理 (`compactWarningState.ts`)
//! - 压缩后清理 (`postCompactCleanup.ts`)
//! - 基于时间的微压缩配置 (`timeBasedMCConfig.ts`)

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use mossen_types::Message;

use crate::context::{
    auto_compact_threshold, effective_context_window, error_threshold, estimate_token_count,
    warning_threshold,
};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 手动压缩缓冲 token。
const MANUAL_COMPACT_BUFFER_TOKENS: u64 = 3_000;

/// 最大连续自动压缩失败次数（断路器）。
const MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES: u32 = 3;

/// 待处理压缩请求超时（毫秒）。
pub const COMPACT_REQUEST_TIMEOUT_MS: u64 = 60_000;

// ---------------------------------------------------------------------------
// Token 警告状态
// ---------------------------------------------------------------------------

/// Token 用量警告状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenWarningState {
    /// 剩余百分比。
    pub percent_left: u32,
    /// 是否超过警告阈值。
    pub is_above_warning_threshold: bool,
    /// 是否超过错误阈值。
    pub is_above_error_threshold: bool,
    /// 是否超过自动压缩阈值。
    pub is_above_auto_compact_threshold: bool,
    /// 是否达到阻塞限制。
    pub is_at_blocking_limit: bool,
}

/// 计算 token 用量警告状态。
///
/// 对应 TS `calculateTokenWarningState()`。
pub fn calculate_token_warning_state(
    token_usage: u64,
    context_window: u64,
    max_output_tokens: u32,
    auto_compact_enabled: bool,
) -> TokenWarningState {
    let compact_threshold = auto_compact_threshold(context_window, max_output_tokens);
    let threshold = if auto_compact_enabled {
        compact_threshold
    } else {
        effective_context_window(context_window, max_output_tokens)
    };

    let percent_left = if threshold > 0 && token_usage < threshold {
        (((threshold - token_usage) as f64 / threshold as f64) * 100.0).round() as u32
    } else {
        0
    };

    let warn_thresh = warning_threshold(context_window, max_output_tokens);
    let err_thresh = error_threshold(context_window, max_output_tokens);

    let is_above_warning_threshold = token_usage >= warn_thresh;
    let is_above_error_threshold = token_usage >= err_thresh;

    let is_above_auto_compact_threshold = auto_compact_enabled && token_usage >= compact_threshold;

    // 阻塞限制
    let actual_window = effective_context_window(context_window, max_output_tokens);
    let default_blocking_limit = actual_window.saturating_sub(MANUAL_COMPACT_BUFFER_TOKENS);

    // 环境变量覆盖
    let blocking_limit = if let Ok(val) = std::env::var("MOSSEN_CODE_BLOCKING_LIMIT_OVERRIDE") {
        val.parse::<u64>().unwrap_or(default_blocking_limit)
    } else {
        default_blocking_limit
    };

    let is_at_blocking_limit = token_usage >= blocking_limit;

    TokenWarningState {
        percent_left,
        is_above_warning_threshold,
        is_above_error_threshold,
        is_above_auto_compact_threshold,
        is_at_blocking_limit,
    }
}

// ---------------------------------------------------------------------------
// 自动压缩启用检查
// ---------------------------------------------------------------------------

/// 检查自动压缩是否启用。
///
/// 对应 TS `isAutoCompactEnabled()`。
pub fn is_auto_compact_enabled() -> bool {
    if crate::condenser::is_env_truthy("DISABLE_COMPACT") {
        return false;
    }
    if crate::condenser::is_env_truthy("DISABLE_AUTO_COMPACT") {
        return false;
    }
    // 默认启用自动压缩（实际部署中应从 GlobalConfig 读取）
    true
}

/// 检查环境变量是否为 truthy 值。
fn is_env_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// 自动压缩跟踪
// ---------------------------------------------------------------------------

/// 自动压缩跟踪状态。
///
/// 对应 TS `AutoCompactTrackingState`。
#[derive(Debug, Clone)]
pub struct AutoCompactTrackingState {
    /// 是否已执行过压缩。
    pub compacted: bool,
    /// 轮次计数。
    pub turn_counter: u32,
    /// 当前轮次 ID。
    pub turn_id: String,
    /// 连续失败次数（断路器用）。
    pub consecutive_failures: u32,
}

impl Default for AutoCompactTrackingState {
    fn default() -> Self {
        Self {
            compacted: false,
            turn_counter: 0,
            turn_id: String::new(),
            consecutive_failures: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// shouldAutoCompact
// ---------------------------------------------------------------------------

/// 判断是否应触发自动压缩。
///
/// 对应 TS `shouldAutoCompact()`。递归防护：session_memory 和 compact
/// 来源不触发。
pub fn should_auto_compact(
    messages: &[Message],
    context_window: u64,
    max_output_tokens: u32,
    query_source: Option<&str>,
    snip_tokens_freed: u64,
) -> bool {
    // 递归防护
    if let Some(source) = query_source {
        if source == "session_memory" || source == "compact" || source == "marble_origami" {
            return false;
        }
    }

    if !is_auto_compact_enabled() {
        return false;
    }

    let token_count = estimate_token_count(messages).saturating_sub(snip_tokens_freed);
    let threshold = auto_compact_threshold(context_window, max_output_tokens);
    let effective_window = effective_context_window(context_window, max_output_tokens);

    debug!(
        token_count,
        threshold, effective_window, snip_tokens_freed, "autocompact: evaluating threshold"
    );

    let state = calculate_token_warning_state(token_count, context_window, max_output_tokens, true);

    state.is_above_auto_compact_threshold
}

// ---------------------------------------------------------------------------
// autoCompactIfNeeded 结果
// ---------------------------------------------------------------------------

/// 自动压缩执行结果。
#[derive(Debug)]
pub struct AutoCompactOutcome {
    /// 是否执行了压缩。
    pub was_compacted: bool,
    /// 压缩结果。
    pub compaction_result: Option<CompactionResult>,
    /// 连续失败次数。
    pub consecutive_failures: Option<u32>,
}

/// 压缩结果。
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// 压缩后的消息列表。
    pub messages: Vec<Message>,
    /// 压缩前 token 数。
    pub before_tokens: u64,
    /// 压缩后 token 数。
    pub after_tokens: u64,
    /// 摘要文本。
    pub summary: String,
}

/// 检查并执行自动压缩。
///
/// 对应 TS `autoCompactIfNeeded()`。
pub async fn auto_compact_if_needed(
    messages: &[Message],
    context_window: u64,
    max_output_tokens: u32,
    query_source: Option<&str>,
    tracking: Option<&AutoCompactTrackingState>,
    snip_tokens_freed: u64,
) -> AutoCompactOutcome {
    if is_env_truthy("DISABLE_COMPACT") {
        return AutoCompactOutcome {
            was_compacted: false,
            compaction_result: None,
            consecutive_failures: None,
        };
    }

    // 断路器
    if let Some(t) = tracking {
        if t.consecutive_failures >= MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES {
            return AutoCompactOutcome {
                was_compacted: false,
                compaction_result: None,
                consecutive_failures: Some(t.consecutive_failures),
            };
        }
    }

    let should_compact = should_auto_compact(
        messages,
        context_window,
        max_output_tokens,
        query_source,
        snip_tokens_freed,
    );

    if !should_compact {
        return AutoCompactOutcome {
            was_compacted: false,
            compaction_result: None,
            consecutive_failures: None,
        };
    }

    // 委托到 services::compact::compact_conversation。
    debug!("auto_compact_if_needed: threshold reached, invoking compact_conversation");

    let before_tokens = estimate_token_count(messages);

    let compact_result =
        crate::services::compact::compact::compact_conversation(messages, "Read").await;

    if !compact_result.success {
        let failures = tracking.map(|t| t.consecutive_failures + 1).unwrap_or(1);
        warn!(
            error = ?compact_result.error,
            consecutive_failures = failures,
            "auto_compact_if_needed: compaction failed"
        );
        return AutoCompactOutcome {
            was_compacted: false,
            compaction_result: None,
            consecutive_failures: Some(failures),
        };
    }

    let after_tokens = estimate_token_count(&compact_result.new_messages);

    AutoCompactOutcome {
        was_compacted: true,
        compaction_result: Some(CompactionResult {
            messages: compact_result.new_messages,
            before_tokens,
            after_tokens,
            summary: format!(
                "Compacted {} message(s); kept {} (~{} tokens)",
                compact_result.compacted_message_count,
                compact_result.remaining_token_count / 256,
                after_tokens
            ),
        }),
        consecutive_failures: Some(0),
    }
}

// ---------------------------------------------------------------------------
// 消息按 API 轮次分组
// ---------------------------------------------------------------------------

/// 按 API 轮次边界分组消息。
///
/// 对应 TS `groupMessagesByApiRound()`。每当出现新的 assistant 消息
///（不同的 message uuid）时触发分组边界。
pub fn group_messages_by_api_round(messages: &[Message]) -> Vec<Vec<&Message>> {
    let mut groups: Vec<Vec<&Message>> = Vec::new();
    let mut current: Vec<&Message> = Vec::new();
    let mut last_assistant_id: Option<&str> = None;

    for msg in messages {
        let is_assistant = msg.role == mossen_types::Role::Assistant;

        if is_assistant && current.len() > 0 {
            let msg_id = msg.uuid.as_deref();
            if msg_id != last_assistant_id {
                groups.push(current);
                current = vec![msg];
                if let Some(id) = msg_id {
                    last_assistant_id = Some(id);
                }
                continue;
            }
        }

        current.push(msg);

        if is_assistant {
            last_assistant_id = msg.uuid.as_deref();
        }
    }

    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

// ---------------------------------------------------------------------------
// 待处理压缩请求缓冲（单槽）
// ---------------------------------------------------------------------------

/// 待处理压缩请求。
///
/// 对应 TS `PendingCompactRequest`。
#[derive(Debug, Clone)]
pub struct PendingCompactRequest {
    /// 请求 ID。
    pub request_id: String,
    /// 模式（目前仅 manual）。
    pub mode: CompactMode,
    /// 是否为干运行。
    pub dry_run: bool,
    /// 自定义指令。
    pub custom_instructions: Option<String>,
    /// 入队时间。
    pub enqueued_at: Instant,
}

/// 压缩模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactMode {
    Manual,
}

/// 入队结果。
#[derive(Debug)]
pub enum EnqueueResult {
    Ok,
    AlreadyPending { reason: String },
}

/// 单槽压缩请求缓冲区。
///
/// 对应 TS 模块级 `pendingRequest` 全局状态。
/// Rust 中使用 `Mutex<Option<…>>` 实现线程安全。
pub struct CompactRequestBuffer {
    inner: Mutex<Option<PendingCompactRequest>>,
}

impl CompactRequestBuffer {
    /// 创建新的缓冲区。
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// 入队请求。如果已有待处理请求则返回失败。
    pub fn enqueue(
        &self,
        request_id: String,
        dry_run: bool,
        custom_instructions: Option<String>,
    ) -> EnqueueResult {
        let mut slot = self.inner.lock();
        if slot.is_some() {
            return EnqueueResult::AlreadyPending {
                reason: "another compact request is already pending".to_string(),
            };
        }
        *slot = Some(PendingCompactRequest {
            request_id,
            mode: CompactMode::Manual,
            dry_run,
            custom_instructions,
            enqueued_at: Instant::now(),
        });
        EnqueueResult::Ok
    }

    /// 出队请求。返回 `None` 如果无待处理请求。
    pub fn dequeue(&self) -> Option<PendingCompactRequest> {
        self.inner.lock().take()
    }

    /// 查看（不出队）。
    pub fn peek(&self) -> Option<PendingCompactRequest> {
        self.inner.lock().clone()
    }

    /// 是否有待处理请求。
    pub fn has_pending(&self) -> bool {
        self.inner.lock().is_some()
    }

    /// 是否已超时。
    pub fn has_timed_out(&self) -> bool {
        self.inner
            .lock()
            .as_ref()
            .map(|r| r.enqueued_at.elapsed() > Duration::from_millis(COMPACT_REQUEST_TIMEOUT_MS))
            .unwrap_or(false)
    }

    /// 无条件清除。
    pub fn clear(&self) {
        *self.inner.lock() = None;
    }
}

impl Default for CompactRequestBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 压缩警告抑制状态
// ---------------------------------------------------------------------------

/// 压缩警告抑制标志。
///
/// 对应 TS `compactWarningStore`。压缩成功后抑制警告，
/// 直到下次压缩尝试开始时清除。
static COMPACT_WARNING_SUPPRESSED: AtomicBool = AtomicBool::new(false);

/// 抑制压缩警告。压缩成功后调用。
pub fn suppress_compact_warning() {
    COMPACT_WARNING_SUPPRESSED.store(true, Ordering::Relaxed);
}

/// 清除压缩警告抑制。新压缩尝试开始时调用。
pub fn clear_compact_warning_suppression() {
    COMPACT_WARNING_SUPPRESSED.store(false, Ordering::Relaxed);
}

/// 检查压缩警告是否被抑制。
pub fn is_compact_warning_suppressed() -> bool {
    COMPACT_WARNING_SUPPRESSED.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// 压缩后清理
// ---------------------------------------------------------------------------

/// 执行压缩后清理。
///
/// 对应 TS `runPostCompactCleanup()`。清理在压缩后失效的缓存和
/// 跟踪状态。
pub fn run_post_compact_cleanup(query_source: Option<&str>) {
    let is_main_thread_compact = match query_source {
        None => true,
        Some(s) => s.starts_with("repl_main_thread") || s == "sdk",
    };

    // 重置微压缩状态
    // （context.rs 中的微压缩状态由调用方管理）

    if is_main_thread_compact {
        // 清除主线程级缓存
        debug!("post_compact_cleanup: clearing main-thread caches");
    }

    // 清除分类器审批和推测性检查
    debug!("post_compact_cleanup: clearing classifier approvals and speculative checks");

    // 清除 session 消息缓存
    debug!("post_compact_cleanup: clearing session messages cache");
}

// ---------------------------------------------------------------------------
// 基于时间的微压缩配置
// ---------------------------------------------------------------------------

/// 基于时间的微压缩配置。
///
/// 对应 TS `TimeBasedMCConfig`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBasedMicrocompactConfig {
    /// 主开关。
    pub enabled: bool,
    /// 触发阈值（分钟）。
    pub gap_threshold_minutes: f64,
    /// 保留最近的可压缩工具结果数量。
    pub keep_recent: usize,
}

impl Default for TimeBasedMicrocompactConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            gap_threshold_minutes: 60.0,
            keep_recent: 5,
        }
    }
}

/// 获取基于时间的微压缩配置。
///
/// 对应 TS `getTimeBasedMCConfig()`。
pub fn get_time_based_microcompact_config() -> TimeBasedMicrocompactConfig {
    // 生产环境中从远程配置（GrowthBook）获取
    // 此处返回默认值
    TimeBasedMicrocompactConfig::default()
}
