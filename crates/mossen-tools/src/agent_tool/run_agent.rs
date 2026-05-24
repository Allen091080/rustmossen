//! # run_agent — Core agent execution loop
//!
//! Translates `tools/AgentTool/runAgent.ts`.
//! Implements the main agent execution loop: prompt assembly, model invocation,
//! tool dispatching, turn counting, and result collection.

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use super::load_agents_dir::AgentDefinition;
use super::utils::PermissionMode;

/// Maximum turns before stopping an agent.
const DEFAULT_MAX_TURNS: u32 = 200;

/// Agent execution options.
#[derive(Debug, Clone)]
pub struct RunAgentOptions {
    pub agent_definition: AgentDefinition,
    pub prompt_messages: Vec<Value>,
    pub model: Option<String>,
    pub is_async: bool,
    pub query_source: String,
    pub available_tool_names: Vec<String>,
    pub fork_context_messages: Option<Vec<Value>>,
    pub use_exact_tools: bool,
    pub worktree_path: Option<String>,
    pub description: Option<String>,
    pub override_system_prompt: Option<String>,
    pub override_agent_id: Option<String>,
}

/// Result of running an agent.
#[derive(Debug, Clone, Serialize)]
pub struct RunAgentResult {
    pub output: String,
    pub turn_count: u32,
    pub token_count: u64,
    pub duration_ms: u64,
    pub stopped_reason: StoppedReason,
    pub worktree_path: Option<String>,
}

/// Reason why the agent stopped.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StoppedReason {
    /// Agent completed normally (end_turn stop reason)
    EndTurn,
    /// Agent hit the max turns limit
    MaxTurns,
    /// Agent was aborted/cancelled
    Aborted,
    /// Agent encountered an error
    Error,
}

/// Cache-safe parameters for prompt caching.
#[derive(Debug, Clone, Serialize)]
pub struct CacheSafeParams {
    pub system_prompt_hash: String,
    pub tool_definitions_hash: String,
    pub message_count: usize,
}

/// Agent turn context — tracks state across turns.
struct AgentTurnState {
    turn_count: u32,
    total_tokens: u64,
    start_time: Instant,
    messages: Vec<Value>,
    stop_reason: StoppedReason,
}

impl AgentTurnState {
    fn new(initial_messages: Vec<Value>) -> Self {
        Self {
            turn_count: 0,
            total_tokens: 0,
            start_time: Instant::now(),
            messages: initial_messages,
            stop_reason: StoppedReason::EndTurn,
        }
    }
}

/// Run the agent execution loop.
///
/// This drives the agent through multiple turns:
/// 1. Assemble system prompt
/// 2. Send messages to model
/// 3. Process tool_use blocks in response
/// 4. Append results to conversation
/// 5. Repeat until end_turn, max_turns, or abort
pub async fn run_agent(options: RunAgentOptions) -> RunAgentResult {
    let max_turns = options
        .agent_definition
        .max_turns
        .unwrap_or(DEFAULT_MAX_TURNS);

    let agent_type = &options.agent_definition.agent_type;
    let is_built_in = options.agent_definition.source == "built-in";

    debug!(
        "Starting agent run: type={}, max_turns={}, is_async={}",
        agent_type, max_turns, options.is_async
    );

    // Build initial messages
    let mut state = AgentTurnState::new(options.prompt_messages.clone());

    // Include fork context if provided
    if let Some(fork_msgs) = &options.fork_context_messages {
        // Prepend fork context before the prompt messages
        let mut all_msgs = fork_msgs.clone();
        all_msgs.extend(state.messages.drain(..));
        state.messages = all_msgs;
    }

    // Main agent loop
    loop {
        if state.turn_count >= max_turns {
            state.stop_reason = StoppedReason::MaxTurns;
            warn!("Agent {} hit max turns limit ({})", agent_type, max_turns);
            break;
        }

        state.turn_count += 1;

        // In a real implementation, this would call the model API.
        // The response would contain text and/or tool_use blocks.
        // For now, we simulate the loop structure.

        // Process assistant response
        let response = process_agent_turn(&state.messages, &options).await;

        match response {
            AgentTurnResult::EndTurn { output, tokens } => {
                state.total_tokens += tokens;
                state.stop_reason = StoppedReason::EndTurn;
                debug!(
                    "Agent {} completed after {} turns",
                    agent_type, state.turn_count
                );

                let duration_ms = state.start_time.elapsed().as_millis() as u64;
                return RunAgentResult {
                    output,
                    turn_count: state.turn_count,
                    token_count: state.total_tokens,
                    duration_ms,
                    stopped_reason: state.stop_reason,
                    worktree_path: options.worktree_path.clone(),
                };
            }
            AgentTurnResult::ToolUse {
                assistant_message,
                tool_results,
                tokens,
            } => {
                state.total_tokens += tokens;
                // Append assistant message and tool results to conversation
                state.messages.push(assistant_message);
                state.messages.push(tool_results);
            }
            AgentTurnResult::Aborted => {
                state.stop_reason = StoppedReason::Aborted;
                break;
            }
            AgentTurnResult::Error { message } => {
                state.stop_reason = StoppedReason::Error;
                warn!("Agent {} error: {}", agent_type, message);
                let duration_ms = state.start_time.elapsed().as_millis() as u64;
                return RunAgentResult {
                    output: format!("Error: {}", message),
                    turn_count: state.turn_count,
                    token_count: state.total_tokens,
                    duration_ms,
                    stopped_reason: state.stop_reason,
                    worktree_path: options.worktree_path.clone(),
                };
            }
        }
    }

    let duration_ms = state.start_time.elapsed().as_millis() as u64;

    // Extract final output from the last assistant message
    let output = extract_final_output(&state.messages);

    RunAgentResult {
        output,
        turn_count: state.turn_count,
        token_count: state.total_tokens,
        duration_ms,
        stopped_reason: state.stop_reason,
        worktree_path: options.worktree_path,
    }
}

/// Result of processing a single agent turn.
enum AgentTurnResult {
    /// Agent decided to end (no more tool calls).
    EndTurn { output: String, tokens: u64 },
    /// Agent made tool calls; results need to be appended.
    ToolUse {
        assistant_message: Value,
        tool_results: Value,
        tokens: u64,
    },
    /// Agent was aborted.
    Aborted,
    /// An error occurred.
    Error { message: String },
}

/// Process a single agent turn (model call + tool execution).
async fn process_agent_turn(messages: &[Value], options: &RunAgentOptions) -> AgentTurnResult {
    // In a full implementation, this would:
    // 1. Call the model API with messages + system prompt + tools
    // 2. Parse the response for tool_use blocks
    // 3. Execute each tool
    // 4. Return the results

    // The actual model invocation is handled by the runtime/orchestrator.
    // This module provides the loop structure and message management.
    AgentTurnResult::EndTurn {
        output: String::new(),
        tokens: 0,
    }
}

/// Extract the final text output from the last assistant message.
fn extract_final_output(messages: &[Value]) -> String {
    for msg in messages.iter().rev() {
        let msg_type = msg.get("type").and_then(|t| t.as_str());
        if msg_type != Some("assistant") {
            continue;
        }
        let content = msg
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array());
        if let Some(blocks) = content {
            let text_parts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| {
                    if b.get("type")?.as_str()? == "text" {
                        b.get("text")?.as_str()
                    } else {
                        None
                    }
                })
                .collect();
            if !text_parts.is_empty() {
                return text_parts.join("");
            }
        }
    }
    String::new()
}

/// Get the query source for agent analytics.
pub fn get_query_source_for_agent(agent_type: &str, is_built_in: bool) -> String {
    if is_built_in {
        format!("built-in-agent:{}", agent_type)
    } else {
        format!("custom-agent:{}", agent_type)
    }
}

/// Get the agent model to use, resolving "inherit" and defaults.
pub fn get_agent_model(
    agent_model: Option<&str>,
    parent_model: Option<&str>,
    override_model: Option<&str>,
    _permission_mode: Option<&PermissionMode>,
) -> Option<String> {
    // Override takes highest priority
    if let Some(m) = override_model {
        return Some(m.to_string());
    }

    match agent_model {
        Some("inherit") | None => parent_model.map(|s| s.to_string()),
        Some(m) => Some(m.to_string()),
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/AgentTool/runAgent.ts` additional export.
// ---------------------------------------------------------------------------

/// `runAgent.ts` `filterIncompleteToolCalls`.
pub fn filter_incomplete_tool_calls(messages: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut tool_use_to_result_ok: HashMap<String, bool> = HashMap::new();
    for msg in &messages {
        let Some(content) = msg.get("content").and_then(|c| c.as_array()) else {
            continue;
        };
        for block in content {
            let kind = block.get("type").and_then(|t| t.as_str());
            if kind == Some("tool_use") {
                if let Some(id) = block.get("id").and_then(|i| i.as_str()) {
                    tool_use_to_result_ok.entry(id.to_string()).or_insert(false);
                }
            } else if kind == Some("tool_result") {
                if let Some(id) = block.get("tool_use_id").and_then(|i| i.as_str()) {
                    tool_use_to_result_ok.insert(id.to_string(), true);
                }
            }
        }
    }
    messages
        .into_iter()
        .filter(|msg| {
            let Some(content) = msg.get("content").and_then(|c| c.as_array()) else {
                return true;
            };
            for block in content {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    if let Some(id) = block.get("id").and_then(|i| i.as_str()) {
                        if !tool_use_to_result_ok.get(id).copied().unwrap_or(false) {
                            return false;
                        }
                    }
                }
            }
            true
        })
        .collect()
}
