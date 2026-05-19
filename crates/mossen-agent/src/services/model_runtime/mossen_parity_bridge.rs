//! Mossen parity bridge — converts canonical types to Mossen API types and back.

use std::collections::HashMap;
use serde_json::Value;
use uuid::Uuid;

use super::canonical::{
    AssistantToolRequest, CanonicalStopReason, CanonicalStreamEvent, CanonicalTurnResult,
    CanonicalUsage,
};

/// Mossen beta message content block.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum MossenContentBlock {
    #[serde(rename = "thinking")]
    Thinking { thinking: String, signature: String },
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        input: HashMap<String, Value>,
        name: String,
    },
}

/// Mossen beta usage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MossenBetaUsage {
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Mossen beta message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MossenBetaMessage {
    pub content: Vec<MossenContentBlock>,
    pub id: String,
    pub model: String,
    pub role: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub usage: MossenBetaUsage,
}

fn to_mossen_usage(usage: &CanonicalUsage) -> MossenBetaUsage {
    MossenBetaUsage {
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
    }
}

fn to_mossen_stop_reason(stop_reason: CanonicalStopReason) -> Option<String> {
    let s = match stop_reason {
        CanonicalStopReason::Compaction => "compaction",
        CanonicalStopReason::MaxTokens => "max_tokens",
        CanonicalStopReason::PauseTurn => "pause_turn",
        CanonicalStopReason::Refusal => "refusal",
        CanonicalStopReason::StopSequence => "stop_sequence",
        CanonicalStopReason::ToolUse => "tool_use",
        CanonicalStopReason::EndTurn => "end_turn",
    };
    Some(s.to_string())
}

/// Convert a canonical turn result into a Mossen beta message.
pub fn canonical_turn_to_mossen_message(
    result: &CanonicalTurnResult,
    fallback_model: &str,
    message_id: Option<&str>,
) -> MossenBetaMessage {
    let mut content: Vec<MossenContentBlock> = Vec::new();
    let mid = message_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("msg_{}", Uuid::new_v4()));

    if !result.thinking_text.is_empty() {
        content.push(MossenContentBlock::Thinking {
            signature: format!("synthetic-thinking:{}", mid),
            thinking: result.thinking_text.clone(),
        });
    }

    if !result.visible_text.is_empty() {
        content.push(MossenContentBlock::Text {
            text: result.visible_text.clone(),
        });
    }

    for tool_request in &result.tool_requests {
        content.push(MossenContentBlock::ToolUse {
            id: tool_request.id.clone(),
            input: tool_request.arguments_object.clone(),
            name: tool_request.name.clone(),
        });
    }

    if content.is_empty() {
        content.push(MossenContentBlock::Text {
            text: String::new(),
        });
    }

    MossenBetaMessage {
        content,
        id: mid,
        model: fallback_model.to_string(),
        role: "assistant".to_string(),
        stop_reason: to_mossen_stop_reason(result.stop_reason),
        stop_sequence: None,
        msg_type: "message".to_string(),
        usage: to_mossen_usage(&result.usage),
    }
}

/// State machine for converting canonical stream events to Mossen raw stream events.
pub struct MossenParityEventState {
    next_index: usize,
    open_text_index: Option<usize>,
    open_thinking_index: Option<usize>,
    tool_indices: HashMap<String, usize>,
}

impl MossenParityEventState {
    pub fn new() -> Self {
        Self {
            next_index: 0,
            open_text_index: None,
            open_thinking_index: None,
            tool_indices: HashMap::new(),
        }
    }

    /// Consume canonical stream events and produce Mossen raw stream events.
    pub fn consume(&mut self, events: &[CanonicalStreamEvent]) -> Vec<Value> {
        let mut emitted: Vec<Value> = Vec::new();

        for event in events {
            match event {
                CanonicalStreamEvent::MessageStart { message_id, model } => {
                    emitted.push(serde_json::json!({
                        "type": "message_start",
                        "message": {
                            "content": [],
                            "id": message_id,
                            "model": model,
                            "role": "assistant",
                            "stop_reason": null,
                            "stop_sequence": null,
                            "type": "message",
                            "usage": { "input_tokens": 0, "output_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0 }
                        }
                    }));
                }
                CanonicalStreamEvent::ThinkingStart => {
                    let idx = self.next_index;
                    self.next_index += 1;
                    self.open_thinking_index = Some(idx);
                    emitted.push(serde_json::json!({
                        "type": "content_block_start",
                        "index": idx,
                        "content_block": { "type": "thinking", "thinking": "", "signature": "" }
                    }));
                }
                CanonicalStreamEvent::ThinkingDelta { text } => {
                    if let Some(idx) = self.open_thinking_index {
                        emitted.push(serde_json::json!({
                            "type": "content_block_delta",
                            "index": idx,
                            "delta": { "type": "thinking_delta", "thinking": text }
                        }));
                    }
                }
                CanonicalStreamEvent::ThinkingEnd => {
                    if let Some(idx) = self.open_thinking_index.take() {
                        emitted.push(serde_json::json!({
                            "type": "content_block_stop",
                            "index": idx
                        }));
                    }
                }
                CanonicalStreamEvent::TextStart => {
                    let idx = self.next_index;
                    self.next_index += 1;
                    self.open_text_index = Some(idx);
                    emitted.push(serde_json::json!({
                        "type": "content_block_start",
                        "index": idx,
                        "content_block": { "type": "text", "text": "" }
                    }));
                }
                CanonicalStreamEvent::TextDelta { text } => {
                    if let Some(idx) = self.open_text_index {
                        emitted.push(serde_json::json!({
                            "type": "content_block_delta",
                            "index": idx,
                            "delta": { "type": "text_delta", "text": text }
                        }));
                    }
                }
                CanonicalStreamEvent::TextEnd => {
                    if let Some(idx) = self.open_text_index.take() {
                        emitted.push(serde_json::json!({
                            "type": "content_block_stop",
                            "index": idx
                        }));
                    }
                }
                CanonicalStreamEvent::ToolUseStart { id, name } => {
                    let idx = self.next_index;
                    self.next_index += 1;
                    self.tool_indices.insert(id.clone(), idx);
                    emitted.push(serde_json::json!({
                        "type": "content_block_start",
                        "index": idx,
                        "content_block": { "type": "tool_use", "id": id, "name": name, "input": {} }
                    }));
                }
                CanonicalStreamEvent::ToolUseArgsDelta { id, partial_json } => {
                    if let Some(&idx) = self.tool_indices.get(id) {
                        emitted.push(serde_json::json!({
                            "type": "content_block_delta",
                            "index": idx,
                            "delta": { "type": "input_json_delta", "partial_json": partial_json }
                        }));
                    }
                }
                CanonicalStreamEvent::ToolUseEnd { id } => {
                    if let Some(idx) = self.tool_indices.remove(id) {
                        emitted.push(serde_json::json!({
                            "type": "content_block_stop",
                            "index": idx
                        }));
                    }
                }
                CanonicalStreamEvent::MessageStop { stop_reason, usage } => {
                    emitted.push(serde_json::json!({
                        "type": "message_delta",
                        "delta": {
                            "stop_reason": to_mossen_stop_reason(*stop_reason),
                            "stop_sequence": null
                        },
                        "usage": { "input_tokens": usage.input_tokens, "output_tokens": usage.output_tokens }
                    }));
                    emitted.push(serde_json::json!({ "type": "message_stop" }));
                }
                CanonicalStreamEvent::ProviderError { .. } => {}
            }
        }

        emitted
    }
}

impl Default for MossenParityEventState {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a list of canonical events into Mossen raw events (non-streaming).
pub fn canonical_events_to_mossen_event_list(events: &[CanonicalStreamEvent]) -> Vec<Value> {
    MossenParityEventState::new().consume(events)
}
