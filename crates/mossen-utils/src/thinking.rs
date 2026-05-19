use once_cell::sync::Lazy;
use regex::Regex;

/// Thinking configuration.
#[derive(Debug, Clone, PartialEq)]
pub enum ThinkingConfig {
    Adaptive,
    Enabled { budget_tokens: u64 },
    Disabled,
}

/// Check if ultrathink feature is enabled.
/// Build-time gate + runtime gate.
pub fn is_ultrathink_enabled() -> bool {
    // Runtime gate via environment variable
    !std::env::var("TENGU_TURTLE_CARBON_DISABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

static ULTRATHINK_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bultrathink\b").unwrap());

/// Check if text contains the "ultrathink" keyword.
pub fn has_ultrathink_keyword(text: &str) -> bool {
    ULTRATHINK_REGEX.is_match(text)
}

/// Position of a thinking trigger keyword.
#[derive(Debug, Clone)]
pub struct ThinkingTriggerPosition {
    pub word: String,
    pub start: usize,
    pub end: usize,
}

/// Find positions of "ultrathink" keyword in text.
pub fn find_thinking_trigger_positions(text: &str) -> Vec<ThinkingTriggerPosition> {
    ULTRATHINK_REGEX
        .find_iter(text)
        .map(|m| ThinkingTriggerPosition {
            word: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        })
        .collect()
}

/// Rainbow color keys.
pub const RAINBOW_COLORS: &[&str] = &[
    "rainbow_red",
    "rainbow_orange",
    "rainbow_yellow",
    "rainbow_green",
    "rainbow_blue",
    "rainbow_indigo",
    "rainbow_violet",
];

/// Rainbow shimmer color keys.
pub const RAINBOW_SHIMMER_COLORS: &[&str] = &[
    "rainbow_red_shimmer",
    "rainbow_orange_shimmer",
    "rainbow_yellow_shimmer",
    "rainbow_green_shimmer",
    "rainbow_blue_shimmer",
    "rainbow_indigo_shimmer",
    "rainbow_violet_shimmer",
];

/// Get rainbow color for a given character index.
pub fn get_rainbow_color(char_index: usize, shimmer: bool) -> &'static str {
    let colors = if shimmer {
        RAINBOW_SHIMMER_COLORS
    } else {
        RAINBOW_COLORS
    };
    colors[char_index % colors.len()]
}

/// Check if a model supports thinking.
pub fn model_supports_thinking(model: &str, api_provider: &str) -> bool {
    let canonical = get_canonical_name(model);

    // 1P and Foundry: all Mossen 4+ models
    if api_provider == "foundry" || api_provider == "firstParty" {
        return !canonical.contains("mossen-3-");
    }

    // 3P (Bedrock/Vertex): only Opus 4+ and Sonnet 4+
    canonical.contains("sonnet-4") || canonical.contains("opus-4")
}

/// Check if a model supports adaptive thinking.
pub fn model_supports_adaptive_thinking(model: &str, api_provider: &str) -> bool {
    let canonical = get_canonical_name(model);

    // Supported by a subset of Mossen 4 models
    if canonical.contains("opus-4-6") || canonical.contains("sonnet-4-6") {
        return true;
    }

    // Exclude known legacy models
    if canonical.contains("opus")
        || canonical.contains("sonnet")
        || canonical.contains("haiku")
    {
        return false;
    }

    // Default to true for 1P and Foundry
    api_provider == "firstParty" || api_provider == "foundry"
}

/// Check if thinking should be enabled by default.
pub fn should_enable_thinking_by_default(
    custom_backend_enabled: bool,
    max_thinking_tokens_env: Option<&str>,
    always_thinking_enabled_setting: Option<bool>,
) -> bool {
    if custom_backend_enabled {
        let custom_enable = std::env::var("MOSSEN_CODE_CUSTOM_ENABLE_THINKING")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);
        if !custom_enable {
            return false;
        }
    }

    if let Some(max_tokens) = max_thinking_tokens_env {
        if let Ok(val) = max_tokens.parse::<i64>() {
            return val > 0;
        }
    }

    if let Some(false) = always_thinking_enabled_setting {
        return false;
    }

    true
}

/// Helper: get canonical model name (lowercase).
fn get_canonical_name(model: &str) -> String {
    model.to_lowercase()
}
