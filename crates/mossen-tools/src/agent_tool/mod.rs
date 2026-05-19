//! # agent_tool — Subagent helper modules (complete translation of AgentTool/)
//!
//! Translates all 20 TS files from `tools/AgentTool/`:
//! - AgentTool.tsx (core) → already in parent `agent.rs`
//! - constants.ts → constants submodule
//! - agentColorManager.ts → color_manager submodule
//! - agentDisplay.ts → display submodule
//! - agentMemory.ts → memory submodule
//! - agentMemorySnapshot.ts → memory_snapshot submodule
//! - agentToolUtils.ts → utils submodule
//! - builtInAgents.ts → built_in_agents submodule
//! - forkSubagent.ts → fork_subagent submodule
//! - loadAgentsDir.ts → load_agents_dir submodule
//! - prompt.ts → prompt submodule
//! - resumeAgent.ts → resume_agent submodule
//! - runAgent.ts → run_agent submodule
//! - built-in/*.ts → built_in submodule
//! - UI.tsx → (display logic in struct methods)

pub mod built_in;
pub mod built_in_agents;
pub mod color_manager;
pub mod constants;
pub mod display;
pub mod fork_subagent;
pub mod load_agents_dir;
pub mod memory;
pub mod memory_snapshot;
pub mod prompt;
pub mod resume_agent;
pub mod run_agent;
pub mod ui_helpers;
pub mod utils;
pub use ui_helpers::{
    agent_prompt_display, agent_response_display, extract_last_tool_info,
    render_agent_last_used_tool, render_agent_settings, render_agent_tokens,
    render_agent_tool_use_id, render_grouped_agent_tool_use, render_result_text,
    render_tool_result_message as render_agent_tool_result_message,
    render_tool_use_error_message as render_agent_tool_use_error_message,
    render_tool_use_message as render_agent_tool_use_message,
    render_tool_use_progress_message as render_agent_tool_use_progress_message,
    render_tool_use_queued_message as render_agent_tool_use_queued_message,
    render_tool_use_rejected_message as render_agent_tool_use_rejected_message,
    user_facing_name_background_color,
};

/// `AgentTool.tsx` `AgentToolProgress` — AgentTool's own progress payload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentToolProgress {
    pub agent_id: String,
    pub agent_type: Option<String>,
    pub tool_use_count: usize,
    pub tokens: u64,
    pub elapsed_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// `AgentTool.tsx` `ShellProgress` — forwarded shell/bash progress from a
/// sub-agent's BashTool or PowerShellTool execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShellProgress {
    pub agent_id: String,
    pub command: Option<String>,
    pub partial_output: Option<String>,
    pub elapsed_ms: u64,
}

/// `AgentTool.tsx` `Progress` — combined progress type that AgentTool forwards.
/// AgentTool forwards both its own progress events and shell progress events
/// from the sub-agent so the SDK receives `tool_progress` updates during
/// bash/powershell runs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Progress {
    Agent(AgentToolProgress),
    Shell(ShellProgress),
}
