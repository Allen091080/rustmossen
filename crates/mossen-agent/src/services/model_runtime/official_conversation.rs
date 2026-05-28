//! Official conversation — tool request/result extraction and message content utilities.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use super::canonical::AssistantToolRequest;

/// Official assistant tool request (with input and type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialAssistantToolRequest {
    pub arguments_object: HashMap<String, Value>,
    pub id: String,
    pub input: HashMap<String, Value>,
    pub name: String,
    #[serde(rename = "type")]
    pub block_type: String,
}

/// Official tool result block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialToolResultBlock {
    pub content: Value,
    pub is_error: Option<bool>,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: String,
}

/// An assistant tool round with text, requests, and results.
#[derive(Debug, Clone)]
pub struct OfficialAssistantToolRound {
    pub assistant_visible_text: Option<String>,
    pub tool_requests: Vec<OfficialAssistantToolRequest>,
    pub tool_results_by_tool_use_id: HashMap<String, OfficialToolResultBlock>,
}

/// Extract assistant tool requests from content blocks.
pub fn extract_assistant_tool_requests(content: &Value) -> Vec<OfficialAssistantToolRequest> {
    let arr = match content.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };

    let mut results = Vec::new();
    for block in arr {
        let obj = match block.as_object() {
            Some(o) => o,
            None => continue,
        };
        if obj.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
            continue;
        }
        let id = match obj.get("id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let name = match obj.get("name").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let input = match obj.get("input") {
            Some(v) if v.is_object() => {
                serde_json::from_value::<HashMap<String, Value>>(v.clone()).unwrap_or_default()
            }
            _ => continue,
        };

        results.push(OfficialAssistantToolRequest {
            arguments_object: input.clone(),
            id,
            input,
            name,
            block_type: "tool_use".to_string(),
        });
    }
    results
}

/// Count assistant tool requests in content.
pub fn count_assistant_tool_requests(content: &Value) -> usize {
    extract_assistant_tool_requests(content).len()
}

/// Check if content has any tool requests.
pub fn has_assistant_tool_requests(content: &Value) -> bool {
    count_assistant_tool_requests(content) > 0
}

/// Find a tool request by name.
pub fn find_assistant_tool_request_by_name(
    content: &Value,
    tool_name: &str,
) -> Option<OfficialAssistantToolRequest> {
    extract_assistant_tool_requests(content)
        .into_iter()
        .find(|r| r.name == tool_name)
}

/// Check if content has a tool request with a given name.
pub fn has_assistant_tool_request_named(content: &Value, tool_name: &str) -> bool {
    find_assistant_tool_request_by_name(content, tool_name).is_some()
}

/// Extract tool request IDs from content.
pub fn extract_assistant_tool_request_ids(content: &Value) -> Vec<String> {
    extract_assistant_tool_requests(content)
        .into_iter()
        .map(|r| r.id)
        .collect()
}

/// Count tool requests across multiple messages.
pub fn count_assistant_tool_requests_in_messages(messages: &[Value]) -> usize {
    messages.iter().fold(0, |total, msg| {
        let content = msg.pointer("/message/content").unwrap_or(&Value::Null);
        total + count_assistant_tool_requests(content)
    })
}

/// Extract tool requests from multiple messages.
pub fn extract_assistant_tool_requests_in_messages(
    messages: &[Value],
) -> Vec<OfficialAssistantToolRequest> {
    messages
        .iter()
        .flat_map(|msg| {
            let content = msg.pointer("/message/content").unwrap_or(&Value::Null);
            extract_assistant_tool_requests(content)
        })
        .collect()
}

/// Find the most recent tool request by name across messages.
pub fn find_most_recent_assistant_tool_request(
    messages: &[Value],
    tool_name: &str,
) -> Option<OfficialAssistantToolRequest> {
    for msg in messages.iter().rev() {
        let content = msg.pointer("/message/content").unwrap_or(&Value::Null);
        if let Some(req) = find_assistant_tool_request_by_name(content, tool_name) {
            return Some(req);
        }
    }
    None
}

/// Check if any message has a tool request with the given name.
pub fn has_assistant_tool_request_named_in_messages(messages: &[Value], tool_name: &str) -> bool {
    find_most_recent_assistant_tool_request(messages, tool_name).is_some()
}

/// Extract official tool result IDs from content.
pub fn extract_official_tool_result_ids(content: &Value) -> Vec<String> {
    extract_official_tool_result_blocks(content)
        .into_iter()
        .map(|b| b.tool_use_id)
        .collect()
}

/// Extract official tool result blocks from content.
pub fn extract_official_tool_result_blocks(content: &Value) -> Vec<OfficialToolResultBlock> {
    let arr = match content.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };

    arr.iter()
        .filter_map(|block| {
            if is_official_tool_result_block(block) {
                Some(OfficialToolResultBlock {
                    content: block.get("content").cloned().unwrap_or(Value::Null),
                    is_error: block.get("is_error").and_then(|v| v.as_bool()),
                    tool_use_id: block
                        .get("tool_use_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    block_type: "tool_result".to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Check if content has any tool result blocks.
pub fn has_official_tool_result_blocks(content: &Value) -> bool {
    !extract_official_tool_result_blocks(content).is_empty()
}

/// Strip tool result blocks from content.
pub fn strip_official_tool_result_blocks(content: &Value) -> Value {
    let arr = match content.as_array() {
        Some(a) => a,
        None => return content.clone(),
    };

    let stripped: Vec<&Value> = arr
        .iter()
        .filter(|block| !is_official_tool_result_block(block))
        .collect();

    if stripped.len() == arr.len() {
        content.clone()
    } else {
        Value::Array(stripped.into_iter().cloned().collect())
    }
}

/// Find a tool result in messages by tool_use_id.
pub fn find_official_tool_result_in_messages(
    messages: &[Value],
    tool_use_id: &str,
) -> Option<OfficialToolResultBlock> {
    for msg in messages {
        if msg.get("type").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }
        let content = msg.pointer("/message/content").unwrap_or(&Value::Null);
        if let Some(block) = find_official_tool_result_block(content, tool_use_id) {
            return Some(block);
        }
    }
    None
}

/// Extract visible text from assistant content blocks.
pub fn extract_assistant_visible_text(content: &Value) -> String {
    let arr = match content.as_array() {
        Some(a) => a,
        None => return String::new(),
    };

    arr.iter()
        .filter_map(|block| {
            let obj = block.as_object()?;
            if obj.get("type")?.as_str()? == "text" {
                obj.get("text")?.as_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Find the most recent assistant visible text across messages.
pub fn find_most_recent_assistant_visible_text_in_messages(messages: &[Value]) -> Option<String> {
    for msg in messages.iter().rev() {
        let content = msg.pointer("/message/content").unwrap_or(&Value::Null);
        let text = extract_assistant_visible_text(content);
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

/// Build an official assistant tool round from messages.
pub fn build_official_assistant_tool_round(
    assistant_messages: &[Value],
    tool_result_messages: &[Value],
) -> OfficialAssistantToolRound {
    let tool_requests = extract_assistant_tool_requests_in_messages(assistant_messages);
    let mut tool_results_by_tool_use_id: HashMap<String, OfficialToolResultBlock> = HashMap::new();

    for req in &tool_requests {
        if let Some(result) = find_official_tool_result_in_messages(tool_result_messages, &req.id) {
            tool_results_by_tool_use_id.insert(req.id.clone(), result);
        }
    }

    OfficialAssistantToolRound {
        assistant_visible_text: find_most_recent_assistant_visible_text_in_messages(
            assistant_messages,
        ),
        tool_requests,
        tool_results_by_tool_use_id,
    }
}

/// Create an official tool result block.
pub fn create_official_tool_result_block(
    tool_use_id: &str,
    content: Value,
    is_error: bool,
) -> OfficialToolResultBlock {
    OfficialToolResultBlock {
        content,
        is_error: if is_error { Some(true) } else { None },
        tool_use_id: tool_use_id.to_string(),
        block_type: "tool_result".to_string(),
    }
}

/// Check if a block is a tool result block.
pub fn is_official_tool_result_block(block: &Value) -> bool {
    let obj = match block.as_object() {
        Some(o) => o,
        None => return false,
    };
    obj.get("type").and_then(|v| v.as_str()) == Some("tool_result")
        && obj.get("tool_use_id").and_then(|v| v.as_str()).is_some()
}

/// Find a specific tool result block by tool_use_id.
pub fn find_official_tool_result_block(
    content: &Value,
    tool_use_id: &str,
) -> Option<OfficialToolResultBlock> {
    extract_official_tool_result_blocks(content)
        .into_iter()
        .find(|b| b.tool_use_id == tool_use_id)
}

/// Find the first tool result block in content.
pub fn find_first_official_tool_result_block(content: &Value) -> Option<OfficialToolResultBlock> {
    extract_official_tool_result_blocks(content)
        .into_iter()
        .next()
}

/// Map tool request inputs using a mapper function.
pub fn map_assistant_tool_request_inputs<F>(content: &Value, mapper: F) -> Value
where
    F: Fn(&OfficialAssistantToolRequest) -> HashMap<String, Value>,
{
    let arr = match content.as_array() {
        Some(a) => a,
        None => return content.clone(),
    };

    let mut modified = false;
    let mut result: Vec<Value> = arr.clone();

    for (i, block) in arr.iter().enumerate() {
        let requests = extract_assistant_tool_requests(&Value::Array(vec![block.clone()]));
        if let Some(req) = requests.first() {
            let mapped_input = mapper(req);
            if mapped_input != req.input {
                let mut obj = block.as_object().unwrap().clone();
                obj.insert(
                    "input".to_string(),
                    serde_json::to_value(&mapped_input)
                        .unwrap_or(Value::Object(Default::default())),
                );
                result[i] = Value::Object(obj);
                modified = true;
            }
        }
    }

    if modified {
        Value::Array(result)
    } else {
        content.clone()
    }
}

/// A tool request paired with its source message. Mirrors TS
/// `type AssistantToolRequestWithMessage<TMessage> = { message, toolRequest }`.
#[derive(Debug, Clone)]
pub struct AssistantToolRequestWithMessage {
    pub message: Value,
    pub tool_request: OfficialAssistantToolRequest,
}

/// Flatten messages into `(message, tool_request)` pairs. Mirrors TS
/// `extractAssistantToolRequestsWithMessages`.
pub fn extract_assistant_tool_requests_with_messages(
    messages: &[Value],
) -> Vec<AssistantToolRequestWithMessage> {
    let mut out = Vec::new();
    for message in messages {
        // Skip null/undefined entries (rendered as JSON null in the Value form).
        if message.is_null() {
            continue;
        }
        let content = message
            .get("message")
            .and_then(|m| m.get("content"))
            .cloned()
            .unwrap_or(Value::Null);
        for tool_request in extract_assistant_tool_requests(&content) {
            out.push(AssistantToolRequestWithMessage {
                message: message.clone(),
                tool_request,
            });
        }
    }
    out
}
