//! # retry — 重试逻辑
//!
//! 对应 TS `services/api/withRetry.ts`，实现指数退避重试、529 过载回退等。

use std::time::Duration;

use rand::Rng;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::api_client::ApiError;
use crate::types::ThinkingConfig;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 默认最大重试次数。
const DEFAULT_MAX_RETRIES: u32 = 10;
/// 基础延迟（毫秒）。
const BASE_DELAY_MS: u64 = 500;
/// 短重试阈值（毫秒）。
const SHORT_RETRY_THRESHOLD_MS: u64 = 20_000;
/// 最大 529 重试次数。
const MAX_529_RETRIES: u32 = 3;
/// 最低输出 token 数。
const FLOOR_OUTPUT_TOKENS: u32 = 3000;
/// 持久化最大退避（毫秒）。
const PERSISTENT_MAX_BACKOFF_MS: u64 = 5 * 60 * 1000;
/// 持久化重置上限（毫秒）。
const PERSISTENT_RESET_CAP_MS: u64 = 6 * 60 * 60 * 1000;
/// 心跳间隔（毫秒）。
const HEARTBEAT_INTERVAL_MS: u64 = 30_000;

// ---------------------------------------------------------------------------
// 重试配置
// ---------------------------------------------------------------------------

/// 重试配置。
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 最大重试次数。
    pub max_retries: u32,
    /// 模型。
    pub model: String,
    /// 回退模型。
    pub fallback_model: Option<String>,
    /// 思考配置。
    pub thinking_config: Option<ThinkingConfig>,
    /// 快速模式。
    pub fast_mode: Option<bool>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            model: String::new(),
            fallback_model: None,
            thinking_config: None,
            fast_mode: None,
        }
    }
}

/// 重试上下文——可变参数在重试过程中可能被调整。
#[derive(Debug, Clone)]
pub struct RetryContext {
    /// max_tokens 覆盖。
    pub max_tokens_override: Option<u32>,
    /// 当前模型。
    pub model: String,
    /// 思考配置。
    pub thinking_config: Option<ThinkingConfig>,
    /// 快速模式。
    pub fast_mode: Option<bool>,
}

// ---------------------------------------------------------------------------
// 重试错误
// ---------------------------------------------------------------------------

/// 重试决策错误。
#[derive(Debug, thiserror::Error)]
pub enum RetryError {
    #[error("Cannot retry: {0}")]
    CannotRetry(#[source] anyhow::Error),

    #[error("Fallback triggered: {original} -> {fallback}")]
    FallbackTriggered { original: String, fallback: String },

    #[error("User aborted")]
    UserAbort,
}

/// 系统 API 错误通知（用于 UI 展示）。
#[derive(Debug, Clone)]
pub struct SystemApiErrorNotification {
    pub error: ApiError,
    pub retry_in_ms: u64,
    pub attempt: u32,
    pub max_retries: u32,
}

// ---------------------------------------------------------------------------
// 重试执行
// ---------------------------------------------------------------------------

/// 带重试的 API 调用执行器。
///
/// 对应 TS `withRetry()`。
pub async fn with_retry<T, F, Fut>(
    operation: F,
    config: &RetryConfig,
    cancel: &CancellationToken,
    mut on_retry: impl FnMut(SystemApiErrorNotification),
) -> Result<T, RetryError>
where
    F: Fn(u32, &RetryContext) -> Fut,
    Fut: std::future::Future<Output = Result<T, ApiError>>,
{
    let mut ctx = RetryContext {
        model: config.model.clone(),
        thinking_config: config.thinking_config.clone(),
        fast_mode: config.fast_mode,
        max_tokens_override: None,
    };

    let mut consecutive_529: u32 = 0;

    for attempt in 1..=(config.max_retries + 1) {
        if cancel.is_cancelled() {
            return Err(RetryError::UserAbort);
        }

        match operation(attempt, &ctx).await {
            Ok(result) => return Ok(result),
            Err(error) => {
                // 529 过载跟踪
                if matches!(error, ApiError::Overloaded { .. }) {
                    consecutive_529 += 1;
                    if consecutive_529 >= MAX_529_RETRIES {
                        if let Some(ref fallback) = config.fallback_model {
                            return Err(RetryError::FallbackTriggered {
                                original: config.model.clone(),
                                fallback: fallback.clone(),
                            });
                        }
                    }
                } else {
                    consecutive_529 = 0;
                }

                // 上下文溢出——调整 max_tokens
                if let ApiError::ContextOverflow {
                    input_tokens,
                    limit,
                } = &error
                {
                    if let Some(adjusted) = adjust_max_tokens_for_overflow(*input_tokens, *limit) {
                        ctx.max_tokens_override = Some(adjusted);
                        debug!(adjusted, "Adjusted max_tokens after context overflow");
                        continue;
                    }
                }

                if !error.is_retryable() || attempt > config.max_retries {
                    return Err(RetryError::CannotRetry(anyhow::anyhow!("{}", error)));
                }

                // 计算退避延迟
                let delay = compute_retry_delay(attempt, None);

                // 通知 UI
                on_retry(SystemApiErrorNotification {
                    error: error.clone(),
                    retry_in_ms: delay.as_millis() as u64,
                    attempt,
                    max_retries: config.max_retries,
                });

                warn!(
                    attempt,
                    max_retries = config.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = %error,
                    "Retrying API call"
                );

                // 等待退避延迟（可取消）
                tokio::select! {
                    _ = sleep(delay) => {}
                    _ = cancel.cancelled() => {
                        return Err(RetryError::UserAbort);
                    }
                }
            }
        }
    }

    Err(RetryError::CannotRetry(anyhow::anyhow!(
        "Max retries ({}) exceeded",
        config.max_retries
    )))
}

// ---------------------------------------------------------------------------
// 退避计算
// ---------------------------------------------------------------------------

/// 计算指数退避延迟（带抖动）。
pub fn compute_retry_delay(attempt: u32, retry_after: Option<&str>) -> Duration {
    // 优先使用 Retry-After header
    if let Some(header) = retry_after {
        if let Ok(secs) = header.parse::<u64>() {
            return Duration::from_secs(secs);
        }
    }

    let base = (BASE_DELAY_MS as f64) * 2.0_f64.powi(attempt as i32 - 1);
    let capped = base.min(32_000.0);
    let jitter = rand::thread_rng().gen_range(0.0..0.25) * capped;
    Duration::from_millis((capped + jitter) as u64)
}

/// 上下文溢出时调整 max_tokens。
fn adjust_max_tokens_for_overflow(input_tokens: u64, limit: u64) -> Option<u32> {
    if limit == 0 || input_tokens == 0 {
        return None;
    }
    let available = limit.saturating_sub(input_tokens);
    let adjusted = (available as u32).max(FLOOR_OUTPUT_TOKENS);
    Some(adjusted)
}

/// 判断错误是否为 429。
pub fn is_rate_limited(error: &ApiError) -> bool {
    matches!(error, ApiError::RateLimited { .. })
}

/// 判断错误是否为 529。
pub fn is_overloaded(error: &ApiError) -> bool {
    matches!(error, ApiError::Overloaded { .. })
}
