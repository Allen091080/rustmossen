//! Mossen parity bridge — bridges canonical types to Mossen API format.

use serde_json::Value;
use super::canonical::{CanonicalStopReason, CanonicalStreamEvent, CanonicalUsage};

/// Convert a Mossen SSE stream event into a canonical stream event.
pub fn map_mossen_sse_to_canonical(event_type: &str, data: &Value) -> Option<CanonicalStreamEvent> {
    match event_type {
        "message_start" => {
            let message_id = data
                .pointer("/message/id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let model = data
                .pointer("/message/model")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(CanonicalStreamEvent::MessageStart { message_id, model })
        }
        "content_block_start" => {
            let block_type = data
                .pointer("/content_block/type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match block_type {
                "thinking" => Some(CanonicalStreamEvent::ThinkingStart),
                "text" => Some(CanonicalStreamEvent::TextStart),
                "tool_use" => {
                    let id = data
                        .pointer("/content_block/id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = data
                        .pointer("/content_block/name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(CanonicalStreamEvent::ToolUseStart { id, name })
                }
                _ => None,
            }
        }
        "content_block_delta" => {
            let delta_type = data
                .pointer("/delta/type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match delta_type {
                "thinking_delta" => {
                    let text = data
                        .pointer("/delta/thinking")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(CanonicalStreamEvent::ThinkingDelta { text })
                }
                "text_delta" => {
                    let text = data
                        .pointer("/delta/text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(CanonicalStreamEvent::TextDelta { text })
                }
                "input_json_delta" => {
                    let partial_json = data
                        .pointer("/delta/partial_json")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                    Some(CanonicalStreamEvent::ToolUseArgsDelta {
                        id: format!("block_{}", index),
                        partial_json,
                    })
                }
                _ => None,
            }
        }
        "content_block_stop" => {
            let block_type = data
                .pointer("/content_block/type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match block_type {
                "thinking" => Some(CanonicalStreamEvent::ThinkingEnd),
                "text" => Some(CanonicalStreamEvent::TextEnd),
                "tool_use" => {
                    let id = data
                        .pointer("/content_block/id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(CanonicalStreamEvent::ToolUseEnd { id })
                }
                _ => None,
            }
        }
        "message_delta" => {
            let stop_reason_str = data
                .pointer("/delta/stop_reason")
                .and_then(|v| v.as_str())
                .unwrap_or("end_turn");
            let stop_reason = super::canonical::canonical_stop_reason_from_mossen(stop_reason_str);
            let input_tokens = data
                .pointer("/usage/input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output_tokens = data
                .pointer("/usage/output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            Some(CanonicalStreamEvent::MessageStop {
                stop_reason,
                usage: CanonicalUsage {
                    input_tokens,
                    output_tokens,
                },
            })
        }
        "error" => {
            let error = data
                .pointer("/error/message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            Some(CanonicalStreamEvent::ProviderError { error })
        }
        _ => None,
    }
}
