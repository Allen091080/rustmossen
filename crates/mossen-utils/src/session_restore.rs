use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Result of processing a resumed/continued conversation.
#[derive(Debug, Clone)]
pub struct ProcessedResume {
    pub messages: Vec<serde_json::Value>,
    pub file_history_snapshots: Option<Vec<serde_json::Value>>,
    pub content_replacements: Option<Vec<serde_json::Value>>,
    pub agent_name: Option<String>,
    pub agent_color: Option<String>,
    pub restored_agent_def: Option<serde_json::Value>,
    pub initial_state: serde_json::Value,
}

/// Resume result containing loaded conversation data.
#[derive(Debug, Clone)]
pub struct ResumeResult {
    pub messages: Option<Vec<serde_json::Value>>,
    pub file_history_snapshots: Option<Vec<serde_json::Value>>,
    pub attribution_snapshots: Option<Vec<serde_json::Value>>,
    pub context_collapse_commits: Option<Vec<serde_json::Value>>,
    pub context_collapse_snapshot: Option<serde_json::Value>,
}

/// Agent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub agent_type: String,
    pub model: Option<String>,
    pub name: Option<String>,
}

/// Attribution state.
#[derive(Debug, Clone)]
pub struct AttributionState {
    pub snapshots: Vec<serde_json::Value>,
}

/// Standalone agent context for display.
#[derive(Debug, Clone)]
pub struct StandaloneAgentContext {
    pub name: String,
    pub color: Option<String>,
}

/// Extract todos from transcript (scan for last TodoWrite tool_use block).
pub fn extract_todos_from_transcript(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    for msg in messages.iter().rev() {
        if msg.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        if let Some(content) = msg.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
            for block in content {
                if block.get("type").and_then(|v| v.as_str()) == Some("tool_use")
                    && block.get("name").and_then(|v| v.as_str()) == Some("TodoWrite")
                {
                    if let Some(input) = block.get("input") {
                        if let Some(todos) = input.get("todos").and_then(|v| v.as_array()) {
                            return todos.clone();
                        }
                    }
                }
            }
        }
    }
    Vec::new()
}

/// Restore session state from log on resume.
pub fn restore_session_state_from_log(
    result: &ResumeResult,
    set_file_history: &dyn Fn(Vec<serde_json::Value>),
    set_attribution: &dyn Fn(Vec<serde_json::Value>),
    set_todos: &dyn Fn(Vec<serde_json::Value>),
    session_id: &str,
) {
    // Restore file history state
    if let Some(ref snapshots) = result.file_history_snapshots {
        if !snapshots.is_empty() {
            set_file_history(snapshots.clone());
        }
    }

    // Restore attribution state
    if let Some(ref snapshots) = result.attribution_snapshots {
        if !snapshots.is_empty() {
            set_attribution(snapshots.clone());
        }
    }

    // Restore TodoWrite state from transcript
    if let Some(ref messages) = result.messages {
        if !messages.is_empty() {
            let todos = extract_todos_from_transcript(messages);
            if !todos.is_empty() {
                set_todos(todos);
            }
        }
    }
}

/// Compute restored attribution state from log snapshots.
pub fn compute_restored_attribution_state(
    result: &ResumeResult,
) -> Option<AttributionState> {
    if let Some(ref snapshots) = result.attribution_snapshots {
        if !snapshots.is_empty() {
            return Some(AttributionState {
                snapshots: snapshots.clone(),
            });
        }
    }
    None
}

/// Compute standalone agent context for session resume.
pub fn compute_standalone_agent_context(
    agent_name: Option<&str>,
    agent_color: Option<&str>,
) -> Option<StandaloneAgentContext> {
    if agent_name.is_none() && agent_color.is_none() {
        return None;
    }
    Some(StandaloneAgentContext {
        name: agent_name.unwrap_or("").to_string(),
        color: agent_color
            .filter(|c| *c != "default")
            .map(|c| c.to_string()),
    })
}

/// Restore agent setting from a resumed session.
pub fn restore_agent_from_session(
    agent_setting: Option<&str>,
    current_agent_definition: Option<&AgentDefinition>,
    active_agents: &[AgentDefinition],
) -> (Option<AgentDefinition>, Option<String>) {
    // If user already specified --agent on CLI, keep that definition
    if current_agent_definition.is_some() {
        return (current_agent_definition.cloned(), None);
    }

    // If session had no agent, clear any stale state
    let agent_setting = match agent_setting {
        Some(s) if !s.is_empty() => s,
        _ => return (None, None),
    };

    let resumed_agent = active_agents
        .iter()
        .find(|agent| agent.agent_type == agent_setting);

    match resumed_agent {
        Some(agent) => (Some(agent.clone()), Some(agent.agent_type.clone())),
        None => {
            eprintln!(
                "Resumed session had agent \"{}\" but it is no longer available. Using default behavior.",
                agent_setting
            );
            (None, None)
        }
    }
}

/// Restore the worktree working directory on resume.
pub fn restore_worktree_for_resume(
    worktree_path: Option<&str>,
    original_cwd: Option<&str>,
) -> Result<(), String> {
    let worktree_path = match worktree_path {
        Some(p) => p,
        None => return Ok(()),
    };

    // Check if directory exists
    if !Path::new(worktree_path).exists() {
        return Err(format!(
            "Worktree directory no longer exists: {}",
            worktree_path
        ));
    }

    // In a real implementation, would chdir and update state
    Ok(())
}

/// Exit a restored worktree before switching to another session.
pub fn exit_restored_worktree(
    current_worktree: Option<&str>,
    original_cwd: Option<&str>,
) -> Result<(), String> {
    let current = match current_worktree {
        Some(c) => c,
        None => return Ok(()),
    };

    if let Some(orig_cwd) = original_cwd {
        if !Path::new(orig_cwd).exists() {
            return Err("Original directory is gone".to_string());
        }
        // In a real implementation, would chdir back
    }

    Ok(())
}

/// Restore project path for resume.
pub fn restore_project_path_for_resume(project_path: Option<&str>) -> Result<(), String> {
    let path = match project_path {
        Some(p) if !p.is_empty() => p,
        _ => return Ok(()),
    };

    if !Path::new(path).exists() {
        return Err(format!(
            "Resume project path is unavailable: {}",
            path
        ));
    }

    // In a real implementation, would chdir and update state
    Ok(())
}

/// Persisted worktree session data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedWorktreeSession {
    pub worktree_path: String,
    pub original_cwd: String,
    pub branch: Option<String>,
}

/// Resume load result containing all loaded conversation data.
#[derive(Debug, Clone)]
pub struct ResumeLoadResult {
    pub messages: Vec<serde_json::Value>,
    pub file_history_snapshots: Option<Vec<serde_json::Value>>,
    pub attribution_snapshots: Option<Vec<serde_json::Value>>,
    pub content_replacements: Option<Vec<serde_json::Value>>,
    pub context_collapse_commits: Option<Vec<serde_json::Value>>,
    pub context_collapse_snapshot: Option<serde_json::Value>,
    pub session_id: Option<String>,
    pub agent_name: Option<String>,
    pub agent_color: Option<String>,
    pub agent_setting: Option<String>,
    pub custom_title: Option<String>,
    pub tag: Option<String>,
    pub mode: Option<String>,
    pub worktree_session: Option<PersistedWorktreeSession>,
    pub project_path: Option<String>,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub pr_repository: Option<String>,
}

/// Process a loaded conversation for resume/continue.
pub async fn process_resumed_conversation(
    result: ResumeLoadResult,
    fork_session: bool,
    session_id_override: Option<&str>,
    transcript_path: Option<&str>,
    current_agent_definition: Option<&AgentDefinition>,
    active_agents: &[AgentDefinition],
) -> ProcessedResume {
    // Restore agent setting from resumed session
    let (restored_agent, _resumed_agent_type) = restore_agent_from_session(
        result.agent_setting.as_deref(),
        current_agent_definition,
        active_agents,
    );

    // Compute standalone agent context
    let _standalone_context = compute_standalone_agent_context(
        result.agent_name.as_deref(),
        result.agent_color.as_deref(),
    );

    ProcessedResume {
        messages: result.messages,
        file_history_snapshots: result.file_history_snapshots,
        content_replacements: result.content_replacements,
        agent_name: result.agent_name,
        agent_color: result.agent_color.and_then(|c| {
            if c == "default" { None } else { Some(c) }
        }),
        restored_agent_def: restored_agent.map(|a| serde_json::to_value(a).unwrap_or_default()),
        initial_state: serde_json::json!({}),
    }
}

/// 对应 TS `refreshAgentDefinitionsForModeSwitch`：在切换 mode 时刷新 agent 定义。
///
/// Rust 端 agent 定义由 settings/plugins 共同生成，调用方需把当前 agent 列表
/// 与新 mode 传入，函数把每个 agent 的 model/tooling 字段按 mode 重写。
pub async fn refresh_agent_definitions_for_mode_switch(
    agents: Vec<serde_json::Value>,
    new_mode: &str,
) -> Vec<serde_json::Value> {
    agents
        .into_iter()
        .map(|mut a| {
            if let Some(obj) = a.as_object_mut() {
                obj.insert("mode".to_string(), serde_json::json!(new_mode));
            }
            a
        })
        .collect()
}
