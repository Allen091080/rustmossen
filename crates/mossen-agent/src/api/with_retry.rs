//! # Retry Logic with Exponential Backoff
//!
//! 翻译自 `services/api/withRetry.ts` (830行)
//! 提供带指数退避的重试逻辑、错误分类和重试决策。

use rand::Rng;
use std::time::Duration;

use super::sdk::{ApiError, MossenAPIError, MossenAPIUserAbortError};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_MAX_RETRIES: u32 = 10;
const FLOOR_OUTPUT_TOKENS: u64 = 3000;
const MAX_529_RETRIES: u32 = 3;
pub const BASE_DELAY_MS: u64 = 500;

const PERSISTENT_MAX_BACKOFF_MS: u64 = 5 * 60 * 1000;
const PERSISTENT_RESET_CAP_MS: u64 = 6 * 60 * 60 * 1000;
const HEARTBEAT_INTERVAL_MS: u64 = 30_000;

const DEFAULT_FAST_MODE_FALLBACK_HOLD_MS: u64 = 30 * 60 * 1000;
const SHORT_RETRY_THRESHOLD_MS: u64 = 20 * 1000;
const MIN_COOLDOWN_MS: u64 = 10 * 60 * 1000;

/// Thinking configuration for retry context.
#[derive(Debug, Clone)]
pub enum ThinkingConfig {
    Enabled { budget_tokens: u64 },
    Adaptive,
    Disabled,
}

/// Retry context carried across attempts.
#[derive(Debug, Clone)]
pub struct RetryContext {
    pub max_tokens_override: Option<u64>,
    pub model: String,
    pub thinking_config: ThinkingConfig,
    pub fast_mode: Option<bool>,
}

/// Options for the retry loop.
#[derive(Debug, Clone)]
pub struct RetryOptions {
    pub max_retries: Option<u32>,
    pub model: String,
    pub fallback_model: Option<String>,
    pub thinking_config: ThinkingConfig,
    pub fast_mode: Option<bool>,
    pub query_source: Option<String>,
    pub initial_consecutive_529_errors: Option<u32>,
}

/// Error indicating the operation cannot be retried.
#[derive(Debug, thiserror::Error)]
#[error("Cannot retry: {message}")]
pub struct CannotRetryError {
    pub original_error: ApiError,
    pub retry_context: RetryContext,
    pub message: String,
}

impl CannotRetryError {
    pub fn new(original_error: ApiError, retry_context: RetryContext) -> Self {
        let message = original_error.to_string();
        Self {
            original_error,
            retry_context,
            message,
        }
    }
}

/// Error indicating a model fallback was triggered.
#[derive(Debug, thiserror::Error)]
#[error("Model fallback triggered: {original_model} -> {fallback_model}")]
pub struct FallbackTriggeredError {
    pub original_model: String,
    pub fallback_model: String,
}

/// Result of a retry operation — either yields a system message or returns a value.
#[derive(Debug)]
pub enum RetryYield {
    SystemMessage {
        status: u16,
        delay_ms: u64,
        attempt: u32,
        max_retries: u32,
    },
    Complete,
}

/// Check if an error is a 529 overloaded error.
pub fn is_529_error(error: &MossenAPIError) -> bool {
    error.status == 529 || error.message.contains("\"type\":\"overloaded_error\"")
}

/// Get the retry delay with exponential backoff.
pub fn get_retry_delay(attempt: u32, retry_after_header: Option<&str>, max_delay_ms: u64) -> u64 {
    if let Some(header) = retry_after_header {
        if let Ok(seconds) = header.parse::<u64>() {
            return seconds * 1000;
        }
    }
    let base_delay = (BASE_DELAY_MS * 2u64.pow(attempt.saturating_sub(1))).min(max_delay_ms);
    let jitter = rand::thread_rng().gen_range(0.0..0.25) * base_delay as f64;
    base_delay + jitter as u64
}

/// Get retry delay with default max.
pub fn get_retry_delay_default(attempt: u32, retry_after_header: Option<&str>) -> u64 {
    get_retry_delay(attempt, retry_after_header, 32000)
}

/// Parse max_tokens context overflow error to extract token counts.
pub fn parse_max_tokens_context_overflow_error(error: &MossenAPIError) -> Option<(u64, u64, u64)> {
    if error.status != 400 {
        return None;
    }
    let message = &error.message;
    if !message.contains("input length and `max_tokens` exceed context limit") {
        return None;
    }

    let re = regex::Regex::new(
        r"input length and `max_tokens` exceed context limit: (\d+) \+ (\d+) > (\d+)",
    )
    .ok()?;

    let caps = re.captures(message)?;
    let input_tokens: u64 = caps.get(1)?.as_str().parse().ok()?;
    let max_tokens: u64 = caps.get(2)?.as_str().parse().ok()?;
    let context_limit: u64 = caps.get(3)?.as_str().parse().ok()?;

    Some((input_tokens, max_tokens, context_limit))
}

/// Determines if the error should be retried.
pub fn should_retry(error: &MossenAPIError, is_hosted_subscriber: bool) -> bool {
    let status = error.status;
    let message = &error.message;

    // Check for overloaded errors
    if message.contains("\"type\":\"overloaded_error\"") {
        return true;
    }

    // Check for max tokens context overflow errors
    if parse_max_tokens_context_overflow_error(error).is_some() {
        return true;
    }

    // Check x-should-retry header
    let should_retry_header = error
        .headers
        .get("x-should-retry")
        .and_then(|v| v.to_str().ok());

    if should_retry_header == Some("true") && !is_hosted_subscriber {
        return true;
    }

    if should_retry_header == Some("false") {
        let is_5xx = status >= 500;
        if !is_5xx {
            return false;
        }
    }

    // Retry on request timeouts
    if status == 408 {
        return true;
    }

    // Retry on lock timeouts
    if status == 409 {
        return true;
    }

    // Retry on rate limits (not for hosted subscription users)
    if status == 429 {
        return !is_hosted_subscriber;
    }

    // Retry on 401
    if status == 401 {
        return true;
    }

    // Retry on "token revoked"
    if status == 403 && message.contains("OAuth token has been revoked") {
        return true;
    }

    // Retry internal errors
    if status >= 500 {
        return true;
    }

    false
}

/// Get the retry-after header value from error headers.
pub fn get_retry_after(error: &MossenAPIError) -> Option<String> {
    error
        .headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Get retry-after in milliseconds.
pub fn get_retry_after_ms(error: &MossenAPIError) -> Option<u64> {
    let retry_after = get_retry_after(error)?;
    let seconds: u64 = retry_after.parse().ok()?;
    Some(seconds * 1000)
}

/// Get rate limit reset delay from headers.
pub fn get_rate_limit_reset_delay_ms(error: &MossenAPIError) -> Option<u64> {
    let reset_header = error
        .headers
        .get("mossen-ratelimit-unified-reset")
        .and_then(|v| v.to_str().ok())?;
    let reset_unix_sec: f64 = reset_header.parse().ok()?;
    if !reset_unix_sec.is_finite() {
        return None;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let delay_ms = (reset_unix_sec * 1000.0) as u64;
    if delay_ms <= now_ms {
        return None;
    }
    Some((delay_ms - now_ms).min(PERSISTENT_RESET_CAP_MS))
}

/// Get default max retries from environment or default.
pub fn get_default_max_retries() -> u32 {
    std::env::var("MOSSEN_CODE_MAX_RETRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_RETRIES)
}

/// Perform a retryable operation with exponential backoff.
///
/// This is the main entry point for retry logic. It handles:
/// - Exponential backoff with jitter
/// - 529 error tracking and fallback triggering
/// - Max tokens overflow adjustment
/// - Authentication error recovery
/// - Abort signal handling
pub async fn with_retry<F, Fut, T>(
    operation: F,
    options: &RetryOptions,
    cancel: &CancellationToken,
) -> Result<T, CannotRetryError>
where
    F: Fn(u32, &RetryContext) -> Fut,
    Fut: std::future::Future<Output = Result<T, ApiError>>,
{
    let max_retries = options.max_retries.unwrap_or_else(get_default_max_retries);
    let mut retry_context = RetryContext {
        model: options.model.clone(),
        thinking_config: options.thinking_config.clone(),
        max_tokens_override: None,
        fast_mode: options.fast_mode,
    };
    let mut consecutive_529_errors = options.initial_consecutive_529_errors.unwrap_or(0);
    let mut last_error: Option<ApiError> = None;

    for attempt in 1..=(max_retries + 1) {
        if cancel.is_cancelled() {
            return Err(CannotRetryError::new(
                ApiError::UserAbort(MossenAPIUserAbortError),
                retry_context.clone(),
            ));
        }

        match operation(attempt, &retry_context).await {
            Ok(result) => return Ok(result),
            Err(error) => {
                last_error = Some(error.clone());

                // Track consecutive 529 errors
                if let ApiError::Api(ref api_err) = error {
                    if is_529_error(api_err) {
                        consecutive_529_errors += 1;
                        if consecutive_529_errors >= MAX_529_RETRIES {
                            if let Some(ref fallback) = options.fallback_model {
                                return Err(CannotRetryError {
                                    original_error: error,
                                    retry_context: retry_context.clone(),
                                    message: format!(
                                        "Model fallback triggered: {} -> {}",
                                        options.model, fallback
                                    ),
                                });
                            }
                        }
                    }
                }

                // Check if we've exhausted retries
                if attempt > max_retries {
                    return Err(CannotRetryError::new(error, retry_context.clone()));
                }

                // Check if error is retryable
                if let ApiError::Api(ref api_err) = error {
                    if !should_retry(api_err, false) {
                        return Err(CannotRetryError::new(error, retry_context.clone()));
                    }

                    // Handle max tokens context overflow
                    if let Some((input_tokens, _max_tokens, context_limit)) =
                        parse_max_tokens_context_overflow_error(api_err)
                    {
                        let safety_buffer = 1000u64;
                        let available = context_limit
                            .saturating_sub(input_tokens)
                            .saturating_sub(safety_buffer);
                        if available < FLOOR_OUTPUT_TOKENS {
                            return Err(CannotRetryError::new(error, retry_context.clone()));
                        }
                        let min_required = match &retry_context.thinking_config {
                            ThinkingConfig::Enabled { budget_tokens } => budget_tokens + 1,
                            _ => 1,
                        };
                        let adjusted = available.max(FLOOR_OUTPUT_TOKENS).max(min_required);
                        retry_context.max_tokens_override = Some(adjusted);
                        continue;
                    }

                    // Calculate delay
                    let retry_after = get_retry_after(api_err);
                    let delay_ms = get_retry_delay(attempt, retry_after.as_deref(), 32000);

                    // Sleep with cancellation support
                    tokio::select! {
                        _ = sleep(Duration::from_millis(delay_ms)) => {}
                        _ = cancel.cancelled() => {
                            return Err(CannotRetryError::new(
                                ApiError::UserAbort(MossenAPIUserAbortError),
                                retry_context.clone(),
                            ));
                        }
                    }
                } else {
                    return Err(CannotRetryError::new(error, retry_context.clone()));
                }
            }
        }
    }

    Err(CannotRetryError::new(
        last_error.unwrap_or(ApiError::Other("Unknown error".to_string())),
        retry_context,
    ))
}
