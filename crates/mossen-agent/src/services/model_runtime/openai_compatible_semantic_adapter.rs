//! OpenAI-compatible semantic adapter — adapts canonical types for OpenAI-compatible APIs.

use super::canonical::{
    AssistantToolRequest, CanonicalHistoryMessage, CanonicalStopReason, CanonicalStreamEvent,
    CanonicalTurnRequest, CanonicalUsage, MessageRole,
};
use serde_json::Value;

/// Convert a canonical turn request to OpenAI API format.
pub fn to_openai_api_request(request: &CanonicalTurnRequest) -> Value {
    let mut messages = Vec::new();

    // Add system message
    if let Some(ref system) = request.system {
        messages.push(serde_json::json!({
            "role": "system",
            "content": system
        }));
    }

    for msg in &request.messages {
        match msg.role {
            MessageRole::User => {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": msg.content
                }));
            }
            MessageRole::Assistant => {
                let mut assistant_msg = serde_json::json!({
                    "role": "assistant",
                    "content": if msg.content.is_empty() { Value::Null } else { Value::String(msg.content.clone()) }
                });
                if let Some(ref tool_calls) = msg.tool_calls {
                    let tc_array: Vec<Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": serde_json::to_string(&tc.arguments_object).unwrap_or_default()
                                }
                            })
                        })
                        .collect();
                    assistant_msg["tool_calls"] = Value::Array(tc_array);
                }
                messages.push(assistant_msg);
            }
            MessageRole::Tool => {
                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": msg.tool_call_id,
                    "content": msg.content
                }));
            }
            MessageRole::System => {
                messages.push(serde_json::json!({
                    "role": "system",
                    "content": msg.content
                }));
            }
        }
    }

    let mut body = serde_json::json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "messages": messages,
        "stream": true
    });

    if let Some(ref tools) = request.tools {
        let openai_tools: Vec<Value> = tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": tool
                })
            })
            .collect();
        body["tools"] = Value::Array(openai_tools);
    }
    if let Some(temp) = request.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(ref stop) = request.stop {
        body["stop"] = serde_json::json!(stop);
    }

    body
}

/// Map OpenAI streaming chunk to canonical stream event.
pub fn map_openai_chunk_to_canonical(chunk: &Value) -> Vec<CanonicalStreamEvent> {
    let mut events = Vec::new();

    let choices = match chunk.get("choices").and_then(|v| v.as_array()) {
        Some(c) => c,
        None => return events,
    };

    for choice in choices {
        let delta = match choice.get("delta") {
            Some(d) => d,
            None => continue,
        };

        // Text content
        if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
            if !content.is_empty() {
                events.push(CanonicalStreamEvent::TextDelta {
                    text: content.to_string(),
                });
            }
        }

        // Tool calls
        if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in tool_calls {
                let id = tc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let function = tc.get("function");

                if let Some(name) = function
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                {
                    if !name.is_empty() {
                        events.push(CanonicalStreamEvent::ToolUseStart {
                            id: id.clone(),
                            name: name.to_string(),
                        });
                    }
                }

                if let Some(args) = function
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                {
                    if !args.is_empty() {
                        events.push(CanonicalStreamEvent::ToolUseArgsDelta {
                            id: id.clone(),
                            partial_json: args.to_string(),
                        });
                    }
                }
            }
        }

        // Finish reason
        if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
            let stop_reason = match finish_reason {
                "tool_calls" => CanonicalStopReason::ToolUse,
                "length" => CanonicalStopReason::MaxTokens,
                "stop" => CanonicalStopReason::EndTurn,
                "content_filter" => CanonicalStopReason::Refusal,
                _ => CanonicalStopReason::EndTurn,
            };

            let usage = chunk
                .get("usage")
                .map(|u| CanonicalUsage {
                    input_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    output_tokens: u
                        .get("completion_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                })
                .unwrap_or_default();

            events.push(CanonicalStreamEvent::MessageStop { stop_reason, usage });
        }
    }

    events
}
