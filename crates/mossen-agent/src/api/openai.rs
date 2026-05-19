//! # OpenAI Compatible Client
//!
//! 翻译自 `services/api/openaiCompatibleClient.ts` (1168行)
//! OpenAI 兼容客户端：消息转换、SSE 解析、流式响应。

use futures::stream::Stream;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;
use uuid::Uuid;

use super::sdk::MossenAPIError;

/// OpenAI compatible client options.
#[derive(Debug, Clone)]
pub struct OpenAICompatibleClientOptions {
    pub base_url: String,
    pub default_headers: HashMap<String, String>,
    pub timeout_ms: u64,
}

/// Request options for API calls.
#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    pub headers: Option<HashMap<String, String>>,
    pub timeout: Option<u64>,
}

/// OpenAI tool call representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAICompatibleToolCall {
    pub function: Option<OpenAIToolCallFunction>,
    pub id: Option<String>,
    pub index: Option<u32>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCallFunction {
    pub arguments: Option<String>,
    pub name: Option<String>,
}

/// OpenAI chat completion choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChatCompletionChoice {
    pub delta: Option<OpenAIChoiceDelta>,
    pub finish_reason: Option<String>,
    pub message: Option<OpenAIChoiceMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChoiceDelta {
    pub content: Option<Value>,
    pub role: Option<String>,
    pub tool_calls: Option<Vec<OpenAICompatibleToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChoiceMessage {
    pub content: Option<Value>,
    pub role: Option<String>,
    pub tool_calls: Option<Vec<OpenAICompatibleToolCall>>,
}

/// OpenAI chat completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChatCompletionResponse {
    pub choices: Option<Vec<OpenAIChatCompletionChoice>>,
    pub id: Option<String>,
    pub model: Option<String>,
    pub usage: Option<OpenAIUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIUsage {
    pub completion_tokens: Option<u64>,
    pub prompt_tokens: Option<u64>,
}

/// SSE frame parsed from a stream.
#[derive(Debug, Clone, Default)]
pub struct SSEFrame {
    pub data: Option<String>,
    pub event: Option<String>,
    pub id: Option<String>,
}

/// Mossen usage type.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MossenBetaUsage {
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Content block types for Mossen messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

/// Mossen message representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossenBetaMessage {
    pub id: String,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub role: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    #[serde(rename = "type")]
    pub message_type: String,
    pub usage: MossenBetaUsage,
}

/// Stream event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MossenBetaRawMessageStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: MossenBetaMessage },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: Value,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: Value },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta { delta: Value, usage: Value },
    #[serde(rename = "message_stop")]
    MessageStop,
}

/// Check if a string is a complete JSON payload.
fn is_complete_json_payload(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    serde_json::from_str::<Value>(trimmed).is_ok()
}

/// Build request headers combining defaults and request-specific headers.
fn build_request_headers(
    default_headers: &HashMap<String, String>,
    request_headers: Option<&HashMap<String, String>>,
) -> HeaderMap {
    let mut headers = HeaderMap::new();

    for (key, value) in default_headers {
        if let (Ok(k), Ok(v)) = (
            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.insert(k, v);
        }
    }

    if let Some(extra) = request_headers {
        for (key, value) in extra {
            if let (Ok(k), Ok(v)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                HeaderValue::from_str(value),
            ) {
                headers.insert(k, v);
            }
        }
    }

    if !headers.contains_key("content-type") {
        headers.insert("content-type", HeaderValue::from_static("application/json"));
    }

    headers
}

/// Flatten content to a string (handles both string and array content).
fn flatten_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut parts = Vec::new();
            for block in arr {
                if let Some(obj) = block.as_object() {
                    if obj.get("type").and_then(|t| t.as_str()) == Some("text") {
                        if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                            parts.push(text.to_string());
                        }
                    } else if obj.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                        if let Some(inner_content) = obj.get("content") {
                            let nested = flatten_content(inner_content);
                            if !nested.is_empty() {
                                let is_error = obj
                                    .get("is_error")
                                    .and_then(|e| e.as_bool())
                                    .unwrap_or(false);
                                if is_error {
                                    parts.push(format!("Tool error: {}", nested));
                                } else {
                                    parts.push(nested);
                                }
                            }
                        }
                    }
                }
            }
            parts.join("\n\n")
        }
        _ => String::new(),
    }
}

/// Flatten text content only.
fn flatten_text_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut parts = Vec::new();
            for block in arr {
                if let Value::String(s) = block {
                    parts.push(s.clone());
                } else if let Some(obj) = block.as_object() {
                    if obj.get("type").and_then(|t| t.as_str()) == Some("text") {
                        if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                            parts.push(text.to_string());
                        }
                    }
                }
            }
            parts.join("")
        }
        _ => String::new(),
    }
}

/// Normalize system prompt from various formats to a single string.
fn normalize_system_prompt(system: &Value) -> Option<String> {
    match system {
        Value::String(s) => {
            if s.trim().is_empty() {
                None
            } else {
                Some(s.clone())
            }
        }
        Value::Array(arr) => {
            let text: String = arr
                .iter()
                .filter_map(|block| {
                    block
                        .as_object()
                        .filter(|obj| obj.get("type").and_then(|t| t.as_str()) == Some("text"))
                        .and_then(|obj| obj.get("text").and_then(|t| t.as_str()))
                        .map(|s| s.to_string())
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        _ => None,
    }
}

/// Convert Mossen messages to OpenAI format.
pub fn mossen_messages_to_openai(
    system: &Value,
    messages: &[Value],
) -> Vec<Value> {
    let mut openai_messages: Vec<Value> = Vec::new();

    // System prompt
    if let Some(system_text) = normalize_system_prompt(system) {
        openai_messages.push(serde_json::json!({
            "role": "system",
            "content": system_text
        }));
    }

    for message in messages {
        let role = message.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = message.get("content").cloned().unwrap_or(Value::String(String::new()));

        if content.is_string() {
            openai_messages.push(serde_json::json!({ "role": role, "content": content }));
            continue;
        }

        if !content.is_array() {
            openai_messages.push(serde_json::json!({ "role": role, "content": "" }));
            continue;
        }

        if role == "assistant" {
            let blocks = content.as_array().unwrap();
            let mut text_parts: Vec<String> = Vec::new();
            let mut tool_calls: Vec<Value> = Vec::new();

            for block in blocks {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(text.to_string());
                        }
                    }
                    "thinking" => {
                        // Skip thinking blocks in OpenAI format
                    }
                    "tool_use" => {
                        let id = block
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&Uuid::new_v4().to_string())
                            .to_string();
                        let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let input = block.get("input").cloned().unwrap_or(Value::Object(Default::default()));
                        tool_calls.push(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "arguments": serde_json::to_string(&input).unwrap_or_default(),
                                "name": name
                            }
                        }));
                    }
                    _ => {}
                }
            }

            let mut msg = serde_json::json!({
                "role": "assistant",
                "content": if text_parts.is_empty() { Value::Null } else { Value::String(text_parts.join("\n\n")) }
            });
            if !tool_calls.is_empty() {
                msg.as_object_mut().unwrap().insert("tool_calls".into(), Value::Array(tool_calls));
            }
            openai_messages.push(msg);
        } else {
            // User messages
            let blocks = content.as_array().unwrap();
            let mut pending_user_text: Vec<String> = Vec::new();

            for block in blocks {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            pending_user_text.push(text.to_string());
                        }
                    }
                    "tool_result" => {
                        // Flush pending user text
                        if !pending_user_text.is_empty() {
                            openai_messages.push(serde_json::json!({
                                "role": "user",
                                "content": pending_user_text.join("\n\n")
                            }));
                            pending_user_text.clear();
                        }
                        let tool_call_id = block
                            .get("tool_use_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&Uuid::new_v4().to_string())
                            .to_string();
                        let tool_content = block
                            .get("content")
                            .map(|c| flatten_content(c))
                            .unwrap_or_default();
                        let mut tool_msg = serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id,
                            "content": tool_content
                        });
                        if block.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false) {
                            tool_msg.as_object_mut().unwrap().insert("name".into(), Value::String("tool_error".into()));
                        }
                        openai_messages.push(tool_msg);
                    }
                    _ => {}
                }
            }

            if !pending_user_text.is_empty() {
                openai_messages.push(serde_json::json!({
                    "role": "user",
                    "content": pending_user_text.join("\n\n")
                }));
            }
        }
    }

    openai_messages
}

/// Convert Mossen tools to OpenAI format.
pub fn mossen_tools_to_openai(tools: &[Value]) -> Option<Vec<Value>> {
    if tools.is_empty() {
        return None;
    }

    let openai_tools: Vec<Value> = tools
        .iter()
        .filter(|tool| {
            tool.get("name").is_some() && tool.get("input_schema").is_some()
        })
        .map(|tool| {
            let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let description = tool.get("description").and_then(|d| d.as_str());
            let parameters = tool
                .get("input_schema")
                .cloned()
                .unwrap_or(serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": true
                }));

            let mut function = serde_json::json!({
                "name": name,
                "parameters": parameters
            });
            if let Some(desc) = description {
                function.as_object_mut().unwrap().insert("description".into(), Value::String(desc.to_string()));
            }

            serde_json::json!({
                "type": "function",
                "function": function
            })
        })
        .collect();

    if openai_tools.is_empty() {
        None
    } else {
        Some(openai_tools)
    }
}

/// Convert Mossen tool choice to OpenAI format.
pub fn mossen_tool_choice_to_openai(tool_choice: Option<&Value>) -> Option<Value> {
    let choice = tool_choice?;
    let obj = choice.as_object()?;
    let choice_type = obj.get("type").and_then(|t| t.as_str())?;

    match choice_type {
        "auto" => Some(Value::String("auto".into())),
        "tool" => {
            let name = obj.get("name").and_then(|n| n.as_str())?;
            Some(serde_json::json!({
                "type": "function",
                "function": { "name": name }
            }))
        }
        _ => None,
    }
}

/// Map OpenAI stop reason to Mossen stop reason.
pub fn map_stop_reason(finish_reason: Option<&str>, has_tool_calls: bool) -> String {
    match finish_reason {
        Some("length") => "max_tokens".to_string(),
        Some("tool_calls") => "tool_use".to_string(),
        _ if has_tool_calls => "tool_use".to_string(),
        _ => "end_turn".to_string(),
    }
}

/// Convert OpenAI usage to Mossen usage.
pub fn to_mossen_usage(usage: Option<&OpenAIUsage>) -> MossenBetaUsage {
    MossenBetaUsage {
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        input_tokens: usage.and_then(|u| u.prompt_tokens).unwrap_or(0),
        output_tokens: usage.and_then(|u| u.completion_tokens).unwrap_or(0),
    }
}

/// Parse SSE frames from a buffer.
pub fn parse_sse_frames(buffer: &str) -> (Vec<SSEFrame>, String) {
    let mut frames = Vec::new();
    let mut pos = 0;

    while let Some(idx) = buffer[pos..].find("\n\n") {
        let raw_frame = &buffer[pos..pos + idx];
        pos = pos + idx + 2;

        if raw_frame.trim().is_empty() {
            continue;
        }

        let mut frame = SSEFrame::default();
        let mut is_comment = false;

        for line in raw_frame.split('\n') {
            if line.starts_with(':') {
                is_comment = true;
                continue;
            }

            let colon_idx = match line.find(':') {
                Some(idx) => idx,
                None => continue,
            };

            let field = &line[..colon_idx];
            let value = if line.as_bytes().get(colon_idx + 1) == Some(&b' ') {
                &line[colon_idx + 2..]
            } else {
                &line[colon_idx + 1..]
            };

            match field {
                "event" => frame.event = Some(value.to_string()),
                "id" => frame.id = Some(value.to_string()),
                "data" => {
                    frame.data = Some(match frame.data {
                        Some(existing) => format!("{}\n{}", existing, value),
                        None => value.to_string(),
                    });
                }
                _ => {}
            }
        }

        if frame.data.is_some() || is_comment {
            frames.push(frame);
        }
    }

    (frames, buffer[pos..].to_string())
}

/// Get request ID from response headers.
pub fn get_request_id(headers: &HeaderMap) -> String {
    headers
        .get("request-id")
        .or_else(|| headers.get("x-request-id"))
        .or_else(|| headers.get("openai-request-id"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// Create an OpenAI request body from Mossen parameters.
pub fn create_openai_request_body(params: &Value, stream: bool) -> Value {
    let system = params.get("system").cloned().unwrap_or(Value::Null);
    let messages = params
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    let openai_messages = mossen_messages_to_openai(&system, &messages);

    let mut body = serde_json::json!({
        "model": params.get("model").cloned().unwrap_or(Value::Null),
        "messages": openai_messages,
        "max_tokens": params.get("max_tokens").cloned().unwrap_or(Value::Null),
        "stream": stream
    });

    let tools = params
        .get("tools")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(openai_tools) = mossen_tools_to_openai(&tools) {
        body.as_object_mut()
            .unwrap()
            .insert("tools".into(), Value::Array(openai_tools));
    }

    if let Some(tool_choice) = mossen_tool_choice_to_openai(params.get("tool_choice")) {
        body.as_object_mut()
            .unwrap()
            .insert("tool_choice".into(), tool_choice);
    }

    if let Some(temp) = params.get("temperature").and_then(|t| t.as_f64()) {
        body.as_object_mut()
            .unwrap()
            .insert("temperature".into(), Value::from(temp));
    }

    if let Some(stop) = params.get("stop_sequences").and_then(|s| s.as_array()) {
        if !stop.is_empty() {
            body.as_object_mut()
                .unwrap()
                .insert("stop".into(), Value::Array(stop.clone()));
        }
    }

    body
}

/// Perform an OpenAI compatible request.
pub async fn perform_openai_compatible_request(
    client: &Client,
    body: &Value,
    request_options: &RequestOptions,
    client_options: &OpenAICompatibleClientOptions,
) -> Result<(reqwest::Response, String), MossenAPIError> {
    let url = format!("{}/chat/completions", client_options.base_url.trim_end_matches('/'));

    let headers = build_request_headers(
        &client_options.default_headers,
        request_options.headers.as_ref(),
    );

    let timeout_ms = request_options.timeout.unwrap_or(client_options.timeout_ms);

    let response = client
        .post(&url)
        .headers(headers)
        .json(body)
        .timeout(Duration::from_millis(timeout_ms))
        .send()
        .await
        .map_err(|e| MossenAPIError::new(
            0,
            serde_json::json!({"error": {"message": e.to_string()}}),
            Some(e.to_string()),
            HeaderMap::new(),
        ))?;

    let request_id = get_request_id(response.headers());

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let resp_headers = response.headers().clone();
        let error_body: Value = response
            .json()
            .await
            .unwrap_or(serde_json::json!({"error": {"message": "Unknown error"}}));
        let message = error_body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("API error")
            .to_string();
        return Err(MossenAPIError::new(status, error_body, Some(message), resp_headers));
    }

    Ok((response, request_id))
}

/// Create an OpenAI compatible client.
/// This returns a struct that mimics the Mossen SDK interface.
pub struct OpenAICompatibleClient {
    client: Client,
    options: OpenAICompatibleClientOptions,
}

impl OpenAICompatibleClient {
    pub fn new(client: Client, options: OpenAICompatibleClientOptions) -> Self {
        Self { client, options }
    }

    /// Create a non-streaming message.
    pub async fn create_message(
        &self,
        params: &Value,
        request_options: &RequestOptions,
    ) -> Result<MossenBetaMessage, MossenAPIError> {
        let body = create_openai_request_body(params, false);
        let (response, _request_id) =
            perform_openai_compatible_request(&self.client, &body, request_options, &self.options)
                .await?;

        let completion: OpenAIChatCompletionResponse = response.json().await.map_err(|e| {
            MossenAPIError::new(
                0,
                serde_json::json!({"error": {"message": e.to_string()}}),
                Some(e.to_string()),
                HeaderMap::new(),
            )
        })?;

        Ok(completion_to_mossen_message(
            &completion,
            params
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or(""),
        ))
    }

    /// Create a streaming message.
    pub async fn create_message_stream(
        &self,
        params: &Value,
        request_options: &RequestOptions,
    ) -> Result<(reqwest::Response, String), MossenAPIError> {
        let body = create_openai_request_body(params, true);
        perform_openai_compatible_request(&self.client, &body, request_options, &self.options).await
    }
}

/// Convert an OpenAI completion to a Mossen message.
pub fn completion_to_mossen_message(
    data: &OpenAIChatCompletionResponse,
    fallback_model: &str,
) -> MossenBetaMessage {
    let choice = data.choices.as_ref().and_then(|c| c.first());
    let model = data.model.as_deref().unwrap_or(fallback_model);
    let id = data.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut content: Vec<ContentBlock> = Vec::new();
    let mut has_tool_calls = false;

    if let Some(choice) = choice {
        // Get text content
        let text_content = choice
            .message
            .as_ref()
            .and_then(|m| m.content.as_ref())
            .map(|c| flatten_text_content(c))
            .unwrap_or_default();

        if !text_content.is_empty() {
            content.push(ContentBlock::Text { text: text_content });
        }

        // Get tool calls
        let tool_calls = choice
            .message
            .as_ref()
            .and_then(|m| m.tool_calls.as_ref())
            .cloned()
            .unwrap_or_default();

        for tc in &tool_calls {
            has_tool_calls = true;
            let id = tc.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
            let name = tc
                .function
                .as_ref()
                .and_then(|f| f.name.as_ref())
                .cloned()
                .unwrap_or_default();
            let arguments = tc
                .function
                .as_ref()
                .and_then(|f| f.arguments.as_ref())
                .and_then(|a| serde_json::from_str(a).ok())
                .unwrap_or(Value::Object(Default::default()));

            content.push(ContentBlock::ToolUse {
                id,
                name,
                input: arguments,
            });
        }

        let finish_reason = choice.finish_reason.as_deref();
        let stop_reason = map_stop_reason(finish_reason, has_tool_calls);

        MossenBetaMessage {
            id,
            content,
            model: model.to_string(),
            role: "assistant".to_string(),
            stop_reason: Some(stop_reason),
            stop_sequence: None,
            message_type: "message".to_string(),
            usage: to_mossen_usage(data.usage.as_ref()),
        }
    } else {
        MossenBetaMessage {
            id,
            content,
            model: model.to_string(),
            role: "assistant".to_string(),
            stop_reason: Some("end_turn".to_string()),
            stop_sequence: None,
            message_type: "message".to_string(),
            usage: to_mossen_usage(data.usage.as_ref()),
        }
    }
}

/// Create the OpenAI compatible client (factory function matching TS export).
pub fn create_openai_compatible_client(
    client: Client,
    options: OpenAICompatibleClientOptions,
) -> OpenAICompatibleClient {
    OpenAICompatibleClient::new(client, options)
}
