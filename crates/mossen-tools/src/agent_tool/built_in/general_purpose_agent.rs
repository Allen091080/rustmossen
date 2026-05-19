//! # general_purpose_agent — General purpose agent built-in definition
//!
//! Translates `tools/AgentTool/built-in/generalPurposeAgent.ts`.

use crate::agent_tool::load_agents_dir::AgentDefinition;
use crate::agent_tool::utils::PermissionMode;

fn get_general_purpose_system_prompt() -> String {
    "You are a general-purpose coding assistant. You can help with a wide range of \
     software development tasks including writing code, debugging, refactoring, \
     testing, and documentation. Use all available tools to accomplish the task \
     given to you. Be thorough and complete in your work."
        .to_string()
}

/// Get the general purpose agent definition.
pub fn definition() -> AgentDefinition {
    AgentDefinition {
        agent_type: "general".to_string(),
        when_to_use: "General-purpose coding agent for tasks that don't match a specialized agent. \
            Can handle code writing, debugging, refactoring, and analysis."
            .to_string(),
        tools: Some(vec!["*".to_string()]),
        disallowed_tools: None,
        skills: None,
        mcp_servers: None,
        hooks: None,
        color: None,
        model: Some("inherit".to_string()),
        effort: None,
        permission_mode: Some(PermissionMode::AcceptEdits),
        max_turns: Some(200),
        filename: None,
        base_dir: Some("built-in".to_string()),
        source: "built-in".to_string(),
        background: None,
        isolation: None,
        memory: None,
        initial_prompt: None,
        use_exact_tools: None,
        system_prompt: Some(get_general_purpose_system_prompt()),
    }
}
