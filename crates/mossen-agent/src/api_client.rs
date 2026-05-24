//! # api_client — API 调用层
//!
//! 对应 TS `services/api/mossen.ts`，负责 HTTP 请求构建、SSE 流式响应处理。

use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use futures::stream::{Stream, StreamExt};
use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE};
use reqwest::Client;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::streaming::{parse_sse_event, RawSseEvent, StreamEvent, StreamParseError};
use crate::types::{
    ApiMetadata, MessageParam, StreamRequestParams, SystemBlock, ThinkingConfig, ToolChoice,
};
use mossen_types::ToolDefinition;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 默认 API 基础 URL。
const DEFAULT_API_BASE_URL: &str = "https://api.mossen.invalid/v1";
/// 默认流式超时（秒）。
const STREAM_TIMEOUT_SECS: u64 = 90;
/// 默认 OpenAI-compatible 流式无语义进展超时（秒）。
const OPENAI_COMPAT_STREAM_TIMEOUT_SECS: u64 = 300;
/// 默认 OpenAI-compatible 响应头等待超时（秒）。
const OPENAI_COMPAT_REQUEST_TIMEOUT_SECS: u64 = 90;
/// SSE 心跳间隔（秒）。
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

fn duration_from_env_secs(key: &str, default_secs: u64) -> Duration {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(default_secs))
}

fn openai_compat_stream_timeout() -> Duration {
    duration_from_env_secs(
        "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS",
        OPENAI_COMPAT_STREAM_TIMEOUT_SECS,
    )
}

fn openai_compat_request_timeout() -> Duration {
    duration_from_env_secs(
        "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS",
        OPENAI_COMPAT_REQUEST_TIMEOUT_SECS,
    )
}

// ---------------------------------------------------------------------------
// API 客户端
// ---------------------------------------------------------------------------

/// API 客户端配置。
#[derive(Debug, Clone)]
pub struct ApiClientConfig {
    /// 基础 URL。
    pub base_url: String,
    /// API 密钥。
    pub api_key: String,
    /// 额外请求头。
    pub extra_headers: HeaderMap,
    /// HTTP 客户端。
    pub client: Client,
}

impl ApiClientConfig {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(5)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            base_url: base_url.unwrap_or_else(|| DEFAULT_API_BASE_URL.to_string()),
            api_key,
            extra_headers: HeaderMap::new(),
            client,
        }
    }

    /// 构建消息 API 端点 URL。
    fn messages_url(&self) -> String {
        format!("{}/messages", self.base_url)
    }
}

// ---------------------------------------------------------------------------
// API 错误
// ---------------------------------------------------------------------------

/// API 错误类型。
#[derive(Debug, Clone, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP error: {status} - {message}")]
    Http { status: u16, message: String },

    #[error("Rate limited (429): retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("Overloaded (529): {message}")]
    Overloaded { message: String },

    #[error("Auth error ({status}): {message}")]
    Auth { status: u16, message: String },

    #[error("Context window overflow: input={input_tokens}, limit={limit}")]
    ContextOverflow { input_tokens: u64, limit: u64 },

    #[error("Connection error: {message}")]
    Connection { message: String },

    #[error("Stream timeout")]
    StreamTimeout,

    #[error("Request cancelled")]
    Cancelled,

    #[error("Stream parse error: {0}")]
    StreamParse(String),

    #[error("Network error: {0}")]
    Network(String),
}

impl ApiError {
    /// 获取 HTTP 状态码（如果适用）。
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::Http { status, .. } => Some(*status),
            Self::RateLimited { .. } => Some(429),
            Self::Overloaded { .. } => Some(529),
            Self::Auth { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// 是否为可重试错误。
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Http { status, .. } => *status == 408 || *status == 409 || *status >= 500,
            Self::RateLimited { .. } => true,
            Self::Overloaded { .. } => true,
            Self::Connection { .. } => true,
            Self::StreamTimeout => true,
            Self::Network(_) => true,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// 流式 API 调用
// ---------------------------------------------------------------------------

/// 执行流式 API 调用，返回 StreamEvent 流。
///
/// 对应 TS `stream()` SDK 方法。
pub async fn call_streaming(
    config: &ApiClientConfig,
    params: &StreamRequestParams,
    cancel: CancellationToken,
) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, ApiError>> + Send>>, ApiError> {
    // Custom-backend (MiniMax / Qwen / GLM …) → OpenAI /chat/completions.
    // 这些后端没有 Provider 兼容 `/v1/messages`；用 OpenAI 适配 + 合成流。
    if mossen_utils::custom_backend::is_custom_backend_enabled() {
        let base_url =
            std::env::var("MOSSEN_CODE_CUSTOM_BASE_URL").unwrap_or_else(|_| "<unset>".to_string());
        tracing::info!(
            target: "mossen_agent::api_client",
            base_url = %base_url,
            model = %params.model,
            "custom backend routing: OpenAI-compat /chat/completions"
        );
        return call_streaming_openai_compat(params, cancel).await;
    }

    let url = config.messages_url();

    debug!(url = %url, model = %params.model, "Starting streaming API call");

    let mut request = config
        .client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "text/event-stream")
        .header("x-api-key", &config.api_key)
        .header("mossen-version", "2023-06-01");

    // 添加额外请求头
    for (key, value) in config.extra_headers.iter() {
        request = request.header(key, value);
    }

    let response = request
        .json(params)
        .send()
        .await
        .map_err(|e| ApiError::Connection {
            message: e.to_string(),
        })?;

    let status = response.status();
    if !status.is_success() {
        let status_code = status.as_u16();
        let body = response.text().await.unwrap_or_default();

        return Err(classify_http_error(status_code, &body));
    }

    // 将响应字节流转化为 SSE 事件流
    let byte_stream = response.bytes_stream();

    let stream = sse_event_stream(byte_stream, cancel);

    Ok(Box::pin(stream))
}

/// 将字节流转化为 SSE 事件流（带超时和取消支持）。
fn sse_event_stream(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + Unpin + 'static,
    cancel: CancellationToken,
) -> impl Stream<Item = Result<StreamEvent, ApiError>> + Send {
    async_stream::stream! {
        let mut byte_stream = Box::pin(byte_stream);
        let timeout_duration = Duration::from_secs(STREAM_TIMEOUT_SECS);
        let mut deadline = tokio::time::Instant::now() + timeout_duration;
        let mut buffer = String::new();
        let _current_event = String::new();
        let _current_data = String::new();

        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    yield Err(ApiError::Cancelled);
                    break;
                }
                _ = tokio::time::sleep_until(deadline) => {
                    yield Err(ApiError::StreamTimeout);
                    break;
                }
                chunk = byte_stream.next() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            // 重置超时
                            deadline = tokio::time::Instant::now() + timeout_duration;

                            let text = String::from_utf8_lossy(&bytes);
                            buffer.push_str(&text);

                            // 处理缓冲区中的完整 SSE 事件
                            while let Some(pos) = buffer.find("\n\n") {
                                let event_text = buffer[..pos].to_string();
                                buffer = buffer[pos + 2..].to_string();

                                if let Some(event) = parse_raw_sse_block(&event_text) {
                                    match parse_sse_event(&event) {
                                        Ok(stream_event) => {
                                            yield Ok(stream_event);
                                        }
                                        Err(StreamParseError::UnknownEvent(_)) => {
                                            // 忽略未知事件类型
                                            continue;
                                        }
                                        Err(e) => {
                                            yield Err(ApiError::StreamParse(e.to_string()));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            yield Err(ApiError::Connection { message: e.to_string() });
                            break;
                        }
                        None => {
                            // 流正常结束
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// 解析一个原始 SSE 块。
fn parse_raw_sse_block(block: &str) -> Option<RawSseEvent> {
    let mut event = String::new();
    let mut data_parts: Vec<&str> = Vec::new();

    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event: ") {
            event = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data: ") {
            data_parts.push(value);
        } else if line.starts_with("event:") {
            event = line["event:".len()..].trim().to_string();
        } else if line.starts_with("data:") {
            data_parts.push(line["data:".len()..].trim());
        }
    }

    if event.is_empty() && data_parts.is_empty() {
        return None;
    }

    Some(RawSseEvent {
        event,
        data: data_parts.join("\n"),
    })
}

/// 将 HTTP 错误状态码分类为具体错误类型。
fn classify_http_error(status: u16, body: &str) -> ApiError {
    match status {
        401 | 403 => ApiError::Auth {
            status,
            message: body.to_string(),
        },
        429 => {
            // 尝试从 body 或 header 提取 retry-after
            ApiError::RateLimited {
                retry_after_ms: 60_000,
            }
        }
        529 => ApiError::Overloaded {
            message: body.to_string(),
        },
        400 => {
            // 检查是否为上下文溢出
            if body.contains("prompt is too long") || body.contains("max_tokens") {
                ApiError::ContextOverflow {
                    input_tokens: 0,
                    limit: 0,
                }
            } else {
                ApiError::Http {
                    status,
                    message: body.to_string(),
                }
            }
        }
        _ => ApiError::Http {
            status,
            message: body.to_string(),
        },
    }
}

/// 构建流式请求参数。
pub fn build_stream_request(
    model: &str,
    max_tokens: u32,
    messages: &[mossen_types::Message],
    system: &[SystemBlock],
    tools: &[ToolDefinition],
    thinking: Option<&ThinkingConfig>,
    tool_choice: Option<&ToolChoice>,
    extra_body: &HashMap<String, serde_json::Value>,
    metadata: &ApiMetadata,
) -> StreamRequestParams {
    let message_params: Vec<MessageParam> = messages.iter().map(MessageParam::from).collect();

    StreamRequestParams {
        model: model.to_string(),
        max_tokens,
        messages: message_params,
        system: system.to_vec(),
        tools: tools.to_vec(),
        thinking: thinking.cloned(),
        tool_choice: tool_choice.cloned(),
        stream: true,
        metadata: metadata.clone(),
        extra_body: extra_body.clone(),
    }
}

// ---------------------------------------------------------------------------
// OpenAI-compatible custom backend route (MiniMax / Qwen / GLM …)
// ---------------------------------------------------------------------------
//
// 这些后端没有 Provider `/v1/messages` 兼容端点，只支持 OpenAI
// `/chat/completions`。本路径：
//   1. 从 `mossen_utils::custom_backend` 拿 base_url / auth / model 覆盖。
//   2. 把 Provider-style 请求转成 OpenAI body（messages + stream:true）。
//   3. POST 到 `{base}/chat/completions`，启用 SSE 流式。
//   4. 把每个 SSE chunk 实时转成 StreamEvent 发回 UI，让"打字机"
//      效果真正逐 token 显示而不是等响应完整再一次性渲染。
//
// `<think>...</think>` 块由 MiniMax 直接嵌在 content 文本里，作为
// 普通 TextDelta 经过——前端可自由决定渲染样式（dim、折叠等）。

async fn call_streaming_openai_compat(
    params: &StreamRequestParams,
    cancel: CancellationToken,
) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, ApiError>> + Send>>, ApiError> {
    use crate::streaming::{ContentBlockInfo, MessageDeltaPayload, MessageStartPayload};
    use crate::types::ApiUsage;
    use eventsource_stream::Eventsource;
    use mossen_utils::custom_backend;
    use serde_json::json;

    if cancel.is_cancelled() {
        return Err(ApiError::Cancelled);
    }

    let base_url =
        custom_backend::get_custom_backend_base_url().ok_or_else(|| ApiError::Connection {
            message: "Custom backend enabled but base URL not configured".to_string(),
        })?;
    let chat_url = openai_compat_chat_url(&base_url);

    // 模型覆盖：custom backend 设置的 model 优先于 params.model。
    let model = custom_backend::get_custom_backend_model().unwrap_or_else(|| params.model.clone());

    // Build OpenAI messages from Provider-style messages — faithful port of
    // openaiCompatibleClient.ts::mossenMessagesToOpenAI. Assistant tool_use
    // blocks become `tool_calls`; user tool_result blocks become separate
    // `role: 'tool'` messages keyed by tool_call_id. Without this the model
    // never sees the tool I/O loop and falls back to writing bash inside
    // markdown code blocks.
    let openai_messages = build_openai_messages(&params.system, &params.messages);

    // Translate tool definitions to OpenAI function format. Empty list →
    // omit the `tools` key entirely (some endpoints reject `tools: []`).
    let openai_tools = build_openai_tools(&params.tools);
    let openai_tool_choice = build_openai_tool_choice(params.tool_choice.as_ref());

    let mut body = json!({
        "model": model,
        "max_tokens": params.max_tokens,
        "messages": openai_messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    if let Some(tools_value) = openai_tools {
        body["tools"] = tools_value;
    }
    if let Some(tc) = openai_tool_choice {
        body["tool_choice"] = tc;
    }

    let request_timeout = openai_compat_request_timeout();
    let mut req = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| ApiError::Connection {
            message: format!("Failed to build HTTP client: {}", e),
        })?
        .post(&chat_url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "text/event-stream")
        .json(&body);

    if let Some(tok) = custom_backend::get_custom_backend_auth_token() {
        req = req.header("Authorization", format!("Bearer {}", tok));
    } else if let Some(key) = custom_backend::get_custom_backend_api_key() {
        req = req.header("x-api-key", key);
    }
    for (k, v) in custom_backend::get_custom_backend_headers() {
        req = req.header(k, v);
    }

    tracing::info!(
        target: "mossen_agent::api_client",
        url = %chat_url,
        model = %model,
        tool_count = params.tools.len(),
        msg_count = openai_messages.len(),
        body_has_tools = body.get("tools").is_some(),
        request_timeout_ms = request_timeout.as_millis() as u64,
        "OpenAI-compat request dispatch",
    );

    let response = tokio::time::timeout(request_timeout, req.send())
        .await
        .map_err(|_| ApiError::Connection {
            message: format!(
                "OpenAI-compatible backend did not return stream headers within {}s",
                request_timeout.as_secs()
            ),
        })?
        .map_err(|e| ApiError::Connection {
            message: e.to_string(),
        })?;
    let status = response.status();
    if !status.is_success() {
        let code = status.as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(classify_http_error(code, &body));
    }
    tracing::info!(
        target: "mossen_agent::api_client",
        status = status.as_u16(),
        "OpenAI-compat SSE response received",
    );

    let model_str = model.clone();
    let semantic_timeout = openai_compat_stream_timeout();
    let stream = async_stream::stream! {
        use futures::stream::StreamExt;

        // Emit MessageStart immediately so the UI can attach the spinner and
        // reserve the assistant placeholder ahead of the first token. The
        // assistant text block (index 0) is opened lazily — the first time we
        // see a non-empty `delta.content` chunk — because some turns are pure
        // tool_calls with no text at all (model just decides to call a tool).
        yield Ok(StreamEvent::MessageStart {
            message: MessageStartPayload {
                id: "openai-compat".to_string(),
                message_type: "message".to_string(),
                role: "assistant".to_string(),
                model: model_str.clone(),
                usage: None,
            },
        });

        let mut sse = response.bytes_stream().eventsource();
        let mut finish_reason: Option<String> = None;
        let mut final_usage: Option<ApiUsage> = None;
        // Index 0 is reserved for the assistant text block; tool_use blocks
        // live at indices >= 1. Their accumulator state lives in `tool_state`
        // (one slot per tool_call index emitted by the upstream).
        let mut text_block_open = false;
        let mut tool_state: Vec<ToolCallAccum> = Vec::new();
        let mut next_tool_block_index: usize = 1;
        let mut delta_chunks: u64 = 0;
        let mut semantic_deadline = tokio::time::Instant::now() + semantic_timeout;

        loop {
            let event = tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    yield Err(ApiError::Cancelled);
                    return;
                }
                _ = tokio::time::sleep_until(semantic_deadline) => {
                    yield Err(ApiError::StreamTimeout);
                    return;
                }
                event = sse.next() => {
                    match event {
                        Some(Ok(event)) => event,
                        Some(Err(e)) => {
                            yield Err(ApiError::Connection {
                                message: format!("SSE stream error: {}", e),
                            });
                            return;
                        }
                        None => break,
                    }
                }
            };

            if event.data.trim() == "[DONE]" {
                break;
            }

            let chunk: serde_json::Value = match serde_json::from_str(&event.data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let mut saw_semantic_progress = false;

            if let Some(choices) = chunk.get("choices").and_then(|v| v.as_array()) {
                if let Some(first) = choices.first() {
                    // 1) Text content delta — open the text block lazily.
                    if let Some(text) = openai_choice_content_text(first) {
                        if !text.is_empty() {
                            saw_semantic_progress = true;
                            if !text_block_open {
                                yield Ok(StreamEvent::ContentBlockStart {
                                    index: 0,
                                    content_block: ContentBlockInfo::Text {
                                        text: String::new(),
                                    },
                                });
                                text_block_open = true;
                            }
                            delta_chunks += 1;
                            debug!(chunk = delta_chunks, len = text.len(), "SSE text delta");
                            yield Ok(StreamEvent::ContentBlockDelta {
                                index: 0,
                                delta: crate::types::ContentDelta::TextDelta {
                                    text,
                                },
                            });
                        }
                    }

                    // 2) tool_calls deltas — each element carries an `index`
                    //    (per-tool, 0-based on the OpenAI side), and either
                    //    `id`+`function.name` (first chunk for that tool) or
                    //    `function.arguments` (subsequent JSON fragments).
                    if let Some(tool_calls) = openai_choice_tool_calls(first) {
                        for tc in tool_calls {
                            saw_semantic_progress = true;
                            let openai_idx = tc
                                .get("index")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as usize;
                            // Lazily extend tool_state so each new tool index
                            // gets a fresh accumulator slot.
                            while tool_state.len() <= openai_idx {
                                tool_state.push(ToolCallAccum::with_fallback_id());
                            }
                            let accum = &mut tool_state[openai_idx];

                            // First sight of this tool index — emit
                            // ContentBlockStart{ToolUse} once we have a
                            // function name. Some OpenAI-compatible streams
                            // omit `id` in deltas; a stable fallback id keeps
                            // the agent loop alive and is echoed back in the
                            // next request's tool result.
                            if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                                if !id.is_empty()
                                    && !accum.started
                                    && (accum.id.is_empty() || accum.id_is_fallback)
                                {
                                    accum.id = id.to_string();
                                    accum.id_is_fallback = false;
                                }
                            }
                            if let Some(name) = tc
                                .get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                            {
                                if !name.is_empty() && accum.name.is_empty() {
                                    accum.name = name.to_string();
                                }
                            }
                            if !accum.started && !accum.name.is_empty() {
                                let block_idx = next_tool_block_index;
                                next_tool_block_index += 1;
                                accum.block_index = block_idx;
                                accum.started = true;
                                debug!(
                                    block_idx = block_idx,
                                    tool_id = %accum.id,
                                    tool_name = %accum.name,
                                    "SSE tool_use block start"
                                );
                                yield Ok(StreamEvent::ContentBlockStart {
                                    index: block_idx,
                                    content_block: ContentBlockInfo::ToolUse {
                                        id: accum.id.clone(),
                                        name: accum.name.clone(),
                                    },
                                });
                            }

                            // Arguments fragment — JSON string built up
                            // character-by-character. Forward as
                            // InputJsonDelta only after the tool block was
                            // opened; otherwise buffer it.
                            if let Some(args) = tc
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .and_then(|a| a.as_str())
                            {
                                if !args.is_empty() {
                                    if accum.started {
                                        delta_chunks += 1;
                                        yield Ok(StreamEvent::ContentBlockDelta {
                                            index: accum.block_index,
                                            delta: crate::types::ContentDelta::InputJsonDelta {
                                                partial_json: args.to_string(),
                                            },
                                        });
                                    } else {
                                        accum.pending_args.push_str(args);
                                    }
                                }
                            }

                            // If we just opened the block and had buffered
                            // args, flush them in order.
                            if accum.started && !accum.pending_args.is_empty() {
                                let pending = std::mem::take(&mut accum.pending_args);
                                yield Ok(StreamEvent::ContentBlockDelta {
                                    index: accum.block_index,
                                    delta: crate::types::ContentDelta::InputJsonDelta {
                                        partial_json: pending,
                                    },
                                });
                            }
                        }
                    }

                    if let Some(reason) =
                        first.get("finish_reason").and_then(|v| v.as_str())
                    {
                        saw_semantic_progress = true;
                        finish_reason = Some(match reason {
                            "length" => "max_tokens".to_string(),
                            "tool_calls" | "function_call" => "tool_use".to_string(),
                            "stop" => "end_turn".to_string(),
                            other => other.to_string(),
                        });
                    }
                }
            }

            if let Some(usage) = chunk.get("usage") {
                saw_semantic_progress = true;
                let input_tokens =
                    usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output_tokens = usage
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                final_usage = Some(ApiUsage {
                    input_tokens,
                    output_tokens,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                });
            }

            if saw_semantic_progress {
                semantic_deadline = tokio::time::Instant::now() + semantic_timeout;
            }
        }

        // Close any blocks we opened — order matters for downstream
        // accumulators that match start/stop indices.
        if text_block_open {
            yield Ok(StreamEvent::ContentBlockStop { index: 0 });
        }
        for accum in &tool_state {
            if accum.started {
                yield Ok(StreamEvent::ContentBlockStop { index: accum.block_index });
            }
        }

        let stop_reason = finish_reason.unwrap_or_else(|| "end_turn".to_string());
        debug!(
            total_chunks = delta_chunks,
            tool_calls = tool_state.iter().filter(|t| t.started).count(),
            stop_reason = %stop_reason,
            "SSE stream complete"
        );
        yield Ok(StreamEvent::MessageDelta {
            delta: MessageDeltaPayload {
                stop_reason: Some(stop_reason),
                stop_sequence: None,
            },
            usage: final_usage,
        });
        yield Ok(StreamEvent::MessageStop);
    };
    Ok(Box::pin(stream))
}

#[derive(Default, Debug)]
struct ToolCallAccum {
    id: String,
    id_is_fallback: bool,
    name: String,
    started: bool,
    block_index: usize,
    pending_args: String,
}

impl ToolCallAccum {
    fn with_fallback_id() -> Self {
        Self {
            id: format!("call_{}", uuid::Uuid::new_v4().simple()),
            id_is_fallback: true,
            ..Self::default()
        }
    }
}

fn openai_choice_content_text(choice: &serde_json::Value) -> Option<String> {
    choice
        .get("delta")
        .and_then(|d| d.get("content"))
        .or_else(|| choice.get("message").and_then(|m| m.get("content")))
        .and_then(openai_content_text)
}

fn openai_content_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(text) = item.as_str() {
                    out.push_str(text);
                } else if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    out.push_str(text);
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        _ => None,
    }
}

fn openai_choice_tool_calls(choice: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    choice
        .get("delta")
        .and_then(|d| d.get("tool_calls"))
        .or_else(|| choice.get("message").and_then(|m| m.get("tool_calls")))
        .and_then(|tc| tc.as_array())
}

fn openai_compat_chat_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{}/chat/completions", trimmed)
    } else {
        format!("{}/v1/chat/completions", trimmed)
    }
}

// ---------------------------------------------------------------------------
// Provider ↔ OpenAI message / tool translation
// ---------------------------------------------------------------------------
// Faithful Rust port of openaiCompatibleClient.ts::mossen{Messages,Tools,
// ToolChoice}ToOpenAI. These exist because the MiniMax/OpenAI-compatible
// backend speaks chat.completions, not /v1/messages.

fn build_openai_messages(
    system: &[SystemBlock],
    messages: &[MessageParam],
) -> Vec<serde_json::Value> {
    use mossen_types::{ContentBlock, ToolResultContent};
    use serde_json::json;

    let mut out: Vec<serde_json::Value> = Vec::new();

    let system_text: String = system
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    if !system_text.trim().is_empty() {
        out.push(json!({"role": "system", "content": system_text}));
    }

    for msg in messages {
        let role = msg.role.as_str();
        if role == "assistant" {
            let mut text_parts: Vec<String> = Vec::new();
            let mut tool_calls: Vec<serde_json::Value> = Vec::new();
            for block in &msg.content {
                match block {
                    ContentBlock::Text(tb) => text_parts.push(tb.text.clone()),
                    ContentBlock::Thinking(_) => { /* dropped — providers reject */ }
                    ContentBlock::ToolUse(tu) => {
                        tool_calls.push(json!({
                            "id": tu.id,
                            "type": "function",
                            "function": {
                                "name": tu.name,
                                "arguments": tu.input.to_string(),
                            },
                        }));
                    }
                    _ => {}
                }
            }
            let mut obj = serde_json::Map::new();
            obj.insert("role".to_string(), json!("assistant"));
            // OpenAI rejects assistant messages where both content and
            // tool_calls are absent. Use empty string when only tool_calls.
            if text_parts.is_empty() && tool_calls.is_empty() {
                obj.insert("content".to_string(), json!(""));
            } else if text_parts.is_empty() {
                // OpenAI allows null content for tool-call-only assistant
                // messages, but some OpenAI-compatible gateways stall or reject
                // the next turn after tool results. Empty string keeps the
                // assistant/tool_call pairing valid while maximizing backend
                // compatibility.
                obj.insert("content".to_string(), json!(""));
            } else {
                obj.insert("content".to_string(), json!(text_parts.join("\n\n")));
            }
            if !tool_calls.is_empty() {
                obj.insert("tool_calls".to_string(), json!(tool_calls));
            }
            out.push(serde_json::Value::Object(obj));
            continue;
        }

        // User role: text + image blocks are coalesced into a single
        // user message. When at least one image block is present we
        // emit OpenAI vision's array-of-blocks content format:
        //   [{"type":"text","text":"…"},
        //    {"type":"image_url","image_url":{"url":"data:image/png;base64,…"}}]
        // Otherwise we stay on the plain-string content path so non-
        // vision backends keep working.
        // tool_result blocks remain their own role:"tool" messages.
        let mut pending_text: Vec<String> = Vec::new();
        let mut pending_images: Vec<serde_json::Value> = Vec::new();
        let flush_user = |text_buf: &mut Vec<String>,
                          img_buf: &mut Vec<serde_json::Value>,
                          out: &mut Vec<serde_json::Value>| {
            if text_buf.is_empty() && img_buf.is_empty() {
                return;
            }
            if img_buf.is_empty() {
                out.push(json!({"role": "user", "content": text_buf.join("\n\n")}));
            } else {
                let mut blocks: Vec<serde_json::Value> = Vec::new();
                if !text_buf.is_empty() {
                    blocks.push(json!({"type": "text", "text": text_buf.join("\n\n")}));
                }
                blocks.extend(std::mem::take(img_buf));
                out.push(json!({"role": "user", "content": blocks}));
            }
            text_buf.clear();
        };

        for block in &msg.content {
            match block {
                ContentBlock::Text(tb) => pending_text.push(tb.text.clone()),
                ContentBlock::Image(img) => {
                    // OpenAI vision: `image_url.url` accepts a data URI.
                    // We only emit base64-source images here; URL-source
                    // images could be passed through verbatim but the
                    // upstream Provider shape doesn't carry that flag.
                    let mime = if img.source.media_type.is_empty() {
                        "image/png".to_string()
                    } else {
                        img.source.media_type.clone()
                    };
                    let url = format!("data:{};base64,{}", mime, img.source.data);
                    pending_images.push(json!({
                        "type": "image_url",
                        "image_url": { "url": url },
                    }));
                }
                ContentBlock::ToolResult(tr) => {
                    flush_user(&mut pending_text, &mut pending_images, &mut out);
                    let content_text = match &tr.content {
                        ToolResultContent::Text(s) => s.clone(),
                        ToolResultContent::Blocks(blocks) => {
                            let mut buf = String::new();
                            for b in blocks {
                                if let ContentBlock::Text(t) = b {
                                    if !buf.is_empty() {
                                        buf.push('\n');
                                    }
                                    buf.push_str(&t.text);
                                }
                            }
                            buf
                        }
                    };
                    let mut tool_msg = serde_json::Map::new();
                    tool_msg.insert("role".to_string(), json!("tool"));
                    tool_msg.insert("tool_call_id".to_string(), json!(tr.tool_use_id));
                    tool_msg.insert("content".to_string(), json!(content_text));
                    if tr.is_error.unwrap_or(false) {
                        tool_msg.insert("name".to_string(), json!("tool_error"));
                    }
                    out.push(serde_json::Value::Object(tool_msg));
                }
                _ => {}
            }
        }
        flush_user(&mut pending_text, &mut pending_images, &mut out);
    }

    out
}

fn build_openai_tools(tools: &[ToolDefinition]) -> Option<serde_json::Value> {
    use serde_json::json;
    if tools.is_empty() {
        return None;
    }
    let arr: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            // ToolInputSchema → JSON object (it already serialises as a JSON
            // Schema fragment via serde — re-serialise to drop the Rust-side
            // Option wrappers cleanly).
            let parameters = serde_json::to_value(&t.input_schema).unwrap_or_else(
                |_| json!({"type": "object", "properties": {}, "additionalProperties": true}),
            );
            let mut function = serde_json::Map::new();
            function.insert("name".to_string(), json!(t.name));
            if !t.description.is_empty() {
                function.insert("description".to_string(), json!(t.description));
            }
            function.insert("parameters".to_string(), parameters);
            json!({"type": "function", "function": function})
        })
        .collect();
    Some(serde_json::Value::Array(arr))
}

fn build_openai_tool_choice(choice: Option<&ToolChoice>) -> Option<serde_json::Value> {
    use serde_json::json;
    let choice = choice?;
    // The Rust ToolChoice enum mirrors the Provider shape. We translate the
    // two cases the API actually accepts (auto + specific tool); anything
    // else maps to None so the upstream picks a default.
    let raw = serde_json::to_value(choice).ok()?;
    let kind = raw.get("type").and_then(|v| v.as_str())?;
    match kind {
        "auto" | "any" => Some(json!("auto")),
        "tool" => {
            let name = raw.get("name").and_then(|v| v.as_str())?;
            Some(json!({"type": "function", "function": {"name": name}}))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_openai_messages, openai_choice_content_text, openai_choice_tool_calls,
        openai_compat_chat_url, openai_content_text, ToolCallAccum,
    };
    use crate::types::MessageParam;
    use mossen_types::{ContentBlock, ToolUseBlock};
    use serde_json::json;

    #[test]
    fn content_text_accepts_openai_vision_array_blocks() {
        let content = json!([
            {"type": "text", "text": "hello "},
            {"text": "world"},
        ]);

        assert_eq!(
            openai_content_text(&content).as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn content_text_preserves_multibyte_markdown_chunks() {
        let content = json!([
            {"type": "text", "text": "先读 `crates/mossen-cli/src/main.rs`。\n"},
            {"type": "text", "text": "然后逐行分析代码块：\n```rust\nfn main() {}\n```"},
        ]);

        let text = openai_content_text(&content).expect("content text should parse");

        assert!(text.contains("逐行分析代码块"));
        assert!(text.contains("```rust"));
        assert!(text.is_char_boundary(text.len()));
    }

    #[test]
    fn choice_helpers_accept_message_level_final_chunks() {
        let choice = json!({
            "message": {
                "content": "final text",
                "tool_calls": [{
                    "index": 0,
                    "type": "function",
                    "function": {"name": "Bash", "arguments": "{\"cmd\":\"ls\"}"}
                }]
            },
            "finish_reason": "tool_calls"
        });

        assert_eq!(
            openai_choice_content_text(&choice).as_deref(),
            Some("final text")
        );
        assert_eq!(openai_choice_tool_calls(&choice).unwrap().len(), 1);
    }

    #[test]
    fn tool_accumulator_has_generated_fallback_id() {
        let accum = ToolCallAccum::with_fallback_id();

        assert!(accum.id.starts_with("call_"));
        assert!(accum.id_is_fallback);
    }

    #[test]
    fn chat_url_accepts_base_with_or_without_v1() {
        assert_eq!(
            openai_compat_chat_url("https://api.example.com"),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(
            openai_compat_chat_url("https://api.example.com/v1/"),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn openai_messages_keep_tool_call_only_assistant_content_as_empty_string() {
        let messages = vec![MessageParam {
            role: "assistant".to_string(),
            content: vec![ContentBlock::ToolUse(ToolUseBlock {
                id: "call-glob".to_string(),
                name: "Glob".to_string(),
                input: json!({"pattern": "**/*.md"}),
            })],
        }];

        let converted = build_openai_messages(&[], &messages);
        let assistant = converted
            .iter()
            .find(|message| message["role"] == "assistant")
            .expect("assistant message should be present");

        assert_eq!(assistant["content"], "");
        assert_eq!(assistant["tool_calls"][0]["id"], "call-glob");
    }
}
