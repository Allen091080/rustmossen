//! # API Logging
//!
//! 翻译自 `services/api/logging.ts` (701行)
//! API 查询、错误和成功事件的日志记录。

use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error};

use super::client::{detect_gateway, KnownGateway};
use super::error_utils::extract_connection_error_details;
use super::errors::classify_api_error;
use super::sdk::{ApiError, MossenAPIError};

/// Re-export empty usage.
pub use super::empty_usage::EMPTY_USAGE;

/// Strategy used for global prompt caching.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GlobalCacheStrategy {
    ToolBased,
    SystemPrompt,
    None,
}

impl GlobalCacheStrategy {
    pub fn as_str(&self) -> &str {
        match self {
            GlobalCacheStrategy::ToolBased => "tool_based",
            GlobalCacheStrategy::SystemPrompt => "system_prompt",
            GlobalCacheStrategy::None => "none",
        }
    }
}

/// Non-nullable usage data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NonNullableUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

/// Query chain tracking info.
#[derive(Debug, Clone)]
pub struct QueryChainTracking {
    pub chain_id: String,
    pub depth: u32,
}

/// Extract error message from an error.
fn get_error_message(error: &ApiError) -> String {
    match error {
        ApiError::Api(e) => {
            if let Some(body) = e.body.get("error") {
                if let Some(msg) = body.get("message").and_then(|m| m.as_str()) {
                    return msg.to_string();
                }
            }
            e.message.clone()
        }
        ApiError::Connection(e) => e.message.clone(),
        ApiError::ConnectionTimeout(e) => e.message.clone(),
        ApiError::UserAbort(_) => "User aborted".to_string(),
        ApiError::Other(msg) => msg.clone(),
    }
}

/// Parameters for logging an API query event.
pub struct LogApiQueryParams {
    pub model: String,
    pub messages_length: usize,
    pub temperature: f64,
    pub betas: Option<Vec<String>>,
    pub permission_mode: Option<String>,
    pub query_source: String,
    pub query_tracking: Option<QueryChainTracking>,
    pub thinking_type: Option<String>,
    pub effort_value: Option<String>,
    pub fast_mode: Option<bool>,
    pub previous_request_id: Option<String>,
}

/// Log an API query event.
pub fn log_api_query(params: &LogApiQueryParams) {
    debug!(
        model = %params.model,
        messages_length = params.messages_length,
        temperature = params.temperature,
        query_source = %params.query_source,
        "tengu_api_query"
    );
}

/// Parameters for logging an API error event.
pub struct LogApiErrorParams {
    pub error: ApiError,
    pub model: String,
    pub message_count: usize,
    pub message_tokens: Option<u64>,
    pub duration_ms: u64,
    pub duration_ms_including_retries: u64,
    pub attempt: u32,
    pub request_id: Option<String>,
    pub client_request_id: Option<String>,
    pub did_fall_back_to_non_streaming: Option<bool>,
    pub prompt_category: Option<String>,
    pub headers: Option<HeaderMap>,
    pub query_tracking: Option<QueryChainTracking>,
    pub query_source: Option<String>,
    pub fast_mode: Option<bool>,
    pub previous_request_id: Option<String>,
    pub base_url: Option<String>,
}

/// Log an API error event.
pub fn log_api_error(params: &LogApiErrorParams) {
    let gateway = detect_gateway(
        params.headers.as_ref(),
        params.base_url.as_deref(),
    );

    let err_str = get_error_message(&params.error);
    let status = match &params.error {
        ApiError::Api(e) => Some(e.status.to_string()),
        _ => None,
    };
    let error_type = match &params.error {
        ApiError::Api(e) => classify_api_error(e),
        _ => "unknown",
    };

    // Log detailed connection error info
    let connection_details = match &params.error {
        ApiError::Api(e) => extract_connection_error_details(e),
        _ => None,
    };
    if let Some(details) = connection_details {
        let ssl_label = if details.is_ssl_error { " (SSL error)" } else { "" };
        debug!(
            "Connection error details: code={}{}, message={}",
            details.code, ssl_label, details.message
        );
    }

    if let Some(ref client_request_id) = params.client_request_id {
        debug!(
            "API error x-client-request-id={} (give this to the API team for server-log lookup)",
            client_request_id
        );
    }

    error!(
        model = %params.model,
        error = %err_str,
        status = ?status,
        error_type = %error_type,
        message_count = params.message_count,
        duration_ms = params.duration_ms,
        attempt = params.attempt,
        gateway = ?gateway.as_ref().map(|g| g.as_str()),
        "tengu_api_error"
    );
}

/// Parameters for logging an API success event.
pub struct LogApiSuccessParams {
    pub model: String,
    pub pre_normalized_model: String,
    pub message_count: usize,
    pub message_tokens: u64,
    pub usage: NonNullableUsage,
    pub duration_ms: u64,
    pub duration_ms_including_retries: u64,
    pub attempt: u32,
    pub ttft_ms: Option<u64>,
    pub request_id: Option<String>,
    pub stop_reason: Option<String>,
    pub canonical_stop_reason: Option<String>,
    pub cost_usd: f64,
    pub did_fall_back_to_non_streaming: bool,
    pub query_source: String,
    pub gateway: Option<KnownGateway>,
    pub query_tracking: Option<QueryChainTracking>,
    pub permission_mode: Option<String>,
    pub global_cache_strategy: Option<GlobalCacheStrategy>,
    pub text_content_length: Option<usize>,
    pub thinking_content_length: Option<usize>,
    pub tool_use_content_lengths: Option<HashMap<String, usize>>,
    pub connector_text_block_count: Option<usize>,
    pub fast_mode: Option<bool>,
    pub previous_request_id: Option<String>,
    pub betas: Option<Vec<String>>,
}

/// Log an API success event.
fn log_api_success(params: &LogApiSuccessParams) {
    debug!(
        model = %params.model,
        input_tokens = params.usage.input_tokens,
        output_tokens = params.usage.output_tokens,
        cached_input_tokens = params.usage.cache_read_input_tokens,
        duration_ms = params.duration_ms,
        attempt = params.attempt,
        ttft_ms = ?params.ttft_ms,
        stop_reason = ?params.stop_reason,
        query_source = %params.query_source,
        "tengu_api_success"
    );
}

/// Parameters for the full success+duration logging.
pub struct LogApiSuccessAndDurationParams {
    pub model: String,
    pub pre_normalized_model: String,
    pub start: u64,
    pub start_including_retries: u64,
    pub ttft_ms: Option<u64>,
    pub usage: NonNullableUsage,
    pub attempt: u32,
    pub message_count: usize,
    pub message_tokens: u64,
    pub request_id: Option<String>,
    pub stop_reason: Option<String>,
    pub canonical_stop_reason: Option<String>,
    pub did_fall_back_to_non_streaming: bool,
    pub query_source: String,
    pub headers: Option<HeaderMap>,
    pub cost_usd: f64,
    pub query_tracking: Option<QueryChainTracking>,
    pub permission_mode: Option<String>,
    pub global_cache_strategy: Option<GlobalCacheStrategy>,
    pub fast_mode: Option<bool>,
    pub previous_request_id: Option<String>,
    pub betas: Option<Vec<String>>,
    pub base_url: Option<String>,
}

/// Log API success with duration tracking and content analysis.
pub fn log_api_success_and_duration(params: &LogApiSuccessAndDurationParams) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let duration_ms = now.saturating_sub(params.start);
    let duration_ms_including_retries = now.saturating_sub(params.start_including_retries);

    let gateway = detect_gateway(params.headers.as_ref(), params.base_url.as_deref());

    log_api_success(&LogApiSuccessParams {
        model: params.model.clone(),
        pre_normalized_model: params.pre_normalized_model.clone(),
        message_count: params.message_count,
        message_tokens: params.message_tokens,
        usage: params.usage.clone(),
        duration_ms,
        duration_ms_including_retries,
        attempt: params.attempt,
        ttft_ms: params.ttft_ms,
        request_id: params.request_id.clone(),
        stop_reason: params.stop_reason.clone(),
        canonical_stop_reason: params.canonical_stop_reason.clone(),
        cost_usd: params.cost_usd,
        did_fall_back_to_non_streaming: params.did_fall_back_to_non_streaming,
        query_source: params.query_source.clone(),
        gateway,
        query_tracking: params.query_tracking.clone(),
        permission_mode: params.permission_mode.clone(),
        global_cache_strategy: params.global_cache_strategy.clone(),
        text_content_length: None,
        thinking_content_length: None,
        tool_use_content_lengths: None,
        connector_text_block_count: None,
        fast_mode: params.fast_mode,
        previous_request_id: params.previous_request_id.clone(),
        betas: params.betas.clone(),
    });
}
