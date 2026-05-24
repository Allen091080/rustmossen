//! # statusline_setup_agent — Statusline setup agent built-in definition
//!
//! Translates `tools/AgentTool/built-in/statuslineSetup.ts`.

use crate::agent_tool::load_agents_dir::AgentDefinition;
use crate::agent_tool::utils::PermissionMode;

fn get_statusline_setup_system_prompt() -> String {
    "You help users configure their terminal statusline integration with Mossen. \
     You can read configuration files, suggest changes, and help set up the \
     statusline display for supported terminals (iTerm2, kitty, tmux, etc.)."
        .to_string()
}

/// Get the statusline setup agent definition.
pub fn definition() -> AgentDefinition {
    AgentDefinition {
        agent_type: "statusline-setup".to_string(),
        when_to_use: "Use this agent to help users configure their terminal statusline \
            integration with Mossen."
            .to_string(),
        tools: Some(vec![
            "Bash".to_string(),
            "Read".to_string(),
            "Write".to_string(),
            "Glob".to_string(),
        ]),
        disallowed_tools: None,
        skills: None,
        mcp_servers: None,
        hooks: None,
        color: None,
        model: Some("fast".to_string()),
        effort: None,
        permission_mode: Some(PermissionMode::AcceptEdits),
        max_turns: Some(20),
        filename: None,
        base_dir: Some("built-in".to_string()),
        source: "built-in".to_string(),
        background: None,
        isolation: None,
        memory: None,
        initial_prompt: None,
        use_exact_tools: None,
        system_prompt: Some(get_statusline_setup_system_prompt()),
    }
}

/// `statuslineSetup.ts` `STATUSLINE_SETUP_AGENT` — singleton definition.
pub static STATUSLINE_SETUP_AGENT: std::sync::LazyLock<AgentDefinition> =
    std::sync::LazyLock::new(definition);
