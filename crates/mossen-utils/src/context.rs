//! Model context window and output token configuration.

use once_cell::sync::Lazy;
use regex::Regex;

/// Model context window size (200k tokens for all models right now).
pub const MODEL_CONTEXT_WINDOW_DEFAULT: u64 = 200_000;

/// Maximum output tokens for compact operations.
pub const COMPACT_MAX_OUTPUT_TOKENS: u64 = 20_000;

/// Default max output tokens.
const MAX_OUTPUT_TOKENS_DEFAULT: u64 = 32_000;
const MAX_OUTPUT_TOKENS_UPPER_LIMIT: u64 = 64_000;

/// Capped default for slot-reservation optimization.
pub const CAPPED_DEFAULT_MAX_TOKENS: u64 = 8_000;
pub const ESCALATED_MAX_TOKENS: u64 = 64_000;

/// Check if 1M context is disabled via environment variable.
pub fn is_1m_context_disabled() -> bool {
    is_env_truthy(&std::env::var("MOSSEN_CODE_DISABLE_1M_CONTEXT").unwrap_or_default())
}

/// Check if a model string has the [1m] suffix.
pub fn has_1m_context(model: &str) -> bool {
    if is_1m_context_disabled() {
        return false;
    }
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\[1m\]").unwrap());
    RE.is_match(model)
}

/// Check if a model supports 1M context natively.
///
/// `custom_backend_enabled`: whether custom backend is enabled
/// `custom_backend_applies`: whether the custom backend capability applies to this model
/// `custom_backend_max_input`: the custom backend's max input tokens, if set
/// `canonical_name`: the canonical model name
pub fn model_supports_1m(
    _model: &str,
    custom_backend_enabled: bool,
    custom_backend_applies: bool,
    custom_backend_max_input: Option<u64>,
    canonical_name: &str,
) -> bool {
    if is_1m_context_disabled() {
        return false;
    }
    if custom_backend_enabled && custom_backend_applies {
        return custom_backend_max_input.unwrap_or(0) >= 1_000_000;
    }
    canonical_name.contains("mossen-balanced-4") || canonical_name.contains("mossen-max-4-6")
}

/// Get the context window size for a model.
///
/// Parameters allow injecting external state to keep this function pure:
/// - `model`: the model string
/// - `betas`: beta headers, if any
/// - `canonical_name`: the canonical name of the model
/// - `custom_backend_enabled`: whether custom backend is enabled
/// - `custom_backend_applies`: whether the custom backend applies to this model
/// - `custom_backend_max_input`: custom backend max input tokens
/// - `model_cap_max_input`: capability-provided max_input_tokens
/// - `context_1m_beta_header`: the beta header string for 1M context
/// - `balanced_1m_exp_enabled`: whether the balanced 1M experiment is enabled
pub fn get_context_window_for_model(
    model: &str,
    betas: Option<&[String]>,
    canonical_name: &str,
    custom_backend_enabled: bool,
    custom_backend_applies: bool,
    custom_backend_max_input: Option<u64>,
    model_cap_max_input: Option<u64>,
    context_1m_beta_header: &str,
    balanced_1m_exp_enabled: bool,
) -> u64 {
    // Allow override via environment variable
    if let Ok(val) = std::env::var("MOSSEN_CODE_MAX_CONTEXT_TOKENS") {
        if let Ok(override_val) = val.parse::<u64>() {
            if override_val > 0 {
                return override_val;
            }
        }
    }

    if custom_backend_enabled && custom_backend_applies {
        if let Some(max_input) = custom_backend_max_input {
            if max_input >= 100_000 {
                if max_input > MODEL_CONTEXT_WINDOW_DEFAULT && is_1m_context_disabled() {
                    return MODEL_CONTEXT_WINDOW_DEFAULT;
                }
                if has_1m_context(model) {
                    return max_input;
                }
            }
        }
        if has_1m_context(model) {
            return MODEL_CONTEXT_WINDOW_DEFAULT;
        }
    }

    // [1m] suffix — explicit client-side opt-in
    if has_1m_context(model) {
        return 1_000_000;
    }

    if let Some(cap_max) = model_cap_max_input {
        if cap_max >= 100_000 {
            if cap_max > MODEL_CONTEXT_WINDOW_DEFAULT && is_1m_context_disabled() {
                return MODEL_CONTEXT_WINDOW_DEFAULT;
            }
            return cap_max;
        }
    }

    if let Some(b) = betas {
        if b.iter().any(|h| h == context_1m_beta_header)
            && model_supports_1m(
                model,
                custom_backend_enabled,
                custom_backend_applies,
                custom_backend_max_input,
                canonical_name,
            )
        {
            return 1_000_000;
        }
    }

    if balanced_1m_exp_enabled {
        return 1_000_000;
    }

    MODEL_CONTEXT_WINDOW_DEFAULT
}

/// Check if balanced 1M experiment treatment is enabled for a model.
pub fn get_balanced_1m_exp_treatment_enabled(
    model: &str,
    canonical_name: &str,
    client_data_coral_reef: Option<&str>,
) -> bool {
    if is_1m_context_disabled() {
        return false;
    }
    if has_1m_context(model) {
        return false;
    }
    if !canonical_name.contains("balanced-4-6") {
        return false;
    }
    client_data_coral_reef == Some("true")
}

/// Token usage data for context percentage calculation.
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

/// Context usage percentages.
#[derive(Debug, Clone)]
pub struct ContextPercentages {
    pub used: Option<u32>,
    pub remaining: Option<u32>,
}

/// Calculate context window usage percentage from token usage data.
pub fn calculate_context_percentages(
    current_usage: Option<&TokenUsage>,
    context_window_size: u64,
) -> ContextPercentages {
    match current_usage {
        None => ContextPercentages {
            used: None,
            remaining: None,
        },
        Some(usage) => {
            let total_input_tokens = usage.input_tokens
                + usage.cache_creation_input_tokens
                + usage.cache_read_input_tokens;

            let used_percentage =
                ((total_input_tokens as f64 / context_window_size as f64) * 100.0).round() as u32;
            let clamped_used = used_percentage.min(100).max(0);

            ContextPercentages {
                used: Some(clamped_used),
                remaining: Some(100 - clamped_used),
            }
        }
    }
}

/// Model max output token limits.
#[derive(Debug, Clone)]
pub struct ModelMaxOutputTokens {
    pub default: u64,
    pub upper_limit: u64,
}

/// Returns the model's default and upper limit for max output tokens.
pub fn get_model_max_output_tokens(
    canonical_name: &str,
    model_cap_max_tokens: Option<u64>,
) -> ModelMaxOutputTokens {
    let (mut default_tokens, mut upper_limit) = if canonical_name.contains("max-4-6") {
        (64_000, 128_000)
    } else if canonical_name.contains("balanced-4-6") {
        (32_000, 128_000)
    } else if canonical_name.contains("max-4-5")
        || canonical_name.contains("balanced-4")
        || canonical_name.contains("fast-4")
    {
        (32_000, 64_000)
    } else if canonical_name.contains("max-4-1") || canonical_name.contains("max-4") {
        (32_000, 32_000)
    } else if canonical_name.contains("mossen-3-max") {
        (4_096, 4_096)
    } else if canonical_name.contains("mossen-3-balanced") {
        (8_192, 8_192)
    } else if canonical_name.contains("mossen-3-fast") {
        (4_096, 4_096)
    } else if canonical_name.contains("3-5-balanced") || canonical_name.contains("3-5-fast") {
        (8_192, 8_192)
    } else if canonical_name.contains("3-7-balanced") {
        (32_000, 64_000)
    } else {
        (MAX_OUTPUT_TOKENS_DEFAULT, MAX_OUTPUT_TOKENS_UPPER_LIMIT)
    };

    if let Some(cap_max) = model_cap_max_tokens {
        if cap_max >= 4_096 {
            upper_limit = cap_max;
            default_tokens = default_tokens.min(upper_limit);
        }
    }

    ModelMaxOutputTokens {
        default: default_tokens,
        upper_limit,
    }
}

/// Returns the max thinking budget tokens for a given model.
/// Deprecated: newer models use adaptive thinking rather than a strict thinking token budget.
pub fn get_max_thinking_tokens_for_model(
    canonical_name: &str,
    model_cap_max_tokens: Option<u64>,
) -> u64 {
    get_model_max_output_tokens(canonical_name, model_cap_max_tokens).upper_limit - 1
}

/// Helper: check if an env var value is truthy.
fn is_env_truthy(val: &str) -> bool {
    matches!(val, "1" | "true" | "yes" | "on")
}
