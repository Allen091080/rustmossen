//! Text-mode mirror of `tools/MCPTool/UI.tsx`.

use mossen_utils::string_utils::truncate_chars;
use serde_json::Value;

/// `UI.tsx` `renderToolUseMessage`.
pub fn render_tool_use_message(tool_name: Option<&str>, input: &Value) -> String {
    let name = tool_name.unwrap_or("mcp_tool");
    let body = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
    format!("{} {}", name, body)
}

/// `UI.tsx` `renderToolUseProgressMessage`.
pub fn render_tool_use_progress_message(elapsed_ms: u64) -> String {
    format!("MCP tool running... ({}ms)", elapsed_ms)
}

/// `UI.tsx` `renderToolResultMessage`.
pub fn render_tool_result_message(output: &Value) -> String {
    if output.is_string() {
        output.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(output).unwrap_or_default()
    }
}

/// `UI.tsx` `tryFlattenJson` — return key/value pairs from a top-level object.
pub fn try_flatten_json(content: &str) -> Option<Vec<(String, String)>> {
    let v: Value = serde_json::from_str(content).ok()?;
    let map = v.as_object()?;
    Some(
        map.iter()
            .map(|(k, v)| {
                let val_str = if let Some(s) = v.as_str() {
                    s.to_string()
                } else {
                    serde_json::to_string(v).unwrap_or_default()
                };
                (k.clone(), val_str)
            })
            .collect(),
    )
}

/// `UI.tsx` `tryUnwrapTextPayload` — extract content text from
/// `{ content: [ { type: 'text', text: '...' } ] }`.
pub fn try_unwrap_text_payload(content: &str) -> Option<String> {
    let v: Value = serde_json::from_str(content).ok()?;
    let arr = v.get("content")?.as_array()?;
    let mut parts = Vec::new();
    for block in arr {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                parts.push(text.to_string());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// `UI.tsx` `trySlackSendCompact` — compact summary for the Slack send tool.
pub fn try_slack_send_compact(input: &Value) -> Option<String> {
    let channel = input.get("channel").and_then(|c| c.as_str())?;
    let text = input
        .get("text")
        .or_else(|| input.get("message"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    Some(format!("→ {}: {}", channel, summarize(text)))
}

fn summarize(s: &str) -> String {
    const LIMIT: usize = 80;
    truncate_chars(s, LIMIT)
}
