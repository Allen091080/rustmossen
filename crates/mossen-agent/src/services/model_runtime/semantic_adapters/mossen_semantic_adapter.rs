//! Mossen semantic adapter — converts Mossen API events to/from canonical events.

use std::collections::HashMap;
use serde_json::Value;

use crate::services::model_runtime::canonical::{
    AssistantToolRequest, CanonicalStopReason, CanonicalStreamEvent, CanonicalTurnResult,
    CanonicalUsage, OfficialSemanticCapabilities, ThinkingParityStrategy, ToolCallArgsEncoding,
    ToolResultRoleStyle, canonical_stop_reason_from_mossen,
};

/// Mossen semantic capabilities constant.
pub const MOSSEN_SEMANTIC_CAPABILITIES: OfficialSemanticCapabilities = OfficialSemanticCapabilities {
    mixed_content_tool_use: true,
    native_thinking_blocks: true,
    reasoning_budget: true,
    streaming_tool_arg_deltas: true,
    structured_stop_reasons: true,
    supports_assistant_prelude_before_tool_use: true,
    tool_call_args_encoding: ToolCallArgsEncoding::Object,
    tool_result_role_style: ToolResultRoleStyle::MossenUserToolResult,
};

/// Convert a Mossen message to canonical turn result.
pub fn mossen_message_to_canonical_turn(message: &Value) -> CanonicalTurnResult {
    let content = message.get("content").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let stop_reason_str = message.get("stop_reason").and_then(|v| v.as_str());

    let thinking_text: String = content
        .iter()
        .filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("thinking"))
        .filter_map(|b| b.get("thinking").and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join("");

    let visible_text: String = content
        .iter()
        .filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("text"))
        .filter_map(|b| b.get("text").and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join("");

    let tool_requests: Vec<AssistantToolRequest> = content
        .iter()
        .filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("tool_use"))
        .filter_map(|b| {
            let id = b.get("id")?.as_str()?.to_string();
            let name = b.get("name")?.as_str()?.to_string();
            let input = b.get("input")?;
            let arguments_object: HashMap<String, Value> =
                serde_json::from_value(input.clone()).unwrap_or_default();
            Some(AssistantToolRequest {
                arguments_object,
                id,
                name,
            })
        })
        .collect();

    let usage = message.get("usage").map(|u| CanonicalUsage {
        input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
    }).unwrap_or_default();

    CanonicalTurnResult {
        provider_diagnostics: None,
        stop_reason: canonical_stop_reason_from_mossen(stop_reason_str),
        thinking_text,
        tool_requests,
        usage,
        visible_text,
    }
}

/// State machine for converting Mossen raw stream events to canonical events.
pub struct MossenSemanticEventState {
    tool_use_ids: HashMap<usize, String>,
    content_block_types: HashMap<usize, String>,
}

impl MossenSemanticEventState {
    pub fn new() -> Self {
        Self {
            tool_use_ids: HashMap::new(),
            content_block_types: HashMap::new(),
        }
    }

    /// Consume a Mossen raw stream event and produce canonical events.
    pub fn consume(&mut self, event: &Value) -> Vec<CanonicalStreamEvent> {
        let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "message_start" => {
                let msg = event.get("message").unwrap_or(&Value::Null);
                let message_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let model = msg.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
                vec![CanonicalStreamEvent::MessageStart { message_id, model }]
            }
            "content_block_start" => {
                let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let block = event.get("content_block").unwrap_or(&Value::Null);
                let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                self.content_block_types.insert(index, block_type.to_string());

                match block_type {
                    "thinking" => vec![CanonicalStreamEvent::ThinkingStart],
                    "text" => vec![CanonicalStreamEvent::TextStart],
                    "tool_use" => {
                        let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        self.tool_use_ids.insert(index, id.clone());
                        vec![CanonicalStreamEvent::ToolUseStart { id, name }]
                    }
                    _ => Vec::new(),
                }
            }
            "content_block_delta" => {
                let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let delta = event.get("delta").unwrap_or(&Value::Null);
                let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match delta_type {
                    "thinking_delta" => {
                        let text = delta.get("thinking").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        vec![CanonicalStreamEvent::ThinkingDelta { text }]
                    }
                    "text_delta" => {
                        let text = delta.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        vec![CanonicalStreamEvent::TextDelta { text }]
                    }
                    "input_json_delta" => {
                        if let Some(tool_id) = self.tool_use_ids.get(&index) {
                            let partial_json = delta.get("partial_json").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            vec![CanonicalStreamEvent::ToolUseArgsDelta {
                                id: tool_id.clone(),
                                partial_json,
                            }]
                        } else {
                            Vec::new()
                        }
                    }
                    _ => Vec::new(),
                }
            }
            "content_block_stop" => {
                let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let block_type = self.content_block_types.get(&index).map(|s| s.as_str()).unwrap_or("");

                match block_type {
                    "thinking" => vec![CanonicalStreamEvent::ThinkingEnd],
                    "text" => vec![CanonicalStreamEvent::TextEnd],
                    "tool_use" => {
                        if let Some(tool_id) = self.tool_use_ids.remove(&index) {
                            vec![CanonicalStreamEvent::ToolUseEnd { id: tool_id }]
                        } else {
                            Vec::new()
                        }
                    }
                    _ => Vec::new(),
                }
            }
            "message_delta" => {
                let delta = event.get("delta").unwrap_or(&Value::Null);
                let stop_reason_str = delta.get("stop_reason").and_then(|v| v.as_str());
                let usage_obj = event.get("usage").unwrap_or(&Value::Null);
                let input_tokens = usage_obj.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output_tokens = usage_obj.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

                vec![CanonicalStreamEvent::MessageStop {
                    stop_reason: canonical_stop_reason_from_mossen(stop_reason_str),
                    usage: CanonicalUsage { input_tokens, output_tokens },
                }]
            }
            _ => Vec::new(),
        }
    }
}

impl Default for MossenSemanticEventState {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a single Mossen event to canonical events.
pub fn mossen_event_to_canonical_events(event: &Value) -> Vec<CanonicalStreamEvent> {
    MossenSemanticEventState::new().consume(event)
}
