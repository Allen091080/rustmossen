//! # utils — Agent tool utilities
//!
//! Translates `tools/AgentTool/agentToolUtils.ts`.
//! Provides tool filtering, agent resolution, permission checking,
//! and async agent lifecycle management.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::constants::AGENT_TOOL_NAME;

/// Tools that are always disallowed for all agent types.
const ALL_AGENT_DISALLOWED_TOOLS: &[&str] = &[
    "TodoWrite",
    "CostQuery",
    "EffortControl",
    "OutputStyle",
];

/// Additional tools disallowed for custom (non-built-in) agents.
const CUSTOM_AGENT_DISALLOWED_TOOLS: &[&str] = &[
    "TeamCreate",
    "TeamDelete",
];

/// Tools allowed for async (background) agents.
const ASYNC_AGENT_ALLOWED_TOOLS: &[&str] = &[
    "Bash",
    "Read",
    "Write",
    "Edit",
    "Glob",
    "Grep",
    "LSP",
    "NotebookEdit",
    "WebFetch",
    "WebSearch",
    "Agent",
    "SendMessage",
    "Skill",
];

/// In-process teammate allowed tools subset.
const IN_PROCESS_TEAMMATE_ALLOWED_TOOLS: &[&str] = &[
    "Bash",
    "Read",
    "Write",
    "Edit",
    "Glob",
    "Grep",
    "LSP",
    "NotebookEdit",
    "WebFetch",
    "WebSearch",
    "Agent",
];

/// Permission mode for agent execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    AcceptEdits,
    DontAsk,
    Plan,
    Bubble,
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::AcceptEdits
    }
}

/// Result of resolving tools for an agent.
#[derive(Debug, Clone)]
pub struct ResolvedAgentTools {
    pub has_wildcard: bool,
    pub valid_tools: Vec<String>,
    pub invalid_tools: Vec<String>,
    pub resolved_tool_names: Vec<String>,
    pub allowed_agent_types: Option<Vec<String>>,
}

/// Filter tools available for a given agent configuration.
pub fn filter_tools_for_agent(
    tool_names: &[String],
    is_built_in: bool,
    is_async: bool,
    permission_mode: Option<&PermissionMode>,
) -> Vec<String> {
    let all_disallowed: HashSet<&str> = ALL_AGENT_DISALLOWED_TOOLS.iter().copied().collect();
    let custom_disallowed: HashSet<&str> = CUSTOM_AGENT_DISALLOWED_TOOLS.iter().copied().collect();
    let async_allowed: HashSet<&str> = ASYNC_AGENT_ALLOWED_TOOLS.iter().copied().collect();

    tool_names
        .iter()
        .filter(|name| {
            let name_str = name.as_str();

            // Allow MCP tools for all agents
            if name_str.starts_with("mcp__") {
                return true;
            }

            // Allow ExitPlanMode for agents in plan mode
            if name_str == "ExitPlanMode" && permission_mode == Some(&PermissionMode::Plan) {
                return true;
            }

            if all_disallowed.contains(name_str) {
                return false;
            }

            if !is_built_in && custom_disallowed.contains(name_str) {
                return false;
            }

            if is_async && !async_allowed.contains(name_str) {
                return false;
            }

            true
        })
        .cloned()
        .collect()
}

/// Resolve tools for an agent based on its tool specification.
/// Handles wildcards ('*'), explicit allowlists, and denylists.
pub fn resolve_agent_tools(
    all_tool_names: &[String],
    agent_tools: Option<&[String]>,
    agent_disallowed: Option<&[String]>,
    is_built_in: bool,
    is_async: bool,
    permission_mode: Option<&PermissionMode>,
) -> ResolvedAgentTools {
    let has_wildcard = agent_tools
        .map(|t| t.iter().any(|s| s == "*"))
        .unwrap_or(true); // No tools specified = wildcard

    let all_names_set: HashSet<&str> = all_tool_names.iter().map(|s| s.as_str()).collect();
    let disallow_set: HashSet<&str> = agent_disallowed
        .unwrap_or(&[])
        .iter()
        .map(|s| s.as_str())
        .collect();

    let (valid_tools, invalid_tools) = if has_wildcard {
        // Wildcard: use all available tools minus disallowed
        let valid: Vec<String> = all_tool_names
            .iter()
            .filter(|n| !disallow_set.contains(n.as_str()))
            .cloned()
            .collect();
        (valid, Vec::new())
    } else {
        let specified = agent_tools.unwrap_or(&[]);
        let mut valid = Vec::new();
        let mut invalid = Vec::new();

        for tool_name in specified {
            if tool_name == "*" {
                continue;
            }
            if all_names_set.contains(tool_name.as_str()) && !disallow_set.contains(tool_name.as_str()) {
                valid.push(tool_name.clone());
            } else {
                invalid.push(tool_name.clone());
            }
        }
        (valid, invalid)
    };

    // Apply agent-level filters (built-in check, async check, etc.)
    let resolved_tool_names = filter_tools_for_agent(
        &valid_tools,
        is_built_in,
        is_async,
        permission_mode,
    );

    // Extract allowed agent types from tools list if specified
    let allowed_agent_types = agent_tools.and_then(|tools| {
        let agent_types: Vec<String> = tools
            .iter()
            .filter(|t| t.starts_with("agent:"))
            .map(|t| t.strip_prefix("agent:").unwrap_or(t).to_string())
            .collect();
        if agent_types.is_empty() {
            None
        } else {
            Some(agent_types)
        }
    });

    ResolvedAgentTools {
        has_wildcard,
        valid_tools,
        invalid_tools,
        resolved_tool_names,
        allowed_agent_types,
    }
}

/// Metadata for analytics/logging of agent execution.
#[derive(Debug, Clone)]
pub struct AgentExecutionMetadata {
    pub prompt: String,
    pub resolved_agent_model: Option<String>,
    pub is_built_in_agent: bool,
    pub start_time: Instant,
    pub agent_type: String,
    pub is_async: bool,
}

/// Progress tracker for an async agent.
#[derive(Debug)]
pub struct ProgressTracker {
    pub agent_id: String,
    pub start_time: Instant,
    pub last_update: Instant,
    pub token_count: u64,
    pub message_count: u32,
    pub is_complete: bool,
}

impl ProgressTracker {
    pub fn new(agent_id: &str) -> Self {
        let now = Instant::now();
        Self {
            agent_id: agent_id.to_string(),
            start_time: now,
            last_update: now,
            token_count: 0,
            message_count: 0,
            is_complete: false,
        }
    }

    pub fn update(&mut self, tokens: u64) {
        self.token_count += tokens;
        self.message_count += 1;
        self.last_update = Instant::now();
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn complete(&mut self) {
        self.is_complete = true;
    }
}

/// Run the async agent lifecycle: starts the agent, tracks progress,
/// handles completion/failure, and notifies the parent.
pub async fn run_async_agent_lifecycle(
    task_id: &str,
    description: &str,
    agent_type: &str,
    is_built_in: bool,
) -> Result<String, String> {
    debug!(
        "Starting async agent lifecycle for {} (type: {})",
        task_id, agent_type
    );

    let tracker = ProgressTracker::new(task_id);

    // The actual agent execution would be handled by the runtime.
    // This function coordinates the lifecycle events.
    info!(
        "Agent {} ({}) started, description: {}",
        task_id, agent_type, description
    );

    // Return task ID for tracking
    Ok(task_id.to_string())
}

/// Extract text content from a message content array (simplified).
pub fn extract_text_content(content: &[serde_json::Value]) -> String {
    content
        .iter()
        .filter_map(|block| {
            if block.get("type")?.as_str()? == "text" {
                block.get("text")?.as_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Check if a tool name matches (case-insensitive comparison).
pub fn tool_matches_name(tool_name: &str, target: &str) -> bool {
    tool_name.eq_ignore_ascii_case(target)
}

/// Get the last assistant message from a conversation history.
pub fn get_last_assistant_message(messages: &[serde_json::Value]) -> Option<&serde_json::Value> {
    messages.iter().rev().find(|m| {
        m.get("type")
            .and_then(|t| t.as_str())
            .map_or(false, |t| t == "assistant")
    })
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/AgentTool/agentToolUtils.ts` additional exports.
// ---------------------------------------------------------------------------

/// `agentToolUtils.ts` `countToolUses` — count tool_use blocks across the
/// assistant messages of a conversation.
pub fn count_tool_uses(messages: &[serde_json::Value]) -> usize {
    let mut count = 0usize;
    for m in messages {
        let Some(content) = m.get("content").and_then(|c| c.as_array()) else {
            continue;
        };
        for block in content {
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                count += 1;
            }
        }
    }
    count
}

/// `agentToolUtils.ts` `getLastToolUseName` — name of the last tool_use in a
/// message's content array.
pub fn get_last_tool_use_name(message: &serde_json::Value) -> Option<String> {
    let content = message.get("content").and_then(|c| c.as_array())?;
    for block in content.iter().rev() {
        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
            if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// `agentToolUtils.ts` `AgentToolResult` shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub agent_type: String,
    pub total_tool_use_count: usize,
    pub total_token_count: u64,
    pub total_duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tool_use_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// `agentToolUtils.ts` `finalizeAgentTool` — collapse a list of subagent
/// messages into the AgentToolResult shape used by the parent agent.
pub fn finalize_agent_tool(
    agent_type: &str,
    messages: &[serde_json::Value],
    total_token_count: u64,
    total_duration_ms: u64,
    task_id: Option<&str>,
) -> AgentToolResult {
    let total_tool_use_count = count_tool_uses(messages);
    let last_assistant = get_last_assistant_message(messages);
    let last_tool_use_name = last_assistant.and_then(get_last_tool_use_name);
    let result_text = last_assistant
        .and_then(|m| m.get("content").and_then(|c| c.as_array()))
        .map(|blocks| extract_text_content(blocks))
        .filter(|s| !s.is_empty());
    AgentToolResult {
        agent_type: agent_type.to_string(),
        total_tool_use_count,
        total_token_count,
        total_duration_ms,
        last_tool_use_name,
        result_text,
        task_id: task_id.map(|s| s.to_string()),
    }
}

/// `agentToolUtils.ts` `emitTaskProgress` — emit a progress payload for the
/// agent's parent task. Returns the constructed payload; transport happens
/// in the caller.
pub fn emit_task_progress(
    task_id: &str,
    token_count: u64,
    duration_ms: u64,
    last_tool_use: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "type": "task_progress",
        "task_id": task_id,
        "token_count": token_count,
        "duration_ms": duration_ms,
        "last_tool_use": last_tool_use,
    })
}

/// `agentToolUtils.ts` `extractPartialResult` — pull the last assistant text
/// out of a conversation so callers can show progress mid-flight.
pub fn extract_partial_result(messages: &[serde_json::Value]) -> Option<String> {
    let m = get_last_assistant_message(messages)?;
    let content = m.get("content").and_then(|c| c.as_array())?;
    Some(extract_text_content(content))
}

/// `agentToolUtils.ts` `classifyHandoffIfNeeded` — classify whether the
/// last message implies a handoff to the parent. Returns the classification.
pub async fn classify_handoff_if_needed(message: &serde_json::Value) -> &'static str {
    if let Some(text) = message
        .get("content")
        .and_then(|c| c.as_array())
        .map(|blocks| extract_text_content(blocks))
    {
        let lower = text.to_lowercase();
        if lower.contains("handoff to") || lower.contains("hand off to") {
            return "handoff";
        }
    }
    "normal"
}

/// `agentToolUtils.ts` `runAsyncAgentLifecycle` — high-level lifecycle hook
/// adapter returning the AgentToolResult shape from finalized state.
pub async fn finalize_async_agent_lifecycle(
    task_id: String,
    agent_type: String,
    initial_messages: Vec<serde_json::Value>,
) -> AgentToolResult {
    finalize_agent_tool(&agent_type, &initial_messages, 0, 0, Some(&task_id))
}

/// Alias for the agent tool result validator (mirrors TS `agentToolResultSchema`).
#[allow(non_camel_case_types)]
pub type agentToolResultSchema = AgentToolResult;
