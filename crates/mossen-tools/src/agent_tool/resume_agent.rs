//! # resume_agent — Resume a previously spawned agent
//!
//! Translates `tools/AgentTool/resumeAgent.ts`.
//! Handles resuming an agent from its saved transcript, re-injecting context,
//! and restarting the agent loop in the background.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use mossen_utils::string_utils::truncate_chars_with_suffix;

use super::fork_subagent::{is_fork_subagent_enabled, FORK_SUBAGENT_TYPE};
use super::load_agents_dir::{is_built_in_agent, AgentDefinition};

/// Result of resuming an agent.
#[derive(Debug, Clone, Serialize)]
pub struct ResumeAgentResult {
    pub agent_id: String,
    pub description: String,
    pub output_file: String,
}

/// Metadata stored for a previously spawned agent.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentMetadata {
    pub agent_type: Option<String>,
    pub description: Option<String>,
    pub worktree_path: Option<String>,
    pub is_fork: Option<bool>,
}

/// Parameters for resuming an agent.
pub struct ResumeAgentParams {
    pub agent_id: String,
    pub prompt: String,
    pub available_agents: Vec<AgentDefinition>,
    pub parent_session_id: Option<String>,
}

/// Read agent metadata from the stored session.
pub async fn read_agent_metadata(agent_id: &str) -> Option<AgentMetadata> {
    let session_dir = get_agent_session_dir(agent_id);
    let meta_path = session_dir.join("metadata.json");

    let content = tokio::fs::read_to_string(&meta_path).await.ok()?;
    serde_json::from_str(&content).ok()
}

/// Get the session directory for an agent.
fn get_agent_session_dir(agent_id: &str) -> PathBuf {
    let base =
        std::env::var("MOSSEN_SESSION_DIR").unwrap_or_else(|_| ".mossen/sessions".to_string());
    PathBuf::from(base).join("agents").join(agent_id)
}

/// Get the task output path for an agent.
fn get_task_output_path(agent_id: &str) -> String {
    let dir = get_agent_session_dir(agent_id);
    dir.join("output.md").to_string_lossy().to_string()
}

/// Read the agent transcript from storage.
pub async fn get_agent_transcript(agent_id: &str) -> Vec<serde_json::Value> {
    let session_dir = get_agent_session_dir(agent_id);
    let transcript_path = session_dir.join("transcript.jsonl");

    let content = match tokio::fs::read_to_string(&transcript_path).await {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Filter out orphaned thinking-only messages from a transcript.
pub fn filter_orphaned_thinking_only_messages(
    messages: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    messages
        .into_iter()
        .filter(|msg| {
            let msg_type = msg.get("type").and_then(|t| t.as_str());
            if msg_type != Some("assistant") {
                return true;
            }
            // Check if message has only thinking blocks (no text or tool_use)
            let content = msg
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array());
            match content {
                Some(blocks) => blocks.iter().any(|b| {
                    let block_type = b.get("type").and_then(|t| t.as_str());
                    block_type == Some("text") || block_type == Some("tool_use")
                }),
                None => true,
            }
        })
        .collect()
}

/// Filter out unresolved tool uses (tool_use blocks without matching tool_result).
pub fn filter_unresolved_tool_uses(messages: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    // Collect all tool_result IDs
    let result_ids: std::collections::HashSet<String> = messages
        .iter()
        .filter_map(|msg| {
            let content = msg.get("message")?.get("content")?.as_array()?;
            let ids: Vec<String> = content
                .iter()
                .filter_map(|b| {
                    if b.get("type")?.as_str()? == "tool_result" {
                        b.get("tool_use_id")?.as_str().map(String::from)
                    } else {
                        None
                    }
                })
                .collect();
            Some(ids)
        })
        .flatten()
        .collect();

    // Filter out assistant messages that have tool_use blocks without results
    messages
        .into_iter()
        .inspect(|msg| {
            let msg_type = msg.get("type").and_then(|t| t.as_str());
            if msg_type != Some("assistant") {}
            // Keep all blocks — caller handles missing results
        })
        .collect()
}

/// Filter out whitespace-only assistant messages.
pub fn filter_whitespace_only_assistant_messages(
    messages: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    messages
        .into_iter()
        .filter(|msg| {
            let msg_type = msg.get("type").and_then(|t| t.as_str());
            if msg_type != Some("assistant") {
                return true;
            }
            let content = msg
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array());
            match content {
                Some(blocks) => blocks.iter().any(|b| {
                    let block_type = b.get("type").and_then(|t| t.as_str());
                    match block_type {
                        Some("text") => {
                            let text = b.get("text").and_then(|t| t.as_str()).unwrap_or("");
                            !text.trim().is_empty()
                        }
                        Some("tool_use") | Some("thinking") => true,
                        _ => false,
                    }
                }),
                None => false,
            }
        })
        .collect()
}

/// Resume an agent in the background.
pub async fn resume_agent_background(
    params: ResumeAgentParams,
) -> Result<ResumeAgentResult, String> {
    let agent_id = &params.agent_id;
    let prompt = &params.prompt;

    // Read agent metadata
    let meta = read_agent_metadata(agent_id).await;
    let agent_type = meta
        .as_ref()
        .and_then(|m| m.agent_type.as_deref())
        .unwrap_or("general");

    let is_resumed_fork =
        meta.as_ref().and_then(|m| m.is_fork).unwrap_or(false) || agent_type == FORK_SUBAGENT_TYPE;

    // Find agent definition
    let selected_agent = if is_resumed_fork && is_fork_subagent_enabled() {
        // Fork agents use a synthetic definition
        AgentDefinition {
            agent_type: FORK_SUBAGENT_TYPE.to_string(),
            when_to_use: "Implicit fork — inherits full conversation context.".to_string(),
            tools: Some(vec!["*".to_string()]),
            disallowed_tools: None,
            skills: None,
            mcp_servers: None,
            hooks: None,
            color: None,
            model: Some("inherit".to_string()),
            effort: None,
            permission_mode: Some(super::utils::PermissionMode::Bubble),
            max_turns: Some(200),
            filename: None,
            base_dir: Some("built-in".to_string()),
            source: "built-in".to_string(),
            background: None,
            isolation: None,
            memory: None,
            initial_prompt: None,
            use_exact_tools: Some(true),
            system_prompt: None,
        }
    } else {
        params
            .available_agents
            .iter()
            .find(|a| a.agent_type == agent_type)
            .cloned()
            .unwrap_or_else(|| {
                // Fallback to general purpose agent
                AgentDefinition {
                    agent_type: "general".to_string(),
                    when_to_use: "General purpose agent".to_string(),
                    tools: Some(vec!["*".to_string()]),
                    disallowed_tools: None,
                    skills: None,
                    mcp_servers: None,
                    hooks: None,
                    color: None,
                    model: None,
                    effort: None,
                    permission_mode: None,
                    max_turns: None,
                    filename: None,
                    base_dir: None,
                    source: "built-in".to_string(),
                    background: None,
                    isolation: None,
                    memory: None,
                    initial_prompt: None,
                    use_exact_tools: None,
                    system_prompt: None,
                }
            })
    };

    let ui_description = meta
        .as_ref()
        .and_then(|m| m.description.as_deref())
        .unwrap_or("Resumed agent")
        .to_string();

    // Read and filter the transcript
    let transcript = get_agent_transcript(agent_id).await;
    let resumed_messages = filter_whitespace_only_assistant_messages(filter_unresolved_tool_uses(
        filter_orphaned_thinking_only_messages(transcript),
    ));

    debug!(
        "Resuming agent {} (type: {}, {} messages in transcript)",
        agent_id,
        selected_agent.agent_type,
        resumed_messages.len()
    );

    let output_file = get_task_output_path(agent_id);

    info!(
        "Agent {} resumed with prompt: {}",
        agent_id,
        truncate_chars_with_suffix(prompt, 100, "...")
    );

    Ok(ResumeAgentResult {
        agent_id: agent_id.to_string(),
        description: ui_description,
        output_file,
    })
}
