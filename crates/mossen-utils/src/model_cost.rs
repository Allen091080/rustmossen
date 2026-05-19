//! Model cost calculation utilities.
//!
//! Provides pricing tiers, cost calculation from token usage, and formatting
//! for model pricing display.

use std::collections::HashMap;

use once_cell::sync::Lazy;

/// Model cost configuration per million tokens.
#[derive(Debug, Clone, Copy)]
pub struct ModelCosts {
    pub input_tokens: f64,
    pub output_tokens: f64,
    pub prompt_cache_write_tokens: f64,
    pub prompt_cache_read_tokens: f64,
    pub web_search_requests: f64,
}

/// Standard pricing tier for Balanced models: $3 input / $15 output per Mtok.
pub const COST_TIER_3_15: ModelCosts = ModelCosts {
    input_tokens: 3.0,
    output_tokens: 15.0,
    prompt_cache_write_tokens: 3.75,
    prompt_cache_read_tokens: 0.3,
    web_search_requests: 0.01,
};

/// Pricing tier for Opus 4/4.1: $15 input / $75 output per Mtok.
pub const COST_TIER_15_75: ModelCosts = ModelCosts {
    input_tokens: 15.0,
    output_tokens: 75.0,
    prompt_cache_write_tokens: 18.75,
    prompt_cache_read_tokens: 1.5,
    web_search_requests: 0.01,
};

/// Pricing tier for Opus 4.5: $5 input / $25 output per Mtok.
pub const COST_TIER_5_25: ModelCosts = ModelCosts {
    input_tokens: 5.0,
    output_tokens: 25.0,
    prompt_cache_write_tokens: 6.25,
    prompt_cache_read_tokens: 0.5,
    web_search_requests: 0.01,
};

/// Fast mode pricing for Opus 4.6: $30 input / $150 output per Mtok.
pub const COST_TIER_30_150: ModelCosts = ModelCosts {
    input_tokens: 30.0,
    output_tokens: 150.0,
    prompt_cache_write_tokens: 37.5,
    prompt_cache_read_tokens: 3.0,
    web_search_requests: 0.01,
};

/// Pricing for Haiku 3.5: $0.80 input / $4 output per Mtok.
pub const COST_HAIKU_35: ModelCosts = ModelCosts {
    input_tokens: 0.8,
    output_tokens: 4.0,
    prompt_cache_write_tokens: 1.0,
    prompt_cache_read_tokens: 0.08,
    web_search_requests: 0.01,
};

/// Pricing for Haiku 4.5: $1 input / $5 output per Mtok.
pub const COST_HAIKU_45: ModelCosts = ModelCosts {
    input_tokens: 1.0,
    output_tokens: 5.0,
    prompt_cache_write_tokens: 1.25,
    prompt_cache_read_tokens: 0.1,
    web_search_requests: 0.01,
};

/// Default cost for unknown models.
pub const DEFAULT_UNKNOWN_MODEL_COST: ModelCosts = COST_TIER_5_25;

/// A type alias for model short names.
pub type ModelShortName = String;

/// Token usage from an API response.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub web_search_requests: Option<u64>,
    /// Speed tier (e.g., "fast") for dynamic pricing.
    pub speed: Option<String>,
}

/// Model configuration entry for name mapping.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub first_party: String,
    pub canonical: String,
}

/// Static model cost registry.
static MODEL_COSTS: Lazy<HashMap<String, ModelCosts>> = Lazy::new(|| {
    let mut m = HashMap::new();
    // Haiku models
    m.insert("mossen-3-5-haiku".to_string(), COST_HAIKU_35);
    m.insert("mossen-haiku-4-5".to_string(), COST_HAIKU_45);
    // Sonnet models
    m.insert("mossen-3-5-sonnet-v2".to_string(), COST_TIER_3_15);
    m.insert("mossen-3-7-sonnet".to_string(), COST_TIER_3_15);
    m.insert("mossen-sonnet-4".to_string(), COST_TIER_3_15);
    m.insert("mossen-sonnet-4-5".to_string(), COST_TIER_3_15);
    m.insert("mossen-sonnet-4-6".to_string(), COST_TIER_3_15);
    // Opus models
    m.insert("mossen-opus-4".to_string(), COST_TIER_15_75);
    m.insert("mossen-opus-4-1".to_string(), COST_TIER_15_75);
    m.insert("mossen-opus-4-5".to_string(), COST_TIER_5_25);
    m.insert("mossen-opus-4-6".to_string(), COST_TIER_5_25);
    m
});

/// Get the canonical short name for a model string.
pub fn get_canonical_name(model: &str) -> String {
    // Simple canonicalization: lowercase, strip version suffixes
    let lower = model.to_lowercase();
    // Try direct match first
    if MODEL_COSTS.contains_key(&lower) {
        return lower;
    }
    // Try stripping date suffixes like "-20241022"
    let parts: Vec<&str> = lower.split('-').collect();
    // Check if last part looks like a date (8 digits)
    if let Some(last) = parts.last() {
        if last.len() == 8 && last.chars().all(|c| c.is_ascii_digit()) {
            let without_date = parts[..parts.len() - 1].join("-");
            if MODEL_COSTS.contains_key(&without_date) {
                return without_date;
            }
        }
    }
    lower
}

/// Check if fast mode is enabled (reads from env).
fn is_fast_mode_enabled() -> bool {
    std::env::var("MOSSEN_CODE_FAST_MODE")
        .ok()
        .map(|v| {
            let v = v.trim().to_lowercase();
            !v.is_empty() && v != "0" && v != "false" && v != "no"
        })
        .unwrap_or(false)
}

/// Get the cost tier for Opus 4.6 based on fast mode.
pub fn get_opus_46_cost_tier(fast_mode: bool) -> ModelCosts {
    if is_fast_mode_enabled() && fast_mode {
        COST_TIER_30_150
    } else {
        COST_TIER_5_25
    }
}

/// Get the model costs for a given model name and usage.
pub fn get_model_costs(model: &str, usage: &TokenUsage) -> ModelCosts {
    let short_name = get_canonical_name(model);

    // Check if this is an Opus 4.6 model with fast mode active
    if short_name == "mossen-opus-4-6" {
        let is_fast = usage.speed.as_deref() == Some("fast");
        return get_opus_46_cost_tier(is_fast);
    }

    if let Some(costs) = MODEL_COSTS.get(&short_name) {
        return *costs;
    }

    // Unknown model - fall back to default
    DEFAULT_UNKNOWN_MODEL_COST
}

/// Calculate USD cost from token usage and model costs.
fn tokens_to_usd_cost(model_costs: &ModelCosts, usage: &TokenUsage) -> f64 {
    let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * model_costs.input_tokens;
    let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * model_costs.output_tokens;
    let cache_read_cost = (usage.cache_read_input_tokens.unwrap_or(0) as f64 / 1_000_000.0)
        * model_costs.prompt_cache_read_tokens;
    let cache_write_cost = (usage.cache_creation_input_tokens.unwrap_or(0) as f64 / 1_000_000.0)
        * model_costs.prompt_cache_write_tokens;
    let web_search_cost =
        usage.web_search_requests.unwrap_or(0) as f64 * model_costs.web_search_requests;

    input_cost + output_cost + cache_read_cost + cache_write_cost + web_search_cost
}

/// Calculate the cost of a query in US dollars.
pub fn calculate_usd_cost(resolved_model: &str, usage: &TokenUsage) -> f64 {
    let model_costs = get_model_costs(resolved_model, usage);
    tokens_to_usd_cost(&model_costs, usage)
}

/// Calculate cost from raw token counts without requiring a full TokenUsage object.
pub fn calculate_cost_from_tokens(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_input_tokens: u64,
    cache_creation_input_tokens: u64,
) -> f64 {
    let usage = TokenUsage {
        input_tokens,
        output_tokens,
        cache_read_input_tokens: Some(cache_read_input_tokens),
        cache_creation_input_tokens: Some(cache_creation_input_tokens),
        ..Default::default()
    };
    calculate_usd_cost(model, &usage)
}

/// Format a price value for display.
fn format_price(price: f64) -> String {
    if price == price.floor() && price == (price as i64) as f64 {
        format!("${}", price as i64)
    } else {
        format!("${:.2}", price)
    }
}

/// Format model costs as a pricing string for display.
/// e.g., "$3/$15 per Mtok"
pub fn format_model_pricing(costs: &ModelCosts) -> String {
    format!(
        "{}/{} per Mtok",
        format_price(costs.input_tokens),
        format_price(costs.output_tokens)
    )
}

/// Get formatted pricing string for a model.
/// Returns None if model is not found.
pub fn get_model_pricing_string(model: &str) -> Option<String> {
    let short_name = get_canonical_name(model);
    MODEL_COSTS.get(&short_name).map(format_model_pricing)
}
