//! Empty usage constant.
//! Translated from `services/api/emptyUsage.ts` (22 lines).

use serde::{Deserialize, Serialize};

/// Server tool use breakdown in usage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerToolUse {
    pub web_search_requests: u64,
    pub web_fetch_requests: u64,
}

/// Cache creation breakdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheCreation {
    pub ephemeral_1h_input_tokens: u64,
    pub ephemeral_5m_input_tokens: u64,
}

/// Iteration entry in usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageIteration {
    #[serde(flatten)]
    pub data: serde_json::Value,
}

/// Full non-nullable usage struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NonNullableUsage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub server_tool_use: ServerToolUse,
    #[serde(default)]
    pub service_tier: String,
    #[serde(default)]
    pub cache_creation: CacheCreation,
    #[serde(default)]
    pub inference_geo: String,
    #[serde(default)]
    pub iterations: Vec<UsageIteration>,
    #[serde(default)]
    pub speed: String,
}

/// Zero-initialized usage object.
pub static EMPTY_USAGE: std::sync::LazyLock<NonNullableUsage> =
    std::sync::LazyLock::new(|| NonNullableUsage {
        input_tokens: 0,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        output_tokens: 0,
        server_tool_use: ServerToolUse {
            web_search_requests: 0,
            web_fetch_requests: 0,
        },
        service_tier: "standard".to_string(),
        cache_creation: CacheCreation {
            ephemeral_1h_input_tokens: 0,
            ephemeral_5m_input_tokens: 0,
        },
        inference_geo: String::new(),
        iterations: Vec::new(),
        speed: "standard".to_string(),
    });
