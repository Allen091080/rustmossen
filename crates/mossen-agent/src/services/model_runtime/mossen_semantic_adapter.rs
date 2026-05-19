//! Mossen semantic adapter — adapts canonical types for Mossen/Claude API.

use super::canonical::{
    AssistantToolRequest, CanonicalHistoryMessage, CanonicalStreamEvent, CanonicalTurnRequest,
    CanonicalUsage, MessageRole,
};
use serde_json::Value;

/// Convert a canonical turn request to Mossen API format.
pub fn to_mossen_api_request(request: &CanonicalTurnRequest) -> Value {
    let mut messages = Vec::new();

    for msg in &request.messages {
        match msg.role {
            MessageRole::User => {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": msg.content
                }));
            }
            MessageRole::Assistant => {
                let mut content = Vec::new();
                if let Some(ref thinking) = msg.thinking {
                    content.push(serde_json::json!({
                        "type": "thinking",
                        "thinking": thinking
                    }));
                }
                if !msg.content.is_empty() {
                    content.push(serde_json::json!({
                        "type": "text",
                        "text": msg.content
                    }));
                }
                if let Some(ref tool_calls) = msg.tool_calls {
                    for tc in tool_calls {
                        content.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments_object
                        }));
                    }
                }
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": content
                }));
            }
            MessageRole::Tool => {
                let tool_result = serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": msg.tool_call_id,
                    "content": msg.content,
                    "is_error": msg.is_error.unwrap_or(false)
                });
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": [tool_result]
                }));
            }
            MessageRole::System => {
                // System messages are handled via the system field
            }
        }
    }

    let mut body = serde_json::json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "messages": messages,
        "stream": true
    });

    if let Some(ref system) = request.system {
        body["system"] = Value::String(system.clone());
    }
    if let Some(ref tools) = request.tools {
        body["tools"] = Value::Array(tools.clone());
    }
    if let Some(temp) = request.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(ref reasoning) = request.reasoning {
        if reasoning.enabled {
            let mut thinking = serde_json::json!({"type": "enabled"});
            if let Some(budget) = reasoning.budget_tokens {
                thinking["budget_tokens"] = serde_json::json!(budget);
            }
            body["thinking"] = thinking;
        }
    }
    if let Some(ref stop) = request.stop {
        body["stop_sequences"] = serde_json::json!(stop);
    }
    if let Some(ref tool_choice) = request.tool_choice {
        body["tool_choice"] = tool_choice.clone();
    }

    body
}

/// Extract usage from a Mossen API response.
pub fn extract_usage_from_response(response: &Value) -> CanonicalUsage {
    CanonicalUsage {
        input_tokens: response
            .pointer("/usage/input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output_tokens: response
            .pointer("/usage/output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    }
}

/// Extract tool requests from a Mossen API response content blocks.
pub fn extract_tool_requests(content_blocks: &[Value]) -> Vec<AssistantToolRequest> {
    content_blocks
        .iter()
        .filter(|block| block.get("type").and_then(|v| v.as_str()) == Some("tool_use"))
        .map(|block| {
            let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let input = block.get("input").cloned().unwrap_or(Value::Object(Default::default()));
            let arguments_object = match input {
                Value::Object(map) => map
                    .into_iter()
                    .map(|(k, v)| (k, v))
                    .collect(),
                _ => std::collections::HashMap::new(),
            };
            AssistantToolRequest {
                arguments_object,
                id,
                name,
            }
        })
        .collect()
}
