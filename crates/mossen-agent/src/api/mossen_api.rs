//! # Core API Client (Query Model)
//!
//! Translated from `services/api/mossen.ts` (3406 lines).
//! Provides the main query pipeline: model querying with streaming,
//! non-streaming fallback, retry logic, cache breakpoints, usage tracking,
//! and all auxiliary helpers.

use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::empty_usage::{CacheCreation, NonNullableUsage, ServerToolUse};
use super::sdk::{
    ApiError, MossenAPIConnectionError, MossenAPIConnectionTimeoutError, MossenAPIError,
    MossenAPIUserAbortError, MossenBetaMessage, MossenStreamHandle,
};
use super::with_retry::{CannotRetryError, RetryContext, ThinkingConfig};

use futures::StreamExt;
use reqwest::header::HeaderMap as ReqHeaderMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const MAX_NON_STREAMING_TOKENS: u64 = 64_000;
pub const API_MAX_MEDIA_PER_REQUEST: usize = 100;
const CAPPED_DEFAULT_MAX_TOKENS: u64 = 8_000;
const STREAM_IDLE_TIMEOUT_DEFAULT_MS: u64 = 90_000;
const STALL_THRESHOLD_MS: u64 = 30_000;
const CACHE_TTL_1HOUR_MS: u64 = 60 * 60 * 1000;

// Beta headers
const EFFORT_BETA_HEADER: &str = "effort-2025-04-01";
const FAST_MODE_BETA_HEADER: &str = "fast-mode-2025-04-01";
const AFK_MODE_BETA_HEADER: &str = "afk-mode-2025-04-01";
const CONTEXT_1M_BETA_HEADER: &str = "context-1m-2025-04-01";
const CONTEXT_MANAGEMENT_BETA_HEADER: &str = "context-management-2025-04-01";
const REDACT_THINKING_BETA_HEADER: &str = "redact-thinking-2025-04-01";
const STRUCTURED_OUTPUTS_BETA_HEADER: &str = "structured-outputs-2025-04-01";
const TASK_BUDGETS_BETA_HEADER: &str = "task-budgets-2026-03-13";
const PROMPT_CACHING_SCOPE_BETA_HEADER: &str = "prompt-caching-scope-2025-04-01";
const ADVISOR_BETA_HEADER: &str = "advisor-2025-04-01";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Query source identifier.
pub type QuerySource = String;

/// Cache scope for prompt caching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheScope {
    #[serde(rename = "global")]
    Global,
    #[serde(rename = "ephemeral")]
    Ephemeral,
}

/// Global cache strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalCacheStrategy {
    None,
    SystemPrompt,
}

/// Effort value can be a string level or a numeric override.
#[derive(Debug, Clone)]
pub enum EffortValue {
    Level(String),
    Numeric(f64),
}

/// Task budget parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBudget {
    pub total: u64,
    pub remaining: Option<u64>,
}

/// Query chain tracking.
#[derive(Debug, Clone, Default)]
pub struct QueryChainTracking {
    pub chain_id: Option<String>,
    pub turn_number: Option<u32>,
}

/// Agent definition placeholder.
#[derive(Debug, Clone)]
pub struct AgentDefinition {
    pub name: String,
    pub model: Option<String>,
}

/// Tool permission context.
#[derive(Debug, Clone)]
pub struct ToolPermissionContext {
    pub mode: String,
}

/// A single tool definition.
#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub is_mcp: bool,
    pub is_lsp: bool,
    pub defer_loading: bool,
}

/// Notification placeholder.
#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
}

/// Options for the main query pipeline.
pub struct Options {
    pub model: String,
    pub tool_choice: Option<Value>,
    pub is_non_interactive_session: bool,
    pub extra_tool_schemas: Vec<Value>,
    pub max_output_tokens_override: Option<u64>,
    pub fallback_model: Option<String>,
    pub query_source: QuerySource,
    pub agents: Vec<AgentDefinition>,
    pub allowed_agent_types: Option<Vec<String>>,
    pub has_append_system_prompt: bool,
    pub enable_prompt_caching: Option<bool>,
    pub skip_cache_write: bool,
    pub temperature_override: Option<f64>,
    pub effort_value: Option<EffortValue>,
    pub has_pending_mcp_servers: bool,
    pub query_tracking: Option<QueryChainTracking>,
    pub agent_id: Option<String>,
    pub output_format: Option<Value>,
    pub fast_mode: Option<bool>,
    pub advisor_model: Option<String>,
    pub task_budget: Option<TaskBudget>,
    pub fetch_override: Option<Value>,
    pub on_streaming_fallback: Option<Box<dyn Fn() + Send + Sync>>,
}

/// System prompt type alias.
pub type SystemPrompt = Vec<String>;

/// User message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub message: UserMessageContent,
    #[serde(default)]
    pub uuid: String,
    #[serde(default)]
    pub timestamp: String,
}

/// User message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessageContent {
    pub role: String,
    pub content: Value,
}

/// Assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub message: AssistantMessageContent,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub uuid: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub research: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisor_model: Option<String>,
}

/// Assistant message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessageContent {
    pub id: Option<String>,
    pub content: Value,
    pub model: Option<String>,
    pub role: String,
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<Value>,
}

/// System API error message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemAPIErrorMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub content: String,
    pub api_error: Option<String>,
    pub error: Option<String>,
}

/// Stream event yielded during streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub event: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
}

/// Items yielded from the query model generator.
#[derive(Debug, Clone)]
pub enum QueryModelOutput {
    StreamEvent(StreamEvent),
    AssistantMessage(AssistantMessage),
    SystemError(SystemAPIErrorMessage),
}

/// Observed stop state from the API.
#[derive(Debug, Clone)]
pub struct ObservedStopState {
    pub stop_reason: Option<String>,
    pub canonical_stop_reason: Option<String>,
}

/// Stop state side effects.
#[derive(Debug, Clone)]
pub struct StopStateSideEffects {
    pub is_max_tokens: bool,
    pub is_context_window_exceeded: bool,
}

/// Model max output tokens info.
struct ModelMaxOutputTokens {
    default: u64,
    upper_limit: u64,
}

// ---------------------------------------------------------------------------
// Internal Helpers (mirror TS module boundaries)
// ---------------------------------------------------------------------------

fn is_env_truthy(var_name: &str) -> bool {
    env::var(var_name)
        .ok()
        .map(|v| {
            let lower = v.to_lowercase();
            lower == "1" || lower == "true" || lower == "yes"
        })
        .unwrap_or(false)
}

fn get_api_provider() -> String {
    env::var("API_PROVIDER").unwrap_or_else(|_| "firstParty".to_string())
}

fn get_user_type() -> String {
    env::var("MOSSEN_USER_TYPE").unwrap_or_else(|_| "user".to_string())
}

fn is_hosted_subscriber() -> bool {
    is_env_truthy("MOSSEN_HOSTED_SUBSCRIBER")
}

fn get_small_fast_model() -> String {
    env::var("MOSSEN_SMALL_FAST_MODEL").unwrap_or_else(|_| "mossen-3-5-fast-latest".to_string())
}

fn get_default_balanced_model() -> String {
    env::var("MOSSEN_DEFAULT_BALANCED_MODEL")
        .unwrap_or_else(|_| "mossen-balanced-4-20250514".to_string())
}

fn get_default_max_model() -> String {
    env::var("MOSSEN_DEFAULT_MAX_MODEL").unwrap_or_else(|_| "mossen-max-4-20250514".to_string())
}

fn get_or_create_user_id() -> String {
    env::var("MOSSEN_USER_ID").unwrap_or_else(|_| Uuid::new_v4().to_string())
}

fn get_session_id() -> String {
    env::var("MOSSEN_SESSION_ID").unwrap_or_else(|_| Uuid::new_v4().to_string())
}

fn get_oauth_account_uuid() -> Option<String> {
    env::var("MOSSEN_OAUTH_ACCOUNT_UUID").ok()
}

fn get_prompt_cache_1h_allowlist() -> Vec<String> {
    env::var("MOSSEN_PROMPT_CACHE_1H_ALLOWLIST")
        .ok()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default()
}

fn model_supports_effort(model: &str) -> bool {
    // Models that support effort parameter
    model.contains("mossen-balanced-4")
        || model.contains("mossen-max-4")
        || model.contains("mossen-3-5-balanced")
}

fn should_include_first_party_only_betas() -> bool {
    get_api_provider() == "firstParty"
}

fn is_max_tokens_cap_enabled() -> bool {
    is_env_truthy("MOSSEN_MAX_TOKENS_CAP")
}

fn get_model_max_output_tokens(model: &str) -> ModelMaxOutputTokens {
    // Default output token limits by model family
    if model.contains("max") {
        ModelMaxOutputTokens {
            default: 16_000,
            upper_limit: 64_000,
        }
    } else if model.contains("balanced") {
        ModelMaxOutputTokens {
            default: 16_000,
            upper_limit: 64_000,
        }
    } else if model.contains("fast") {
        ModelMaxOutputTokens {
            default: 8_000,
            upper_limit: 16_000,
        }
    } else {
        ModelMaxOutputTokens {
            default: 16_000,
            upper_limit: 64_000,
        }
    }
}

fn normalize_model_string_for_api(model: &str) -> String {
    model.to_string()
}

/// Default provider protocol header value.
///
/// Mirrors the version the JS SDK sends.
const DEFAULT_PROVIDER_VERSION: &str = "2023-06-01";

/// Default Mossen API base URL. Used when no MOSSEN_CODE_API_BASE_URL or
/// custom backend URL is configured.
const DEFAULT_PROVIDER_BASE_URL: &str = "https://api.mossen.ai";

/// Decide the effective base URL for the API request.
///
/// Priority (mirrors `services/api/client.ts` `getMossenClient` selection):
/// 1. Custom backend (`MOSSEN_CODE_CUSTOM_BASE_URL`) when enabled.
/// 2. `MOSSEN_CODE_API_BASE_URL` (explicit hosted adapter).
/// 3. Provider SDK base URL env convention.
/// 4. Default Provider public endpoint.
fn resolve_base_url() -> String {
    if mossen_utils::custom_backend::is_custom_backend_enabled() {
        if let Some(url) = mossen_utils::custom_backend::get_custom_backend_base_url() {
            return url.trim_end_matches('/').to_string();
        }
    }
    if let Ok(url) = env::var("MOSSEN_CODE_API_BASE_URL") {
        if !url.is_empty() {
            return url.trim_end_matches('/').to_string();
        }
    }
    DEFAULT_PROVIDER_BASE_URL.to_string()
}

/// Decide which headers to apply for auth.
///
/// Mirrors `services/api/client.ts` `getMossenClient` and `configureApiKeyHeaders`:
/// - Custom backend: delegate to `mossen_utils::custom_backend` so auth header
///   style follows the selected protocol and explicit user headers keep
///   precedence.
/// - Hosted/first-party: send `Authorization: Bearer <MOSSEN_CODE_AUTH_TOKEN>`
///   if present (OAuth path), else `x-api-key: <MOSSEN_CODE_API_KEY>`.
fn apply_auth_headers(headers: &mut ReqHeaderMap) {
    use reqwest::header::{HeaderName, HeaderValue};

    let insert_header = |headers: &mut ReqHeaderMap, name: &str, value: String| {
        if let (Ok(h), Ok(v)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            headers.insert(h, v);
        }
    };

    if mossen_utils::custom_backend::is_custom_backend_enabled() {
        for (k, v) in mossen_utils::custom_backend::get_custom_backend_auth_headers() {
            insert_header(headers, &k, v);
        }
        return;
    }

    // Hosted/first-party path.
    if let Ok(token) = env::var("MOSSEN_CODE_AUTH_TOKEN") {
        if !token.is_empty() {
            insert_header(headers, "Authorization", format!("Bearer {}", token));
            return;
        }
    }
    // Keep the provider SDK env fallback without exposing retired branding.
    let api_key = env::var("MOSSEN_CODE_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(mossen_utils::auth::get_mossen_api_key);
    if let Some(key) = api_key {
        insert_header(headers, "x-api-key", key);
    }
}

/// Build the request headers shared by streaming and non-streaming calls.
fn build_request_headers(betas: Option<&[String]>) -> ReqHeaderMap {
    use reqwest::header::{HeaderName, HeaderValue};
    let mut headers = ReqHeaderMap::new();

    let version = env::var("MOSSEN_CODE_API_VERSION")
        .unwrap_or_else(|_| DEFAULT_PROVIDER_VERSION.to_string());
    if let Ok(v) = HeaderValue::from_str(&version) {
        headers.insert("mossen-version", v);
    }
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("application/json"),
    );
    headers.insert("x-app", HeaderValue::from_static("cli"));
    headers.insert(
        "X-Mossen-Code-Session-Id",
        HeaderValue::from_str(&get_session_id())
            .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
    );

    if let Some(betas) = betas {
        if !betas.is_empty() {
            if let Ok(v) = HeaderValue::from_str(&betas.join(",")) {
                headers.insert("mossen-beta", v);
            }
        }
    }

    // Pass-through additional headers from env (newline-separated `Name: Value`).
    if let Ok(extra) = env::var("MOSSEN_CODE_CUSTOM_HEADERS") {
        for line in extra.split(['\n', '\r']) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((name, value)) = line.split_once(':') {
                let name = name.trim();
                let value = value.trim();
                if !name.is_empty() {
                    if let (Ok(h), Ok(v)) = (
                        HeaderName::from_bytes(name.as_bytes()),
                        HeaderValue::from_str(value),
                    ) {
                        headers.insert(h, v);
                    }
                }
            }
        }
    }

    apply_auth_headers(&mut headers);
    headers
}

/// Map a reqwest error into the project's `ApiError` enum.
fn classify_reqwest_error(err: reqwest::Error) -> ApiError {
    if err.is_timeout() {
        return ApiError::ConnectionTimeout(MossenAPIConnectionTimeoutError::new(Some(
            err.to_string(),
        )));
    }
    if err.is_connect() || err.is_request() || err.is_body() || err.is_decode() {
        return ApiError::Connection(MossenAPIConnectionError::new(Some(err.to_string())));
    }
    ApiError::Other(err.to_string())
}

/// Aggregate SSE events from an Provider streaming response into a final
/// message JSON. Mirrors the SDK's `Stream.finalMessage()` accumulator.
///
/// We collect:
/// - `message_start.message` as the base message envelope.
/// - `content_block_start` to seed each block at its declared index.
/// - `content_block_delta` to append `text_delta`, `input_json_delta`,
///   `thinking_delta`, and `signature_delta` onto the corresponding block.
/// - `message_delta` to update `stop_reason`, `stop_sequence`, and `usage`.
/// - `error` event to surface a stream-level API error.
async fn collect_stream_to_final_message(
    response: reqwest::Response,
    request_id: Option<String>,
) -> Result<Value, ApiError> {
    use eventsource_stream::Eventsource;

    let mut stream = response.bytes_stream().eventsource();

    let mut message: Option<Value> = None;
    let mut blocks: Vec<Value> = Vec::new();
    let mut partial_json: HashMap<usize, String> = HashMap::new();

    while let Some(item) = stream.next().await {
        let event = match item {
            Ok(e) => e,
            Err(e) => {
                return Err(ApiError::Connection(MossenAPIConnectionError::new(Some(
                    format!("SSE read error: {}", e),
                ))));
            }
        };

        let data = event.data;
        if data.is_empty() || data == "[DONE]" {
            continue;
        }

        let payload: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = payload
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        match event_type.as_str() {
            "error" => {
                let err_body = payload.get("error").cloned().unwrap_or(payload.clone());
                let msg = err_body
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("streaming error")
                    .to_string();
                let mut headers = ReqHeaderMap::new();
                if let Some(rid) = &request_id {
                    if let Ok(v) = reqwest::header::HeaderValue::from_str(rid) {
                        headers.insert("request-id", v);
                    }
                }
                return Err(ApiError::Api(MossenAPIError::new(
                    400,
                    err_body,
                    Some(msg),
                    headers,
                )));
            }
            "message_start" => {
                if let Some(m) = payload.get("message") {
                    message = Some(m.clone());
                    if let Some(content) = m.get("content").and_then(|c| c.as_array()) {
                        blocks = content.clone();
                    } else {
                        blocks.clear();
                    }
                }
            }
            "content_block_start" => {
                let index = payload.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let block = payload.get("content_block").cloned().unwrap_or(json!({}));
                while blocks.len() <= index {
                    blocks.push(json!({}));
                }
                blocks[index] = block;
                partial_json.remove(&index);
            }
            "content_block_delta" => {
                let index = payload.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                while blocks.len() <= index {
                    blocks.push(json!({}));
                }
                let block = &mut blocks[index];
                let delta = match payload.get("delta") {
                    Some(d) => d,
                    None => continue,
                };
                let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match delta_type {
                    "text_delta" => {
                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                            let existing = block
                                .get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();
                            block["text"] = Value::String(existing + text);
                        }
                    }
                    "input_json_delta" => {
                        if let Some(p) = delta.get("partial_json").and_then(|t| t.as_str()) {
                            partial_json.entry(index).or_default().push_str(p);
                        }
                    }
                    "thinking_delta" => {
                        if let Some(text) = delta.get("thinking").and_then(|t| t.as_str()) {
                            let existing = block
                                .get("thinking")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();
                            block["thinking"] = Value::String(existing + text);
                        }
                    }
                    "signature_delta" => {
                        if let Some(sig) = delta.get("signature").and_then(|t| t.as_str()) {
                            block["signature"] = Value::String(sig.to_string());
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = payload.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if let Some(buf) = partial_json.remove(&index) {
                    if blocks.len() > index {
                        let block = &mut blocks[index];
                        let parsed: Value = serde_json::from_str(&buf).unwrap_or(json!({}));
                        block["input"] = parsed;
                    }
                }
            }
            "message_delta" => {
                if let Some(msg) = message.as_mut() {
                    if let Some(delta) = payload.get("delta") {
                        if let Some(stop_reason) = delta.get("stop_reason").and_then(|v| v.as_str())
                        {
                            msg["stop_reason"] = Value::String(stop_reason.to_string());
                        }
                        if let Some(stop_sequence) =
                            delta.get("stop_sequence").and_then(|v| v.as_str())
                        {
                            msg["stop_sequence"] = Value::String(stop_sequence.to_string());
                        }
                    }
                    if let Some(usage) = payload.get("usage") {
                        // Merge: overwrite/extend existing usage with the delta.
                        let merged = match msg.get("usage").cloned() {
                            Some(Value::Object(mut existing)) => {
                                if let Some(obj) = usage.as_object() {
                                    for (k, v) in obj {
                                        existing.insert(k.clone(), v.clone());
                                    }
                                }
                                Value::Object(existing)
                            }
                            _ => usage.clone(),
                        };
                        msg["usage"] = merged;
                    }
                }
            }
            "message_stop" => {
                // No-op; loop will exit when stream closes.
            }
            "ping" => {}
            _ => {
                // Unknown event types are ignored — matches SDK behaviour.
            }
        }
    }

    let mut final_msg = message.unwrap_or_else(|| {
        json!({
            "id": Uuid::new_v4().to_string(),
            "type": "message",
            "role": "assistant",
            "model": "",
            "content": [],
            "stop_reason": null,
            "stop_sequence": null,
            "usage": {},
        })
    });
    final_msg["content"] = Value::Array(blocks);
    Ok(final_msg)
}

/// Make an API request.
///
/// Translates `services/api/mossen.ts` `mossenClient.beta.messages.create` and
/// the wrapping `withErrorMapping` / `withRetry` plumbing into a direct HTTP
/// call. Supports both streaming (`params.stream === true`) and non-streaming
/// (the SDK's `.create({...stream:false})` form), returning a JSON `Value`
/// shaped like the SDK's `MossenBetaMessage`.
async fn make_api_request(params: &Value) -> Result<Value, ApiError> {
    let base_url = resolve_base_url();

    // Route custom backends by their configured protocol. This legacy API path
    // is still used by hooks, fast queries, and non-streaming fallbacks, so it
    // must mirror the dialogue streaming client instead of assuming every
    // custom backend is OpenAI chat-completions compatible.
    if mossen_utils::custom_backend::is_custom_backend_enabled() {
        match mossen_utils::custom_backend::get_custom_backend_protocol() {
            mossen_utils::custom_backend::CustomBackendProtocol::OpenaiCompatible => {
                return openai_compat_make_api_request(params, &base_url).await;
            }
            mossen_utils::custom_backend::CustomBackendProtocol::OpenaiResponses
            | mossen_utils::custom_backend::CustomBackendProtocol::Anthropic => {
                return custom_backend_streaming_make_api_request(params, &base_url).await;
            }
            mossen_utils::custom_backend::CustomBackendProtocol::MossenCompatible
            | mossen_utils::custom_backend::CustomBackendProtocol::Private => {}
        }
    }

    let url = format!("{}/v1/messages", base_url);

    // Extract betas → header; remove from body since Provider accepts them
    // either as a header or in the body. We mirror the SDK and send the
    // header.
    let mut body = params.clone();
    let betas: Option<Vec<String>> = body.get("betas").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    });
    if let Some(obj) = body.as_object_mut() {
        obj.remove("betas");
    }

    let stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let headers = build_request_headers(betas.as_deref());

    let timeout_ms = env::var("API_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(600_000);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(classify_reqwest_error)?;

    let request = client.post(&url).headers(headers).json(&body);

    let response = match request.send().await {
        Ok(r) => r,
        Err(e) => return Err(classify_reqwest_error(e)),
    };

    let status = response.status();
    let response_headers = response.headers().clone();
    let request_id = response_headers
        .get("request-id")
        .or_else(|| response_headers.get("x-request-id"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if !status.is_success() {
        let status_code = status.as_u16();
        let body_text = response.text().await.unwrap_or_default();
        let parsed: Value =
            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text.clone()));
        let message = parsed
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("API request failed with status {}", status_code));
        return Err(ApiError::Api(MossenAPIError::new(
            status_code,
            parsed,
            Some(message),
            response_headers,
        )));
    }

    if stream {
        return collect_stream_to_final_message(response, request_id).await;
    }

    let raw = response.text().await.map_err(classify_reqwest_error)?;
    let parsed: Value = serde_json::from_str(&raw).map_err(|e| {
        ApiError::Other(format!(
            "Failed to parse API response JSON: {} ({})",
            e, raw
        ))
    })?;

    // Some errors come back as 200 OK with `{"type":"error","error":{...}}` —
    // surface those as API errors so retry/categorisation handles them.
    if parsed.get("type").and_then(|t| t.as_str()) == Some("error") {
        let err_body = parsed.get("error").cloned().unwrap_or(parsed.clone());
        let msg = err_body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("API error")
            .to_string();
        return Err(ApiError::Api(MossenAPIError::new(
            400,
            err_body,
            Some(msg),
            response_headers,
        )));
    }

    Ok(parsed)
}

/// Parse a Value response into an AssistantMessage.
fn parse_assistant_message(
    response: Value,
    request_id: Option<String>,
    advisor_model: Option<String>,
) -> Result<AssistantMessage, ApiError> {
    let content = response.get("content").cloned().unwrap_or(json!([]));
    let model = response
        .get("model")
        .and_then(|m| m.as_str())
        .map(|s| s.to_string());
    let stop_reason = response
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());
    let id = response
        .get("id")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());
    let usage = response.get("usage").cloned();

    Ok(AssistantMessage {
        message: AssistantMessageContent {
            id,
            content,
            model,
            role: "assistant".to_string(),
            stop_reason,
            usage,
        },
        request_id,
        msg_type: "assistant".to_string(),
        uuid: Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        research: None,
        advisor_model,
    })
}

// ---------------------------------------------------------------------------
// Helper Functions
// ---------------------------------------------------------------------------

/// Parse extra body parameters from environment variable.
pub fn get_extra_body_params(beta_headers: Option<&[String]>) -> HashMap<String, Value> {
    let mut result: HashMap<String, Value> = HashMap::new();

    if let Ok(extra_body_str) = env::var("MOSSEN_CODE_EXTRA_BODY") {
        if !extra_body_str.is_empty() {
            match serde_json::from_str::<Value>(&extra_body_str) {
                Ok(Value::Object(map)) => {
                    for (k, v) in map {
                        result.insert(k, v);
                    }
                }
                Ok(_) => {
                    tracing::error!(
                        "MOSSEN_CODE_EXTRA_BODY env var must be a JSON object, but was given {}",
                        extra_body_str
                    );
                }
                Err(e) => {
                    tracing::error!("Error parsing MOSSEN_CODE_EXTRA_BODY: {}", e);
                }
            }
        }
    }

    // Handle beta headers if provided
    if let Some(headers) = beta_headers {
        if !headers.is_empty() {
            let existing = result
                .get("mossen_beta")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let existing_strs: HashSet<String> = existing
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();

            let mut merged: Vec<Value> = existing;
            for header in headers {
                if !existing_strs.contains(header.as_str()) {
                    merged.push(Value::String(header.clone()));
                }
            }
            result.insert("mossen_beta".to_string(), Value::Array(merged));
        }
    }

    result
}

/// Check if prompt caching is enabled for a given model.
pub fn get_prompt_caching_enabled(model: &str) -> bool {
    if is_env_truthy("DISABLE_PROMPT_CACHING") {
        return false;
    }

    if is_env_truthy("DISABLE_PROMPT_CACHING_FAST") {
        let small_fast = get_small_fast_model();
        if model == small_fast {
            return false;
        }
    }

    if is_env_truthy("DISABLE_PROMPT_CACHING_BALANCED") {
        let default_balanced = get_default_balanced_model();
        if model == default_balanced {
            return false;
        }
    }

    if is_env_truthy("DISABLE_PROMPT_CACHING_MAX") {
        let default_max = get_default_max_model();
        if model == default_max {
            return false;
        }
    }

    true
}

/// Get cache control parameters.
pub fn get_cache_control(scope: Option<&CacheScope>, query_source: Option<&str>) -> Value {
    let mut control = json!({ "type": "ephemeral" });

    if should_1h_cache_ttl(query_source) {
        control["ttl"] = json!("1h");
    }

    if scope == Some(&CacheScope::Global) {
        control["scope"] = json!("global");
    }

    control
}

/// Determine if 1h TTL should be used for prompt caching.
fn should_1h_cache_ttl(query_source: Option<&str>) -> bool {
    if get_api_provider() == "bedrock" && is_env_truthy("ENABLE_PROMPT_CACHING_1H_BEDROCK") {
        return true;
    }

    let user_type = get_user_type();
    let user_eligible = user_type == "internal" || is_hosted_subscriber();

    if !user_eligible {
        return false;
    }

    let allowlist = get_prompt_cache_1h_allowlist();
    match query_source {
        Some(qs) => allowlist.iter().any(|pattern| {
            if pattern.ends_with('*') {
                qs.starts_with(&pattern[..pattern.len() - 1])
            } else {
                qs == pattern
            }
        }),
        None => false,
    }
}

/// Configure effort parameters for API request.
pub fn configure_effort_params(
    effort_value: Option<&EffortValue>,
    output_config: &mut HashMap<String, Value>,
    extra_body_params: &mut HashMap<String, Value>,
    betas: &mut Vec<String>,
    model: &str,
) {
    if !model_supports_effort(model) || output_config.contains_key("effort") {
        return;
    }

    match effort_value {
        None => {
            betas.push(EFFORT_BETA_HEADER.to_string());
        }
        Some(EffortValue::Level(level)) => {
            output_config.insert("effort".to_string(), Value::String(level.clone()));
            betas.push(EFFORT_BETA_HEADER.to_string());
        }
        Some(EffortValue::Numeric(value)) => {
            if get_user_type() == "internal" {
                let existing = extra_body_params
                    .entry("mossen_internal".to_string())
                    .or_insert_with(|| json!({}));
                if let Value::Object(ref mut map) = existing {
                    map.insert(
                        "effort_override".to_string(),
                        serde_json::Number::from_f64(*value)
                            .map(Value::Number)
                            .unwrap_or(Value::Null),
                    );
                }
            }
        }
    }
}

/// Configure task budget parameters.
pub fn configure_task_budget_params(
    task_budget: Option<&TaskBudget>,
    output_config: &mut HashMap<String, Value>,
    betas: &mut Vec<String>,
) {
    let budget = match task_budget {
        Some(b) => b,
        None => return,
    };

    if output_config.contains_key("task_budget") || !should_include_first_party_only_betas() {
        return;
    }

    let mut param = json!({
        "type": "tokens",
        "total": budget.total,
    });

    if let Some(remaining) = budget.remaining {
        param["remaining"] = json!(remaining);
    }

    output_config.insert("task_budget".to_string(), param);

    if !betas.contains(&TASK_BUDGETS_BETA_HEADER.to_string()) {
        betas.push(TASK_BUDGETS_BETA_HEADER.to_string());
    }
}

/// Get API metadata for requests.
pub fn get_api_metadata() -> Value {
    let mut extra: HashMap<String, Value> = HashMap::new();

    if let Ok(extra_str) = env::var("MOSSEN_CODE_EXTRA_METADATA") {
        if !extra_str.is_empty() {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&extra_str) {
                for (k, v) in map {
                    extra.insert(k, v);
                }
            } else {
                tracing::error!(
                    "MOSSEN_CODE_EXTRA_METADATA env var must be a JSON object, but was given {}",
                    extra_str
                );
            }
        }
    }

    let device_id = get_or_create_user_id();
    let session_id = get_session_id();
    let account_uuid = get_oauth_account_uuid().unwrap_or_default();

    extra.insert("device_id".to_string(), json!(device_id));
    extra.insert("account_uuid".to_string(), json!(account_uuid));
    extra.insert("session_id".to_string(), json!(session_id));

    json!({
        "user_id": serde_json::to_string(&extra).unwrap_or_default()
    })
}

/// Verify an API key by making a minimal request.
pub async fn verify_api_key(
    api_key: &str,
    is_non_interactive_session: bool,
) -> Result<bool, ApiError> {
    if is_non_interactive_session {
        return Ok(true);
    }

    let model = get_small_fast_model();
    let params = json!({
        "model": model,
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "test"}],
        "temperature": 1,
    });

    match make_api_request(&params).await {
        Ok(_) => Ok(true),
        Err(ApiError::Api(ref e)) if e.message.contains("authentication_error") => Ok(false),
        Err(e) => Err(e),
    }
}

/// Convert a user message to API message param format.
pub fn user_message_to_message_param(
    message: &UserMessage,
    add_cache: bool,
    enable_prompt_caching: bool,
    query_source: Option<&str>,
) -> Value {
    let content = &message.message.content;

    if add_cache {
        if let Some(text) = content.as_str() {
            let mut block = json!({
                "type": "text",
                "text": text,
            });
            if enable_prompt_caching {
                block["cache_control"] = get_cache_control(None, query_source);
            }
            return json!({
                "role": "user",
                "content": [block],
            });
        } else if let Some(arr) = content.as_array() {
            let len = arr.len();
            let mapped: Vec<Value> = arr
                .iter()
                .enumerate()
                .map(|(i, block)| {
                    let mut b = block.clone();
                    if i == len - 1 && enable_prompt_caching {
                        b["cache_control"] = get_cache_control(None, query_source);
                    }
                    b
                })
                .collect();
            return json!({
                "role": "user",
                "content": mapped,
            });
        }
    }

    let cloned_content = if let Some(arr) = content.as_array() {
        Value::Array(arr.clone())
    } else {
        content.clone()
    };

    json!({
        "role": "user",
        "content": cloned_content,
    })
}

/// Convert an assistant message to API message param format.
pub fn assistant_message_to_message_param(
    message: &AssistantMessage,
    add_cache: bool,
    enable_prompt_caching: bool,
    query_source: Option<&str>,
) -> Value {
    let content = &message.message.content;

    if add_cache {
        if let Some(text) = content.as_str() {
            let mut block = json!({
                "type": "text",
                "text": text,
            });
            if enable_prompt_caching {
                block["cache_control"] = get_cache_control(None, query_source);
            }
            return json!({
                "role": "assistant",
                "content": [block],
            });
        } else if let Some(arr) = content.as_array() {
            let len = arr.len();
            let mapped: Vec<Value> = arr
                .iter()
                .enumerate()
                .map(|(i, block)| {
                    let mut b = block.clone();
                    let block_type = b.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if i == len - 1
                        && block_type != "thinking"
                        && block_type != "redacted_thinking"
                        && block_type != "connector_text"
                        && enable_prompt_caching
                    {
                        b["cache_control"] = get_cache_control(None, query_source);
                    }
                    b
                })
                .collect();
            return json!({
                "role": "assistant",
                "content": mapped,
            });
        }
    }

    json!({
        "role": "assistant",
        "content": content.clone(),
    })
}

/// Check if a content block is a media item (image or document).
fn is_media(block: &Value) -> bool {
    matches!(
        block.get("type").and_then(|t| t.as_str()),
        Some("image") | Some("document")
    )
}

/// Check if a block is an official tool result block.
fn is_official_tool_result_block(block: &Value) -> bool {
    block.get("type").and_then(|t| t.as_str()) == Some("tool_result")
}

/// Strip excess media items from messages, keeping the most recent.
pub fn strip_excess_media_items(messages: &[Value], limit: usize) -> Vec<Value> {
    let mut total_media: usize = 0;
    for msg in messages {
        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            for block in content {
                if is_media(block) {
                    total_media += 1;
                }
                if is_official_tool_result_block(block) {
                    if let Some(nested_content) = block.get("content").and_then(|c| c.as_array()) {
                        for nested in nested_content {
                            if is_media(nested) {
                                total_media += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    if total_media <= limit {
        return messages.to_vec();
    }

    let mut to_remove = total_media - limit;
    let mut result = Vec::with_capacity(messages.len());

    for msg in messages {
        if to_remove == 0 {
            result.push(msg.clone());
            continue;
        }

        let content = match msg.get("content").and_then(|c| c.as_array()) {
            Some(arr) => arr,
            None => {
                result.push(msg.clone());
                continue;
            }
        };

        let before = to_remove;
        let stripped: Vec<Value> = content
            .iter()
            .filter_map(|block| {
                if to_remove > 0 && is_official_tool_result_block(block) {
                    if let Some(nested_arr) = block.get("content").and_then(|c| c.as_array()) {
                        let filtered: Vec<Value> = nested_arr
                            .iter()
                            .filter(|n| {
                                if to_remove > 0 && is_media(n) {
                                    to_remove -= 1;
                                    false
                                } else {
                                    true
                                }
                            })
                            .cloned()
                            .collect();
                        if filtered.len() != nested_arr.len() {
                            let mut new_block = block.clone();
                            new_block["content"] = Value::Array(filtered);
                            return Some(new_block);
                        }
                    }
                }
                if to_remove > 0 && is_media(block) {
                    to_remove -= 1;
                    return None;
                }
                Some(block.clone())
            })
            .collect();

        if before == to_remove {
            result.push(msg.clone());
        } else {
            let mut new_msg = msg.clone();
            new_msg["content"] = Value::Array(stripped);
            result.push(new_msg);
        }
    }

    result
}

/// Get previous request ID from the most recent assistant message.
fn get_previous_request_id_from_messages(messages: &[Value]) -> Option<String> {
    for msg in messages.iter().rev() {
        if msg.get("type").and_then(|t| t.as_str()) == Some("assistant") {
            if let Some(request_id) = msg.get("requestId").and_then(|r| r.as_str()) {
                if !request_id.is_empty() {
                    return Some(request_id.to_string());
                }
            }
        }
    }
    None
}

/// Get the non-streaming fallback timeout in milliseconds.
fn get_nonstreaming_fallback_timeout_ms() -> u64 {
    if let Ok(val) = env::var("API_TIMEOUT_MS") {
        if let Ok(ms) = val.parse::<u64>() {
            if ms > 0 {
                return ms;
            }
        }
    }

    if is_env_truthy("MOSSEN_CODE_REMOTE") {
        120_000
    } else {
        300_000
    }
}

/// Update usage statistics with new values from streaming API events.
pub fn update_usage(usage: &NonNullableUsage, part_usage: Option<&Value>) -> NonNullableUsage {
    let part = match part_usage {
        Some(p) => p,
        None => return usage.clone(),
    };

    let input_tokens = part
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .filter(|&v| v > 0)
        .unwrap_or(usage.input_tokens);

    let cache_creation_input_tokens = part
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .filter(|&v| v > 0)
        .unwrap_or(usage.cache_creation_input_tokens);

    let cache_read_input_tokens = part
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .filter(|&v| v > 0)
        .unwrap_or(usage.cache_read_input_tokens);

    let output_tokens = part
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(usage.output_tokens);

    let web_search = part
        .get("server_tool_use")
        .and_then(|s| s.get("web_search_requests"))
        .and_then(|v| v.as_u64())
        .unwrap_or(usage.server_tool_use.web_search_requests);

    let web_fetch = part
        .get("server_tool_use")
        .and_then(|s| s.get("web_fetch_requests"))
        .and_then(|v| v.as_u64())
        .unwrap_or(usage.server_tool_use.web_fetch_requests);

    let ephemeral_1h = part
        .get("cache_creation")
        .and_then(|c| c.get("ephemeral_1h_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(usage.cache_creation.ephemeral_1h_input_tokens);

    let ephemeral_5m = part
        .get("cache_creation")
        .and_then(|c| c.get("ephemeral_5m_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(usage.cache_creation.ephemeral_5m_input_tokens);

    let speed = part
        .get("speed")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| usage.speed.clone());

    NonNullableUsage {
        input_tokens,
        cache_creation_input_tokens,
        cache_read_input_tokens,
        output_tokens,
        server_tool_use: ServerToolUse {
            web_search_requests: web_search,
            web_fetch_requests: web_fetch,
        },
        service_tier: usage.service_tier.clone(),
        cache_creation: CacheCreation {
            ephemeral_1h_input_tokens: ephemeral_1h,
            ephemeral_5m_input_tokens: ephemeral_5m,
        },
        inference_geo: usage.inference_geo.clone(),
        iterations: usage.iterations.clone(),
        speed,
    }
}

/// Accumulate usage from one message into a total usage object.
pub fn accumulate_usage(
    total_usage: &NonNullableUsage,
    message_usage: &NonNullableUsage,
) -> NonNullableUsage {
    NonNullableUsage {
        input_tokens: total_usage.input_tokens + message_usage.input_tokens,
        cache_creation_input_tokens: total_usage.cache_creation_input_tokens
            + message_usage.cache_creation_input_tokens,
        cache_read_input_tokens: total_usage.cache_read_input_tokens
            + message_usage.cache_read_input_tokens,
        output_tokens: total_usage.output_tokens + message_usage.output_tokens,
        server_tool_use: ServerToolUse {
            web_search_requests: total_usage.server_tool_use.web_search_requests
                + message_usage.server_tool_use.web_search_requests,
            web_fetch_requests: total_usage.server_tool_use.web_fetch_requests
                + message_usage.server_tool_use.web_fetch_requests,
        },
        service_tier: message_usage.service_tier.clone(),
        cache_creation: CacheCreation {
            ephemeral_1h_input_tokens: total_usage.cache_creation.ephemeral_1h_input_tokens
                + message_usage.cache_creation.ephemeral_1h_input_tokens,
            ephemeral_5m_input_tokens: total_usage.cache_creation.ephemeral_5m_input_tokens
                + message_usage.cache_creation.ephemeral_5m_input_tokens,
        },
        inference_geo: message_usage.inference_geo.clone(),
        iterations: message_usage.iterations.clone(),
        speed: message_usage.speed.clone(),
    }
}

/// Adjust parameters for non-streaming requests.
pub fn adjust_params_for_non_streaming(params: &mut Value, max_tokens_cap: u64) {
    let current_max = params
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(max_tokens_cap);

    let capped_max = current_max.min(max_tokens_cap);

    if let Some(thinking) = params.get_mut("thinking") {
        if thinking.get("type").and_then(|t| t.as_str()) == Some("enabled") {
            if let Some(budget) = thinking.get("budget_tokens").and_then(|b| b.as_u64()) {
                let adjusted = budget.min(capped_max.saturating_sub(1));
                thinking["budget_tokens"] = json!(adjusted);
            }
        }
    }

    params["max_tokens"] = json!(capped_max);
}

/// Get the maximum output tokens for a model.
pub fn get_max_output_tokens_for_model(model: &str) -> u64 {
    let max_output = get_model_max_output_tokens(model);
    let default_tokens = if is_max_tokens_cap_enabled() {
        max_output.default.min(CAPPED_DEFAULT_MAX_TOKENS)
    } else {
        max_output.default
    };

    if let Ok(val) = env::var("MOSSEN_CODE_MAX_OUTPUT_TOKENS") {
        if let Ok(tokens) = val.parse::<u64>() {
            if tokens > 0 && tokens <= max_output.upper_limit {
                return tokens;
            }
        }
    }

    default_tokens
}

/// Build system prompt blocks with cache control.
pub fn build_system_prompt_blocks(
    system_prompt: &SystemPrompt,
    enable_prompt_caching: bool,
    skip_global_cache: bool,
    query_source: Option<&str>,
) -> Vec<Value> {
    system_prompt
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let mut block = json!({
                "type": "text",
                "text": text,
            });

            if enable_prompt_caching && !skip_global_cache {
                block["cache_control"] = get_cache_control(
                    if i == 0 {
                        Some(&CacheScope::Global)
                    } else {
                        None
                    },
                    query_source,
                );
            }

            block
        })
        .collect()
}

/// Add cache breakpoints to messages.
pub fn add_cache_breakpoints(
    messages: &[Value],
    enable_prompt_caching: bool,
    query_source: Option<&str>,
    use_cached_mc: bool,
    skip_cache_write: bool,
) -> Vec<Value> {
    let marker_index = if skip_cache_write {
        messages.len().saturating_sub(2)
    } else {
        messages.len().saturating_sub(1)
    };

    let mut result: Vec<Value> = messages
        .iter()
        .enumerate()
        .map(|(index, msg)| {
            let add_cache = index == marker_index;

            if add_cache && enable_prompt_caching {
                let mut new_msg = msg.clone();
                if let Some(content) = new_msg.get_mut("content") {
                    if let Some(arr) = content.as_array_mut() {
                        if let Some(last) = arr.last_mut() {
                            last["cache_control"] = get_cache_control(None, query_source);
                        }
                    } else if let Some(text) = content.as_str() {
                        let text_owned = text.to_string();
                        let mut block = json!({
                            "type": "text",
                            "text": text_owned,
                        });
                        block["cache_control"] = get_cache_control(None, query_source);
                        *content = json!([block]);
                    }
                }
                new_msg
            } else {
                msg.clone()
            }
        })
        .collect();

    if use_cached_mc && enable_prompt_caching {
        let mut last_cc_msg: i64 = -1;
        for (i, msg) in result.iter().enumerate() {
            if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                for block in content {
                    if block.get("cache_control").is_some() {
                        last_cc_msg = i as i64;
                    }
                }
            }
        }

        if last_cc_msg >= 0 {
            for i in 0..(last_cc_msg as usize) {
                let msg = &mut result[i];
                if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
                    continue;
                }
                if let Some(content) = msg.get_mut("content").and_then(|c| c.as_array_mut()) {
                    for block in content.iter_mut() {
                        if is_official_tool_result_block(block) {
                            if let Some(tool_use_id) =
                                block.get("tool_use_id").and_then(|t| t.as_str())
                            {
                                block["cache_reference"] = Value::String(tool_use_id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

/// Observe the stop state from the API response.
pub fn observe_mossen_stop_state(stop_reason: Option<&str>) -> ObservedStopState {
    let canonical = stop_reason.map(|r| match r {
        "end_turn" => "end_turn".to_string(),
        "tool_use" => "tool_use".to_string(),
        "max_tokens" => "max_tokens".to_string(),
        "stop_sequence" => "stop_sequence".to_string(),
        other => other.to_string(),
    });

    ObservedStopState {
        stop_reason: stop_reason.map(|s| s.to_string()),
        canonical_stop_reason: canonical,
    }
}

/// Classify the observed stop state for side effects.
pub fn classify_observed_stop_state(state: &ObservedStopState) -> StopStateSideEffects {
    let is_max_tokens = state.stop_reason.as_deref() == Some("max_tokens");
    let is_context_window_exceeded =
        state.canonical_stop_reason.as_deref() == Some("context_window_exceeded");

    StopStateSideEffects {
        is_max_tokens,
        is_context_window_exceeded,
    }
}

/// Check if a stream terminated without observed stop state.
pub fn did_stream_terminate_without_observed_stop_state(
    has_partial_message: bool,
    new_messages_count: usize,
    observed_state: &ObservedStopState,
) -> bool {
    if !has_partial_message {
        return true;
    }
    if new_messages_count == 0 && observed_state.stop_reason.is_none() {
        return true;
    }
    false
}

/// Clean up stream resources.
pub fn cleanup_stream(_stream: Option<&mut MossenStreamHandle>) {
    // In Rust, dropping the handle cleans up resources automatically.
}

/// Query a specific model (Fast-class, non-streaming).
pub async fn query_fast(
    system_prompt: &SystemPrompt,
    user_prompt: &str,
    output_format: Option<&Value>,
    _cancel: &CancellationToken,
) -> Result<AssistantMessage, ApiError> {
    let model = get_small_fast_model();

    let messages = vec![json!({
        "role": "user",
        "content": user_prompt,
    })];

    let system = build_system_prompt_blocks(system_prompt, false, false, None);

    let mut params = json!({
        "model": model,
        "messages": messages,
        "system": system,
        "max_tokens": get_max_output_tokens_for_model(&model),
        "temperature": 1,
    });

    if let Some(fmt) = output_format {
        params["output_format"] = fmt.clone();
    }

    let response = make_api_request(&params).await?;
    parse_assistant_message(response, None, None)
}

/// Query a specific model with full options (non-streaming).
pub async fn query_with_model(
    system_prompt: &SystemPrompt,
    user_prompt: &str,
    model: &str,
    output_format: Option<&Value>,
    _cancel: &CancellationToken,
    enable_prompt_caching: bool,
) -> Result<AssistantMessage, ApiError> {
    let messages = vec![json!({
        "role": "user",
        "content": user_prompt,
    })];

    let system = build_system_prompt_blocks(system_prompt, enable_prompt_caching, false, None);

    let mut params = json!({
        "model": model,
        "messages": messages,
        "system": system,
        "max_tokens": get_max_output_tokens_for_model(model),
        "temperature": 1,
    });

    if let Some(fmt) = output_format {
        params["output_format"] = fmt.clone();
    }

    let response = make_api_request(&params).await?;
    parse_assistant_message(response, None, None)
}

/// Query model without streaming — consumes the full stream and returns
/// the final assistant message.
pub async fn query_model_without_streaming(
    messages: &[Value],
    system_prompt: &SystemPrompt,
    thinking_config: &ThinkingConfig,
    tools: &[Tool],
    cancel: &CancellationToken,
    options: &Options,
) -> Result<AssistantMessage, ApiError> {
    let mut assistant_message: Option<AssistantMessage> = None;

    let mut rx = query_model_stream(
        messages,
        system_prompt,
        thinking_config,
        tools,
        cancel,
        options,
    )
    .await?;

    while let Some(output) = rx.recv().await {
        match output {
            QueryModelOutput::AssistantMessage(msg) => {
                assistant_message = Some(msg);
            }
            QueryModelOutput::SystemError(_) => {}
            QueryModelOutput::StreamEvent(_) => {}
        }
    }

    match assistant_message {
        Some(msg) => Ok(msg),
        None => {
            if cancel.is_cancelled() {
                Err(ApiError::UserAbort(MossenAPIUserAbortError))
            } else {
                Err(ApiError::Other("No assistant message found".to_string()))
            }
        }
    }
}

/// Query model with streaming — returns a channel receiver of outputs.
pub async fn query_model_with_streaming(
    messages: &[Value],
    system_prompt: &SystemPrompt,
    thinking_config: &ThinkingConfig,
    tools: &[Tool],
    cancel: &CancellationToken,
    options: &Options,
) -> Result<mpsc::Receiver<QueryModelOutput>, ApiError> {
    query_model_stream(
        messages,
        system_prompt,
        thinking_config,
        tools,
        cancel,
        options,
    )
    .await
}

/// Internal: launch the query model pipeline and return a channel of outputs.
async fn query_model_stream(
    messages: &[Value],
    system_prompt: &SystemPrompt,
    thinking_config: &ThinkingConfig,
    tools: &[Tool],
    cancel: &CancellationToken,
    options: &Options,
) -> Result<mpsc::Receiver<QueryModelOutput>, ApiError> {
    let (tx, rx) = mpsc::channel(64);

    let messages = messages.to_vec();
    let system_prompt = system_prompt.clone();
    let thinking_config = thinking_config.clone();
    let tools = tools.to_vec();
    let cancel = cancel.clone();
    let model = options.model.clone();
    let query_source = options.query_source.clone();
    let fallback_model = options.fallback_model.clone();
    let max_output_override = options.max_output_tokens_override;
    let enable_prompt_caching = options.enable_prompt_caching;
    let skip_cache_write = options.skip_cache_write;
    let temperature_override = options.temperature_override;
    let effort_value = options.effort_value.clone();
    let task_budget = options.task_budget.clone();
    let advisor_model = options.advisor_model.clone();
    let fast_mode = options.fast_mode;
    let agent_id = options.agent_id.clone();
    let output_format = options.output_format.clone();

    tokio::spawn(async move {
        let result = run_query_model(
            &messages,
            &system_prompt,
            &thinking_config,
            &tools,
            &cancel,
            &model,
            &query_source,
            fallback_model.as_deref(),
            max_output_override,
            enable_prompt_caching,
            skip_cache_write,
            temperature_override,
            effort_value.as_ref(),
            task_budget.as_ref(),
            advisor_model.as_deref(),
            fast_mode,
            agent_id.as_deref(),
            output_format.as_ref(),
            &tx,
        )
        .await;

        if let Err(e) = result {
            let error_msg = SystemAPIErrorMessage {
                msg_type: "system".to_string(),
                content: format!("API error: {}", e),
                api_error: Some(format!("{:?}", e)),
                error: Some(e.to_string()),
            };
            let _ = tx.send(QueryModelOutput::SystemError(error_msg)).await;
        }
    });

    Ok(rx)
}

/// Core query model implementation with streaming, retry, and fallback.
#[allow(clippy::too_many_arguments)]
async fn run_query_model(
    messages: &[Value],
    system_prompt: &SystemPrompt,
    thinking_config: &ThinkingConfig,
    tools: &[Tool],
    cancel: &CancellationToken,
    model: &str,
    query_source: &str,
    fallback_model: Option<&str>,
    max_output_override: Option<u64>,
    enable_prompt_caching: Option<bool>,
    skip_cache_write: bool,
    temperature_override: Option<f64>,
    effort_value: Option<&EffortValue>,
    task_budget: Option<&TaskBudget>,
    advisor_model: Option<&str>,
    fast_mode: Option<bool>,
    agent_id: Option<&str>,
    output_format: Option<&Value>,
    tx: &mpsc::Sender<QueryModelOutput>,
) -> Result<(), ApiError> {
    let previous_request_id = get_previous_request_id_from_messages(messages);
    let resolved_model = model.to_string();

    // Build betas list
    let mut betas: Vec<String> = Vec::new();
    let is_agentic_query = query_source.starts_with("repl_main_thread")
        || query_source.starts_with("agent:")
        || query_source == "sdk"
        || query_source == "hook_agent"
        || query_source == "verification_agent";

    // Determine prompt caching
    let caching_enabled =
        enable_prompt_caching.unwrap_or_else(|| get_prompt_caching_enabled(model));

    // Build system prompt blocks
    let system =
        build_system_prompt_blocks(system_prompt, caching_enabled, false, Some(query_source));

    // Determine max output tokens
    let max_output_tokens =
        max_output_override.unwrap_or_else(|| get_max_output_tokens_for_model(model));

    // Build thinking config for params
    let has_thinking = !matches!(thinking_config, ThinkingConfig::Disabled)
        && !is_env_truthy("MOSSEN_CODE_DISABLE_THINKING");

    let thinking_param = if has_thinking {
        match thinking_config {
            ThinkingConfig::Adaptive => Some(json!({"type": "adaptive"})),
            ThinkingConfig::Enabled { budget_tokens } => {
                let budget = (*budget_tokens).min(max_output_tokens.saturating_sub(1));
                Some(json!({"budget_tokens": budget, "type": "enabled"}))
            }
            ThinkingConfig::Disabled => None,
        }
    } else {
        None
    };

    // Temperature: only send when thinking is disabled
    let temperature = if !has_thinking {
        Some(temperature_override.unwrap_or(1.0))
    } else {
        None
    };

    // Build extra body params
    let mut extra_body = get_extra_body_params(None);
    let mut output_config: HashMap<String, Value> = HashMap::new();

    configure_effort_params(
        effort_value,
        &mut output_config,
        &mut extra_body,
        &mut betas,
        model,
    );
    configure_task_budget_params(task_budget, &mut output_config, &mut betas);

    if let Some(fmt) = output_format {
        if !output_config.contains_key("format") {
            output_config.insert("format".to_string(), fmt.clone());
            if !betas.contains(&STRUCTURED_OUTPUTS_BETA_HEADER.to_string()) {
                betas.push(STRUCTURED_OUTPUTS_BETA_HEADER.to_string());
            }
        }
    }

    // Build messages for API
    let messages_for_api = add_cache_breakpoints(
        messages,
        caching_enabled,
        Some(query_source),
        false,
        skip_cache_write,
    );

    // Strip excess media
    let messages_for_api = strip_excess_media_items(&messages_for_api, API_MAX_MEDIA_PER_REQUEST);

    // Build full request params
    let mut params = json!({
        "model": normalize_model_string_for_api(model),
        "messages": messages_for_api,
        "system": system,
        "max_tokens": max_output_tokens,
        "metadata": get_api_metadata(),
        "stream": true,
    });

    if let Some(thinking) = &thinking_param {
        params["thinking"] = thinking.clone();
    }

    if let Some(temp) = temperature {
        params["temperature"] = json!(temp);
    }

    if !betas.is_empty() {
        params["betas"] = json!(betas);
    }

    if !output_config.is_empty() {
        params["output_config"] = serde_json::to_value(&output_config).unwrap_or(json!({}));
    }

    // Merge extra body params
    if let Value::Object(ref mut params_map) = params {
        for (k, v) in &extra_body {
            if !params_map.contains_key(k) {
                params_map.insert(k.clone(), v.clone());
            }
        }
    }

    let start = Instant::now();
    let start_including_retries = start;
    let mut attempt_number = 0u32;
    let mut stream_request_id: Option<String> = None;

    // Attempt streaming request
    match make_api_request(&params).await {
        Ok(response) => {
            // Process the streaming response events
            let msg = parse_assistant_message(
                response,
                stream_request_id.clone(),
                advisor_model.map(|s| s.to_string()),
            )?;

            let _ = tx.send(QueryModelOutput::AssistantMessage(msg)).await;
        }
        Err(ApiError::UserAbort(_)) => {
            if cancel.is_cancelled() {
                return Err(ApiError::UserAbort(MossenAPIUserAbortError));
            } else {
                // SDK timeout - convert to connection timeout
                return Err(ApiError::ConnectionTimeout(
                    MossenAPIConnectionTimeoutError::new(Some("Request timed out".to_string())),
                ));
            }
        }
        Err(e) => {
            // Attempt non-streaming fallback
            let disable_fallback = is_env_truthy("MOSSEN_CODE_DISABLE_NONSTREAMING_FALLBACK");

            if disable_fallback {
                tracing::error!("Streaming error (fallback disabled): {:?}", e);
                return Err(e);
            }

            tracing::warn!("Streaming error, falling back to non-streaming: {:?}", e);

            // Build non-streaming params
            let mut ns_params = params.clone();
            ns_params["stream"] = json!(false);
            adjust_params_for_non_streaming(&mut ns_params, MAX_NON_STREAMING_TOKENS);

            let fallback_timeout_ms = get_nonstreaming_fallback_timeout_ms();

            match tokio::time::timeout(
                Duration::from_millis(fallback_timeout_ms),
                make_api_request(&ns_params),
            )
            .await
            {
                Ok(Ok(response)) => {
                    let msg = parse_assistant_message(
                        response,
                        stream_request_id.clone(),
                        advisor_model.map(|s| s.to_string()),
                    )?;

                    let _ = tx.send(QueryModelOutput::AssistantMessage(msg)).await;
                }
                Ok(Err(fallback_err)) => {
                    return Err(fallback_err);
                }
                Err(_timeout) => {
                    return Err(ApiError::ConnectionTimeout(
                        MossenAPIConnectionTimeoutError::new(Some(
                            "Non-streaming fallback timed out".to_string(),
                        )),
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Execute a non-streaming request with retry logic.
pub async fn execute_non_streaming_request(
    model: &str,
    query_source: &str,
    params: &Value,
    cancel: &CancellationToken,
    fallback_model: Option<&str>,
    thinking_config: &ThinkingConfig,
    fast_mode: Option<bool>,
    initial_consecutive_529_errors: Option<u32>,
) -> Result<(MossenBetaMessage, Vec<SystemAPIErrorMessage>), ApiError> {
    let fallback_timeout_ms = get_nonstreaming_fallback_timeout_ms();
    let mut system_messages: Vec<SystemAPIErrorMessage> = Vec::new();

    let mut adjusted_params = params.clone();
    adjust_params_for_non_streaming(&mut adjusted_params, MAX_NON_STREAMING_TOKENS);
    adjusted_params["stream"] = json!(false);

    let response = tokio::time::timeout(
        Duration::from_millis(fallback_timeout_ms),
        make_api_request(&adjusted_params),
    )
    .await
    .map_err(|_| {
        ApiError::ConnectionTimeout(MossenAPIConnectionTimeoutError::new(Some(
            "Request timed out".to_string(),
        )))
    })?
    .map_err(|e| {
        if let ApiError::UserAbort(_) = &e {
            return e;
        }
        tracing::error!("Non-streaming fallback error");
        e
    })?;

    let message: MossenBetaMessage = serde_json::from_value(response)
        .map_err(|e| ApiError::Other(format!("Failed to parse API response: {}", e)))?;

    Ok((message, system_messages))
}

fn params_to_custom_backend_stream_request(params: &Value) -> crate::types::StreamRequestParams {
    use crate::types::{ApiMetadata, StreamRequestParams};

    let model = mossen_utils::custom_backend::get_custom_backend_model()
        .or_else(|| {
            params
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "custom-backend".to_string());
    let max_tokens = params
        .get("max_tokens")
        .or_else(|| params.get("max_output_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(CAPPED_DEFAULT_MAX_TOKENS)
        .min(u32::MAX as u64) as u32;
    let messages = params
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter_map(message_param_from_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let system = system_blocks_from_value(params.get("system"));
    let tools = params
        .get("tools")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    let thinking = thinking_config_from_value(params.get("thinking"));
    let tool_choice = params
        .get("tool_choice")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok());
    let metadata = params
        .get("metadata")
        .cloned()
        .and_then(|value| serde_json::from_value::<ApiMetadata>(value).ok())
        .unwrap_or(ApiMetadata { user_id: None });
    let effort = effort_level_from_params(params);

    StreamRequestParams {
        model,
        max_tokens,
        messages,
        system,
        tools,
        thinking,
        tool_choice,
        stream: true,
        metadata,
        extra_body: protocol_extra_body_from_params(params),
        effort,
    }
}

fn message_param_from_value(value: &Value) -> Option<crate::types::MessageParam> {
    let object = value.as_object()?;
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();
    let content = content_blocks_from_value(object.get("content"))?;
    Some(crate::types::MessageParam { role, content })
}

fn content_blocks_from_value(value: Option<&Value>) -> Option<Vec<mossen_types::ContentBlock>> {
    let value = value?;
    match value {
        Value::String(text) => Some(vec![mossen_types::ContentBlock::Text(
            mossen_types::TextBlock { text: text.clone() },
        )]),
        Value::Array(items) => {
            if let Ok(blocks) = serde_json::from_value::<Vec<mossen_types::ContentBlock>>(
                Value::Array(items.clone()),
            ) {
                return Some(blocks);
            }
            let blocks = items
                .iter()
                .filter_map(content_block_from_value)
                .collect::<Vec<_>>();
            Some(blocks)
        }
        Value::Object(_) => content_block_from_value(value).map(|block| vec![block]),
        _ => Some(Vec::new()),
    }
}

fn content_block_from_value(value: &Value) -> Option<mossen_types::ContentBlock> {
    if let Ok(block) = serde_json::from_value::<mossen_types::ContentBlock>(value.clone()) {
        return Some(block);
    }
    value
        .get("text")
        .and_then(Value::as_str)
        .map(|text| {
            mossen_types::ContentBlock::Text(mossen_types::TextBlock {
                text: text.to_string(),
            })
        })
        .or_else(|| {
            value.as_str().map(|text| {
                mossen_types::ContentBlock::Text(mossen_types::TextBlock {
                    text: text.to_string(),
                })
            })
        })
}

fn system_blocks_from_value(value: Option<&Value>) -> Vec<crate::types::SystemBlock> {
    let Some(value) = value else {
        return Vec::new();
    };
    match value {
        Value::String(text) => vec![crate::types::SystemBlock {
            text: text.clone(),
            cache_control: None,
        }],
        Value::Array(items) => items
            .iter()
            .filter_map(system_block_from_value)
            .collect::<Vec<_>>(),
        Value::Object(_) => system_block_from_value(value).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn system_block_from_value(value: &Value) -> Option<crate::types::SystemBlock> {
    if let Ok(block) = serde_json::from_value::<crate::types::SystemBlock>(value.clone()) {
        return Some(block);
    }
    value
        .get("text")
        .and_then(Value::as_str)
        .map(|text| crate::types::SystemBlock {
            text: text.to_string(),
            cache_control: None,
        })
        .or_else(|| {
            value.as_str().map(|text| crate::types::SystemBlock {
                text: text.to_string(),
                cache_control: None,
            })
        })
}

fn thinking_config_from_value(value: Option<&Value>) -> Option<crate::types::ThinkingConfig> {
    let value = value?;
    if let Ok(thinking) = serde_json::from_value::<crate::types::ThinkingConfig>(value.clone()) {
        return Some(thinking);
    }
    let object = value.as_object()?;
    match object.get("type").and_then(Value::as_str) {
        Some("enabled") => Some(crate::types::ThinkingConfig {
            enabled: true,
            budget_tokens: object
                .get("budget_tokens")
                .and_then(Value::as_u64)
                .map(|value| value.min(u32::MAX as u64) as u32),
        }),
        _ => None,
    }
}

fn effort_level_from_params(params: &Value) -> Option<crate::types::EffortLevel> {
    let raw = params
        .get("effort")
        .or_else(|| params.pointer("/output_config/effort"))
        .and_then(Value::as_str)?;
    match raw {
        "low" => Some(crate::types::EffortLevel::Low),
        "medium" => Some(crate::types::EffortLevel::Medium),
        "high" => Some(crate::types::EffortLevel::High),
        "max" => Some(crate::types::EffortLevel::Max),
        _ => None,
    }
}

fn protocol_extra_body_from_params(params: &Value) -> HashMap<String, Value> {
    const KNOWN_KEYS: &[&str] = &[
        "betas",
        "effort",
        "max_output_tokens",
        "max_tokens",
        "messages",
        "metadata",
        "model",
        "output_config",
        "stream",
        "system",
        "thinking",
        "tool_choice",
        "tools",
    ];
    let Some(object) = params.as_object() else {
        return HashMap::new();
    };
    object
        .iter()
        .filter(|(key, _)| !KNOWN_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

async fn custom_backend_streaming_make_api_request(
    params: &Value,
    base_url: &str,
) -> Result<Value, ApiError> {
    let stream_params = params_to_custom_backend_stream_request(params);
    let config = crate::api_client::ApiClientConfig::new(String::new(), Some(base_url.to_string()));
    let stream =
        crate::api_client::call_streaming(&config, &stream_params, CancellationToken::new())
            .await
            .map_err(map_api_client_error)?;

    collect_api_client_stream_to_final_message(stream).await
}

async fn collect_api_client_stream_to_final_message(
    mut stream: std::pin::Pin<
        Box<
            dyn futures::Stream<
                    Item = Result<crate::streaming::StreamEvent, crate::api_client::ApiError>,
                > + Send,
        >,
    >,
) -> Result<Value, ApiError> {
    use crate::streaming::{ContentBlockInfo, StreamEvent as ApiClientStreamEvent};
    use crate::types::ContentDelta;

    let mut message: Option<Value> = None;
    let mut blocks: Vec<Value> = Vec::new();
    let mut partial_json: HashMap<usize, String> = HashMap::new();

    while let Some(item) = stream.next().await {
        match item.map_err(map_api_client_error)? {
            ApiClientStreamEvent::MessageStart { message: start } => {
                message = Some(json!({
                    "id": start.id,
                    "type": start.message_type,
                    "role": start.role,
                    "model": start.model,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": start.usage.unwrap_or_default(),
                }));
                blocks.clear();
            }
            ApiClientStreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                while blocks.len() <= index {
                    blocks.push(json!({}));
                }
                blocks[index] = match content_block {
                    ContentBlockInfo::Text { text } => json!({
                        "type": "text",
                        "text": text,
                    }),
                    ContentBlockInfo::ToolUse { id, name } => json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": {},
                    }),
                    ContentBlockInfo::Thinking { thinking } => json!({
                        "type": "thinking",
                        "thinking": thinking,
                    }),
                };
                partial_json.remove(&index);
            }
            ApiClientStreamEvent::ContentBlockDelta { index, delta } => {
                while blocks.len() <= index {
                    blocks.push(json!({}));
                }
                let block = &mut blocks[index];
                match delta {
                    ContentDelta::TextDelta { text } => {
                        let existing = block
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        block["text"] = Value::String(existing + &text);
                    }
                    ContentDelta::ThinkingDelta { thinking } => {
                        let existing = block
                            .get("thinking")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        block["thinking"] = Value::String(existing + &thinking);
                    }
                    ContentDelta::InputJsonDelta { partial_json: part } => {
                        partial_json.entry(index).or_default().push_str(&part);
                    }
                }
            }
            ApiClientStreamEvent::ContentBlockStop { index } => {
                if let Some(buf) = partial_json.remove(&index) {
                    if blocks.len() > index {
                        let parsed = serde_json::from_str::<Value>(&buf).unwrap_or(json!({}));
                        blocks[index]["input"] = parsed;
                    }
                }
            }
            ApiClientStreamEvent::MessageDelta { delta, usage } => {
                if let Some(msg) = message.as_mut() {
                    if let Some(stop_reason) = delta.stop_reason {
                        msg["stop_reason"] = Value::String(stop_reason);
                    }
                    if let Some(stop_sequence) = delta.stop_sequence {
                        msg["stop_sequence"] = Value::String(stop_sequence);
                    }
                    if let Some(usage) = usage {
                        msg["usage"] = serde_json::to_value(usage).unwrap_or_else(|_| json!({}));
                    }
                }
            }
            ApiClientStreamEvent::MessageStop | ApiClientStreamEvent::Ping => {}
            ApiClientStreamEvent::Error { error } => {
                let body = json!({
                    "type": error.error_type,
                    "message": error.message,
                });
                return Err(ApiError::Api(MossenAPIError::new(
                    400,
                    body,
                    Some(error.message),
                    ReqHeaderMap::new(),
                )));
            }
        }
    }

    let mut final_msg = message.unwrap_or_else(|| {
        json!({
            "id": Uuid::new_v4().to_string(),
            "type": "message",
            "role": "assistant",
            "model": "",
            "content": [],
            "stop_reason": null,
            "stop_sequence": null,
            "usage": {},
        })
    });
    final_msg["content"] = Value::Array(blocks);
    Ok(final_msg)
}

fn map_api_client_error(error: crate::api_client::ApiError) -> ApiError {
    match error {
        crate::api_client::ApiError::Auth { status, message }
        | crate::api_client::ApiError::Http { status, message } => {
            ApiError::Api(MossenAPIError::new(
                status,
                json!({"error": {"message": message.clone()}}),
                Some(message),
                ReqHeaderMap::new(),
            ))
        }
        crate::api_client::ApiError::RateLimited { retry_after_ms } => {
            let message = format!("Rate limited; retry after {}ms", retry_after_ms);
            ApiError::Api(MossenAPIError::new(
                429,
                json!({"error": {"message": message.clone()}}),
                Some(message),
                ReqHeaderMap::new(),
            ))
        }
        crate::api_client::ApiError::Overloaded { message } => ApiError::Api(MossenAPIError::new(
            529,
            json!({"error": {"message": message.clone()}}),
            Some(message),
            ReqHeaderMap::new(),
        )),
        crate::api_client::ApiError::Connection { message }
        | crate::api_client::ApiError::Network(message) => {
            ApiError::Connection(MossenAPIConnectionError::new(Some(message)))
        }
        crate::api_client::ApiError::StreamTimeout => {
            ApiError::ConnectionTimeout(MossenAPIConnectionTimeoutError::new(Some(
                "Streaming custom backend request timed out".to_string(),
            )))
        }
        crate::api_client::ApiError::Cancelled => ApiError::UserAbort(MossenAPIUserAbortError),
        crate::api_client::ApiError::ContextOverflow {
            input_tokens,
            limit,
        } => ApiError::Other(format!(
            "Context window overflow: input={}, limit={}",
            input_tokens, limit
        )),
        crate::api_client::ApiError::StreamParse(message) => ApiError::Other(message),
    }
}

/// OpenAI-compatible route for custom backends (ExampleProvider / Qwen / provider / etc.).
///
/// Reuses the existing OpenAI adapter:
///   1. Override `model` field with `MOSSEN_CODE_CUSTOM_MODEL` if set.
///   2. Force `stream:false` (streaming wiring deferred to a follow-up).
///   3. Build `OpenAICompatibleClient` with auth headers from custom_backend module.
///   4. Convert Provider-style params → OpenAI body → call /chat/completions →
///      OpenAI response → `MossenBetaMessage` shape that downstream expects.
async fn openai_compat_make_api_request(params: &Value, base_url: &str) -> Result<Value, ApiError> {
    use crate::api::openai::{
        OpenAICompatibleClient, OpenAICompatibleClientOptions, RequestOptions,
    };
    use std::collections::HashMap;

    // Override model when custom backend defines one.
    let mut adjusted = params.clone();
    if let Some(custom_model) = mossen_utils::custom_backend::get_custom_backend_model() {
        if let Some(obj) = adjusted.as_object_mut() {
            obj.insert("model".to_string(), json!(custom_model));
        }
    }
    // Force non-streaming for first integration (OpenAI streaming returns SSE in
    // a different envelope; the streaming path through the existing semantic
    // adapter requires more glue — out of scope for the smoke test).
    if let Some(obj) = adjusted.as_object_mut() {
        obj.insert("stream".to_string(), json!(false));
        obj.remove("betas");
    }

    let default_headers: HashMap<String, String> =
        mossen_utils::custom_backend::get_custom_backend_auth_headers();

    let timeout_ms = env::var("API_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(600_000);

    let options = OpenAICompatibleClientOptions {
        base_url: base_url.to_string(),
        default_headers,
        timeout_ms,
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(classify_reqwest_error)?;

    let openai_client = OpenAICompatibleClient::new(client, options);
    let request_options = RequestOptions::default();

    let mossen_msg = openai_client
        .create_message(&adjusted, &request_options)
        .await
        .map_err(ApiError::Api)?;

    serde_json::to_value(&mossen_msg).map_err(|e| {
        ApiError::Other(format!(
            "Failed to serialize OpenAI-compat MossenBetaMessage: {}",
            e
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn custom_backend_test_lock() -> tokio::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock_async().await
    }

    struct EnvRestore {
        vars: Vec<(&'static str, Option<String>)>,
    }

    impl EnvRestore {
        fn set_custom_backend(base_url: &str, protocol: &str) -> Self {
            const KEYS: &[&str] = &[
                "MOSSEN_CODE_CUSTOM_API_KEY",
                "MOSSEN_CODE_CUSTOM_AUTH_TOKEN",
                "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL",
                "MOSSEN_CODE_CUSTOM_BASE_URL",
                "MOSSEN_CODE_CUSTOM_HEADERS",
                "MOSSEN_CODE_CUSTOM_MODEL",
                "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS",
                "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS",
                "MOSSEN_CODE_USE_CUSTOM_BACKEND",
            ];
            let vars = KEYS
                .iter()
                .map(|key| (*key, std::env::var(key).ok()))
                .collect();
            std::env::set_var("MOSSEN_CODE_CUSTOM_API_KEY", "sk-test");
            std::env::remove_var("MOSSEN_CODE_CUSTOM_AUTH_TOKEN");
            std::env::set_var("MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL", protocol);
            std::env::set_var("MOSSEN_CODE_CUSTOM_BASE_URL", base_url);
            std::env::remove_var("MOSSEN_CODE_CUSTOM_HEADERS");
            std::env::set_var("MOSSEN_CODE_CUSTOM_MODEL", "harness-test");
            std::env::set_var("MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS", "5");
            std::env::set_var("MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS", "5");
            std::env::set_var("MOSSEN_CODE_USE_CUSTOM_BACKEND", "1");
            Self { vars }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in self.vars.drain(..) {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }

    fn sse_event(event: &str, data: serde_json::Value) -> String {
        format!("event: {event}\ndata: {data}\n\n")
    }

    fn http_sse_response(body: String) -> String {
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn openai_chat_response(text: &str) -> String {
        let body = json!({
            "id": "chatcmpl_legacy",
            "object": "chat.completion",
            "model": "harness-test",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": text,
                },
                "finish_reason": "stop",
            }],
            "usage": {
                "prompt_tokens": 3,
                "completion_tokens": 4,
                "total_tokens": 7,
            },
        })
        .to_string();
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn openai_responses_text_response(text: &str) -> String {
        let mut body = String::new();
        body.push_str(&sse_event(
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "delta": text,
            }),
        ));
        body.push_str(&sse_event(
            "response.completed",
            json!({
                "type": "response.completed",
                "response": {
                    "usage": {
                        "input_tokens": 3,
                        "output_tokens": 4,
                    },
                },
            }),
        ));
        http_sse_response(body)
    }

    fn anthropic_text_response(text: &str) -> String {
        let mut body = String::new();
        body.push_str(&sse_event(
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": "msg_legacy",
                    "type": "message",
                    "role": "assistant",
                    "model": "harness-test",
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 3,
                        "output_tokens": 0,
                    },
                },
            }),
        ));
        body.push_str(&sse_event(
            "content_block_start",
            json!({
                "index": 0,
                "content_block": {
                    "type": "text",
                    "text": "",
                },
            }),
        ));
        body.push_str(&sse_event(
            "content_block_delta",
            json!({
                "index": 0,
                "delta": {
                    "type": "text_delta",
                    "text": text,
                },
            }),
        ));
        body.push_str(&sse_event(
            "content_block_stop",
            json!({
                "index": 0,
            }),
        ));
        body.push_str(&sse_event(
            "message_delta",
            json!({
                "delta": {
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                },
                "usage": {
                    "input_tokens": 3,
                    "output_tokens": 5,
                },
            }),
        ));
        body.push_str(&sse_event("message_stop", json!({})));
        http_sse_response(body)
    }

    fn find_header_end(bytes: &[u8]) -> Option<usize> {
        bytes.windows(4).position(|window| window == b"\r\n\r\n")
    }

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> (String, String) {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 1024];
        let header_end = loop {
            let n = stream.read(&mut chunk).await.expect("read request");
            assert!(n > 0, "connection closed before request headers");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = find_header_end(&buf) {
                break pos + 4;
            }
        };
        let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().expect("content length"))
            })
            .unwrap_or(0);
        while buf.len() < header_end + content_length {
            let n = stream.read(&mut chunk).await.expect("read body");
            assert!(n > 0, "connection closed before request body");
            buf.extend_from_slice(&chunk[..n]);
        }
        (
            headers,
            String::from_utf8_lossy(&buf[header_end..header_end + content_length]).to_string(),
        )
    }

    async fn spawn_capture_server(
        response: String,
    ) -> (String, tokio::task::JoinHandle<(String, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind harness server");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let request = read_http_request(&mut stream).await;
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write response");
            request
        });
        (base_url, handle)
    }

    fn legacy_params() -> Value {
        json!({
            "model": "ignored-model",
            "messages": [{
                "role": "user",
                "content": "hello",
            }],
            "system": [{
                "type": "text",
                "text": "be brief",
            }],
            "max_tokens": 12,
            "temperature": 0.2,
            "stream": false,
        })
    }

    #[tokio::test]
    async fn custom_backend_non_streaming_routes_openai_compatible_uses_bearer_auth() {
        let _guard = custom_backend_test_lock().await;
        let (base_url, server) =
            spawn_capture_server(openai_chat_response("legacy openai ok")).await;
        let _env = EnvRestore::set_custom_backend(&base_url, "openai-compatible");

        let response = make_api_request(&legacy_params())
            .await
            .expect("OpenAI-compatible custom backend should complete");

        assert_eq!(response["content"][0]["text"], "legacy openai ok");
        assert_eq!(response["model"], "harness-test");
        assert_eq!(response["stop_reason"], "end_turn");
        let (headers, body) = tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .expect("harness server should receive request")
            .expect("harness server should join");
        assert!(headers.starts_with("POST /chat/completions "));
        assert!(headers.contains("authorization: Bearer sk-test"));
        assert!(!headers.to_ascii_lowercase().contains("x-api-key"));
        let body: Value = serde_json::from_str(&body).expect("request body should be JSON");
        assert_eq!(body["model"], "harness-test");
        assert_eq!(body["stream"], false);
        assert!(body.get("messages").is_some());
    }

    #[tokio::test]
    async fn custom_backend_non_streaming_routes_openai_responses_protocol() {
        let _guard = custom_backend_test_lock().await;
        let (base_url, server) =
            spawn_capture_server(openai_responses_text_response("legacy responses ok")).await;
        let _env = EnvRestore::set_custom_backend(&base_url, "openai-responses");

        let response = make_api_request(&legacy_params())
            .await
            .expect("OpenAI Responses custom backend should complete");

        assert_eq!(response["content"][0]["text"], "legacy responses ok");
        assert_eq!(response["model"], "harness-test");
        assert_eq!(response["stop_reason"], "end_turn");
        let (headers, body) = tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .expect("harness server should receive request")
            .expect("harness server should join");
        assert!(headers.starts_with("POST /v1/responses "));
        assert!(headers.contains("authorization: Bearer sk-test"));
        let body: Value = serde_json::from_str(&body).expect("request body should be JSON");
        assert_eq!(body["model"], "harness-test");
        assert_eq!(body["max_output_tokens"], 12);
        assert_eq!(body["stream"], true);
        assert!(body.get("input").is_some());
        assert!(body.get("messages").is_none());
        assert_eq!(body["temperature"], 0.2);
    }

    #[tokio::test]
    async fn custom_backend_non_streaming_routes_anthropic_protocol() {
        let _guard = custom_backend_test_lock().await;
        let (base_url, server) =
            spawn_capture_server(anthropic_text_response("legacy anthropic ok")).await;
        let _env = EnvRestore::set_custom_backend(&base_url, "anthropic");

        let response = make_api_request(&legacy_params())
            .await
            .expect("Anthropic custom backend should complete");

        assert_eq!(response["content"][0]["text"], "legacy anthropic ok");
        assert_eq!(response["model"], "harness-test");
        assert_eq!(response["stop_reason"], "end_turn");
        let (headers, body) = tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .expect("harness server should receive request")
            .expect("harness server should join");
        assert!(headers.starts_with("POST /v1/messages "));
        assert!(headers.contains("x-api-key: sk-test"));
        assert!(headers.contains("anthropic-version: 2023-06-01"));
        let body: Value = serde_json::from_str(&body).expect("request body should be JSON");
        assert_eq!(body["model"], "harness-test");
        assert_eq!(body["max_tokens"], 12);
        assert_eq!(body["stream"], true);
        assert!(body.get("messages").is_some());
        assert!(body.get("input").is_none());
        assert_eq!(body["temperature"], 0.2);
    }
}
