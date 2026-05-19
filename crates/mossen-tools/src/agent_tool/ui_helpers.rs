//! Text-mode mirror of `tools/AgentTool/UI.tsx`.

/// `UI.tsx` `AgentPromptDisplay` — flatten the agent prompt for printing.
pub fn agent_prompt_display(agent_type: Option<&str>, prompt: Option<&str>) -> String {
    let kind = agent_type.unwrap_or("agent");
    let body = prompt.unwrap_or("");
    if body.is_empty() {
        format!("[{}]", kind)
    } else {
        format!("[{}] {}", kind, body)
    }
}

/// `UI.tsx` `AgentResponseDisplay` — flatten the agent's response.
pub fn agent_response_display(agent_type: Option<&str>, body: &str) -> String {
    let kind = agent_type.unwrap_or("agent");
    format!("[{}] {}", kind, body)
}

/// `UI.tsx` `renderToolUseMessage`.
pub fn render_tool_use_message(agent_type: Option<&str>, prompt: Option<&str>) -> String {
    let kind = agent_type.unwrap_or("agent");
    match prompt {
        Some(p) if !p.is_empty() => format!("Task[{}]: {}", kind, p),
        _ => format!("Task[{}]", kind),
    }
}

/// `UI.tsx` `renderToolUseProgressMessage`.
pub fn render_tool_use_progress_message(agent_type: Option<&str>, tool_use_count: usize) -> String {
    format!(
        "Task[{}] running ({} tool call{})",
        agent_type.unwrap_or("agent"),
        tool_use_count,
        if tool_use_count == 1 { "" } else { "s" }
    )
}

/// `UI.tsx` `renderToolUseRejectedMessage`.
pub fn render_tool_use_rejected_message(agent_type: Option<&str>) -> String {
    format!("(Rejected) Task[{}]", agent_type.unwrap_or("agent"))
}

/// `UI.tsx` `renderToolUseErrorMessage`.
pub fn render_tool_use_error_message(message: &str) -> String {
    format!("Task error: {}", message)
}

/// `UI.tsx` `renderToolUseQueuedMessage`.
pub fn render_tool_use_queued_message() -> &'static str {
    "Task queued"
}

/// `UI.tsx` `renderToolResultMessage`.
pub fn render_tool_result_message(
    agent_type: Option<&str>,
    summary: Option<&str>,
) -> String {
    let kind = agent_type.unwrap_or("agent");
    match summary {
        Some(s) if !s.is_empty() => format!("Task[{}] complete: {}", kind, s),
        _ => format!("Task[{}] complete", kind),
    }
}

/// `UI.tsx` `renderAgentLastUsedTool` — short label for the last tool an agent
/// used (used in progress lines).
pub fn render_agent_last_used_tool(tool_name: Option<&str>) -> String {
    format!("last tool: {}", tool_name.unwrap_or("<idle>"))
}

/// `UI.tsx` `renderResultText` — pretty-print the agent's result text.
pub fn render_result_text(text: &str) -> String {
    text.to_string()
}

/// `UI.tsx` `renderAgentToolUseId` — display the task ID stamp.
pub fn render_agent_tool_use_id(task_id: &str) -> String {
    format!("(task {})", task_id)
}

/// `UI.tsx` `renderAgentSettings` — flatten model/effort settings.
pub fn render_agent_settings(model: Option<&str>, effort: Option<&str>) -> String {
    match (model, effort) {
        (Some(m), Some(e)) => format!("model={}, effort={}", m, e),
        (Some(m), None) => format!("model={}", m),
        (None, Some(e)) => format!("effort={}", e),
        (None, None) => String::new(),
    }
}

/// `UI.tsx` `renderAgentTokens` — summarize token usage for an agent run.
pub fn render_agent_tokens(input: u64, output: u64) -> String {
    format!("tokens: {} in / {} out", input, output)
}

/// `UI.tsx` `renderGroupedAgentToolUse` — collapse a series of agent
/// invocations into one display line.
pub fn render_grouped_agent_tool_use(agent_type: &str, count: usize) -> String {
    format!(
        "Task[{}] × {} invocation{}",
        agent_type,
        count,
        if count == 1 { "" } else { "s" }
    )
}

/// `UI.tsx` `userFacingNameBackgroundColor` — pick a background colour token
/// for the agent name pill. The Rust port returns a stable string the TUI
/// crate can map to its palette.
pub fn user_facing_name_background_color(agent_type: &str) -> &'static str {
    match agent_type {
        "code-review" => "blue",
        "test-author" => "green",
        "debug" => "red",
        "research" => "magenta",
        _ => "default",
    }
}

/// `UI.tsx` `extractLastToolInfo` — pull a `(tool_name, summary)` tuple from
/// the latest assistant message of the subagent's transcript.
pub fn extract_last_tool_info(messages: &[serde_json::Value]) -> Option<(String, String)> {
    for msg in messages.iter().rev() {
        let Some(content) = msg.get("content").and_then(|c| c.as_array()) else {
            continue;
        };
        for block in content.iter().rev() {
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                let name = block
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let summary = block
                    .get("input")
                    .and_then(|v| serde_json::to_string(v).ok())
                    .unwrap_or_default();
                return Some((name, summary));
            }
        }
    }
    None
}
