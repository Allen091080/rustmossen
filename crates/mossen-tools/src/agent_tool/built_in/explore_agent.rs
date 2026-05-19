//! # explore_agent — Explore agent built-in definition
//!
//! Translates `tools/AgentTool/built-in/exploreAgent.ts`.

use crate::agent_tool::load_agents_dir::AgentDefinition;
use crate::agent_tool::utils::PermissionMode;

const BASH_TOOL_NAME: &str = "Bash";
const FILE_READ_TOOL_NAME: &str = "Read";
const GLOB_TOOL_NAME: &str = "Glob";
const GREP_TOOL_NAME: &str = "Grep";
const AGENT_TOOL_NAME: &str = "Agent";

fn get_explore_system_prompt() -> String {
    let embedded = std::env::var("MOSSEN_EMBEDDED_SEARCH_TOOLS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let glob_guidance = if embedded {
        format!("- Use `find` via {} for broad file pattern matching", BASH_TOOL_NAME)
    } else {
        format!("- Use {} for broad file pattern matching", GLOB_TOOL_NAME)
    };

    let grep_guidance = if embedded {
        format!("- Use `grep` via {} for searching file contents with regex", BASH_TOOL_NAME)
    } else {
        format!("- Use {} for searching file contents with regex", GREP_TOOL_NAME)
    };

    let grep_extra = if embedded { ", grep" } else { "" };

    format!(
        r#"You are a file search specialist for Mossen. You excel at thoroughly navigating and exploring codebases.

=== CRITICAL: READ-ONLY MODE - NO FILE MODIFICATIONS ===
This is a READ-ONLY exploration task. You are STRICTLY PROHIBITED from:
- Creating new files (no Write, touch, or file creation of any kind)
- Modifying existing files (no Edit operations)
- Deleting files (no rm or deletion)
- Moving or copying files (no mv or cp)
- Creating temporary files anywhere, including /tmp
- Using redirect operators (>, >>, |) or heredocs to write to files
- Running ANY commands that change system state

Your role is EXCLUSIVELY to search and analyze existing code. You do NOT have access to file editing tools - attempting to edit files will fail.

Your strengths:
- Rapidly finding files using glob patterns
- Searching code and text with powerful regex patterns
- Reading and analyzing file contents

Guidelines:
{glob_guidance}
{grep_guidance}
- Use {read} when you know the specific file path you need to read
- Use {bash} ONLY for read-only operations (ls, git status, git log, git diff, find{grep_extra}, cat, head, tail)
- NEVER use {bash} for: mkdir, touch, rm, cp, mv, git add, git commit, npm install, pip install, or any file creation/modification
- Adapt your search approach based on the thoroughness level specified by the caller"#,
        glob_guidance = glob_guidance,
        grep_guidance = grep_guidance,
        read = FILE_READ_TOOL_NAME,
        bash = BASH_TOOL_NAME,
        grep_extra = grep_extra,
    )
}

/// Get the explore agent definition.
pub fn definition() -> AgentDefinition {
    let embedded = std::env::var("MOSSEN_EMBEDDED_SEARCH_TOOLS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let tools = if embedded {
        vec![BASH_TOOL_NAME.to_string(), FILE_READ_TOOL_NAME.to_string()]
    } else {
        vec![
            GLOB_TOOL_NAME.to_string(),
            GREP_TOOL_NAME.to_string(),
            FILE_READ_TOOL_NAME.to_string(),
            BASH_TOOL_NAME.to_string(),
        ]
    };

    AgentDefinition {
        agent_type: "explore".to_string(),
        when_to_use: "Use this agent for broad file search and codebase exploration tasks. \
            It excels at finding files, searching code, and analyzing project structure."
            .to_string(),
        tools: Some(tools),
        disallowed_tools: Some(vec![
            AGENT_TOOL_NAME.to_string(),
            "ExitPlanMode".to_string(),
            "Edit".to_string(),
            "Write".to_string(),
            "NotebookEdit".to_string(),
        ]),
        skills: None,
        mcp_servers: None,
        hooks: None,
        color: None,
        model: Some("inherit".to_string()),
        effort: None,
        permission_mode: Some(PermissionMode::DontAsk),
        max_turns: None,
        filename: None,
        base_dir: Some("built-in".to_string()),
        source: "built-in".to_string(),
        background: None,
        isolation: None,
        memory: None,
        initial_prompt: None,
        use_exact_tools: None,
        system_prompt: Some(get_explore_system_prompt()),
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/AgentTool/built-in/exploreAgent.ts` exports.
// ---------------------------------------------------------------------------

/// `exploreAgent.ts` `EXPLORE_AGENT_MIN_QUERIES`.
pub const EXPLORE_AGENT_MIN_QUERIES: usize = 3;

/// `exploreAgent.ts` `EXPLORE_AGENT`.
pub fn explore_agent() -> AgentDefinition {
    definition()
}
