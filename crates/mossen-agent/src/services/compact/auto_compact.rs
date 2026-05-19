//! Auto-compact — automatic compaction triggered when context exceeds threshold.

use std::env;
use tracing::{debug, warn};

use crate::token_estimation::rough_token_count_estimation;
use mossen_types::{ContentBlock, Message};

use super::compact::{CompactionResult, RecompactionInfo, ERROR_MESSAGE_USER_ABORT};

/// Reserve this many tokens for output during compaction.
/// Based on p99.99 of compact summary output being 17,387 tokens.
const MAX_OUTPUT_TOKENS_FOR_SUMMARY: usize = 20_000;

/// Buffer tokens subtracted from context window for auto-compact threshold.
pub const AUTOCOMPACT_BUFFER_TOKENS: usize = 13_000;
pub const WARNING_THRESHOLD_BUFFER_TOKENS: usize = 20_000;
pub const ERROR_THRESHOLD_BUFFER_TOKENS: usize = 20_000;
pub const MANUAL_COMPACT_BUFFER_TOKENS: usize = 3_000;

/// Stop trying autocompact after this many consecutive failures.
const MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES: u32 = 3;

/// Tracking state for auto-compact across turns.
#[derive(Debug, Clone)]
pub struct AutoCompactTrackingState {
    pub compacted: bool,
    pub turn_counter: u64,
    pub turn_id: String,
    pub consecutive_failures: Option<u32>,
}

/// Returns the context window size minus the max output tokens for the model.
pub fn get_effective_context_window_size(model: &str) -> usize {
    let reserved_tokens = std::cmp::min(
        get_max_output_tokens_for_model(model),
        MAX_OUTPUT_TOKENS_FOR_SUMMARY,
    );
    let mut context_window = get_context_window_for_model(model);

    if let Ok(auto_compact_window) = env::var("MOSSEN_CODE_AUTO_COMPACT_WINDOW") {
        if let Ok(parsed) = auto_compact_window.parse::<usize>() {
            if parsed > 0 {
                context_window = std::cmp::min(context_window, parsed);
            }
        }
    }

    context_window.saturating_sub(reserved_tokens)
}

/// Get auto-compact threshold for a model.
pub fn get_auto_compact_threshold(model: &str) -> usize {
    let effective_context_window = get_effective_context_window_size(model);
    let autocompact_threshold = effective_context_window.saturating_sub(AUTOCOMPACT_BUFFER_TOKENS);

    // Override for easier testing of autocompact
    if let Ok(env_percent) = env::var("MOSSEN_AUTOCOMPACT_PCT_OVERRIDE") {
        if let Ok(parsed) = env_percent.parse::<f64>() {
            if parsed > 0.0 && parsed <= 100.0 {
                let percentage_threshold =
                    ((effective_context_window as f64) * (parsed / 100.0)) as usize;
                return std::cmp::min(percentage_threshold, autocompact_threshold);
            }
        }
    }

    autocompact_threshold
}

/// Token warning state for a given usage level.
#[derive(Debug, Clone)]
pub struct TokenWarningState {
    pub percent_left: usize,
    pub is_above_warning_threshold: bool,
    pub is_above_error_threshold: bool,
    pub is_above_auto_compact_threshold: bool,
    pub is_at_blocking_limit: bool,
}

/// Calculate the token warning state for given usage and model.
pub fn calculate_token_warning_state(token_usage: usize, model: &str) -> TokenWarningState {
    let auto_compact_threshold = get_auto_compact_threshold(model);
    let auto_compact_enabled = is_auto_compact_enabled();
    let threshold = if auto_compact_enabled {
        auto_compact_threshold
    } else {
        get_effective_context_window_size(model)
    };

    let percent_left = if token_usage >= threshold {
        0
    } else {
        ((threshold - token_usage) as f64 / threshold as f64 * 100.0).round() as usize
    };

    let warning_threshold = threshold.saturating_sub(WARNING_THRESHOLD_BUFFER_TOKENS);
    let error_threshold = threshold.saturating_sub(ERROR_THRESHOLD_BUFFER_TOKENS);

    let is_above_warning_threshold = token_usage >= warning_threshold;
    let is_above_error_threshold = token_usage >= error_threshold;
    let is_above_auto_compact_threshold = auto_compact_enabled && token_usage >= auto_compact_threshold;

    let actual_context_window = get_effective_context_window_size(model);
    let default_blocking_limit = actual_context_window.saturating_sub(MANUAL_COMPACT_BUFFER_TOKENS);

    let blocking_limit = if let Ok(override_str) = env::var("MOSSEN_CODE_BLOCKING_LIMIT_OVERRIDE") {
        override_str.parse::<usize>().unwrap_or(default_blocking_limit)
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

/// Check if auto-compact is enabled.
pub fn is_auto_compact_enabled() -> bool {
    if is_env_truthy("DISABLE_COMPACT") {
        return false;
    }
    if is_env_truthy("DISABLE_AUTO_COMPACT") {
        return false;
    }
    // In production, also check user config.
    true
}

/// Check if a query source should trigger auto-compact.
pub fn should_auto_compact(
    messages: &[Message],
    model: &str,
    query_source: Option<&str>,
    snip_tokens_freed: usize,
) -> bool {
    // Recursion guards
    if let Some(source) = query_source {
        if source == "session_memory" || source == "compact" {
            return false;
        }
    }

    if !is_auto_compact_enabled() {
        return false;
    }

    let token_count = token_count_with_estimation(messages).saturating_sub(snip_tokens_freed);
    let threshold = get_auto_compact_threshold(model);
    let effective_window = get_effective_context_window_size(model);

    debug!(
        "autocompact: tokens={} threshold={} effectiveWindow={}{}",
        token_count,
        threshold,
        effective_window,
        if snip_tokens_freed > 0 {
            format!(" snipFreed={}", snip_tokens_freed)
        } else {
            String::new()
        }
    );

    let state = calculate_token_warning_state(token_count, model);
    state.is_above_auto_compact_threshold
}

/// Result of auto-compact attempt.
#[derive(Debug, Clone)]
pub struct AutoCompactResult {
    pub was_compacted: bool,
    pub compaction_result: Option<CompactionResult>,
    pub consecutive_failures: Option<u32>,
}

/// Attempt auto-compaction if needed.
pub async fn auto_compact_if_needed(
    messages: &[Message],
    model: &str,
    query_source: Option<&str>,
    tracking: Option<&AutoCompactTrackingState>,
    snip_tokens_freed: usize,
) -> AutoCompactResult {
    if is_env_truthy("DISABLE_COMPACT") {
        return AutoCompactResult {
            was_compacted: false,
            compaction_result: None,
            consecutive_failures: None,
        };
    }

    // Circuit breaker
    if let Some(track) = tracking {
        if let Some(failures) = track.consecutive_failures {
            if failures >= MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES {
                return AutoCompactResult {
                    was_compacted: false,
                    compaction_result: None,
                    consecutive_failures: Some(failures),
                };
            }
        }
    }

    let should_compact = should_auto_compact(messages, model, query_source, snip_tokens_freed);
    if !should_compact {
        return AutoCompactResult {
            was_compacted: false,
            compaction_result: None,
            consecutive_failures: None,
        };
    }

    // In production, would call compact_conversation here.
    // For now, return not compacted (placeholder for integration).
    AutoCompactResult {
        was_compacted: false,
        compaction_result: None,
        consecutive_failures: None,
    }
}

// --- Helper functions ---

fn is_env_truthy(key: &str) -> bool {
    env::var(key)
        .ok()
        .map(|v| {
            let v = v.to_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(false)
}

fn get_context_window_for_model(model: &str) -> usize {
    match model {
        m if m.contains("opus") => 200_000,
        m if m.contains("sonnet") => 200_000,
        m if m.contains("haiku") => 200_000,
        m if m.contains("gpt-4") => 128_000,
        _ => 200_000,
    }
}

fn get_max_output_tokens_for_model(model: &str) -> usize {
    match model {
        m if m.contains("opus") => 32_000,
        m if m.contains("sonnet") => 64_000,
        m if m.contains("haiku") => 8_192,
        _ => 16_000,
    }
}

fn token_count_with_estimation(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| {
            let text: String = m.content.iter().filter_map(|block| {
                if let ContentBlock::Text(t) = block { Some(t.text.as_str()) } else { None }
            }).collect::<Vec<_>>().join("");
            rough_token_count_estimation(&text, 4) as usize
        })
        .sum()
}
