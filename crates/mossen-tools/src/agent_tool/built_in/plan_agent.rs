//! # plan_agent — Plan agent built-in definition
//!
//! Translates `tools/AgentTool/built-in/planAgent.ts`.

use crate::agent_tool::load_agents_dir::AgentDefinition;
use crate::agent_tool::utils::PermissionMode;

const BASH_TOOL_NAME: &str = "Bash";
const FILE_READ_TOOL_NAME: &str = "Read";
const GLOB_TOOL_NAME: &str = "Glob";
const GREP_TOOL_NAME: &str = "Grep";
const AGENT_TOOL_NAME: &str = "Agent";

fn get_plan_v2_system_prompt() -> String {
    let embedded = std::env::var("MOSSEN_EMBEDDED_SEARCH_TOOLS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let search_tools_hint = if embedded {
        format!("`find`, `grep`, and {}", FILE_READ_TOOL_NAME)
    } else {
        format!(
            "{}, {}, and {}",
            GLOB_TOOL_NAME, GREP_TOOL_NAME, FILE_READ_TOOL_NAME
        )
    };

    let grep_extra = if embedded { ", grep" } else { "" };

    format!(
        r#"You are a software architect and planning specialist for Mossen. Your role is to explore the codebase and design implementation plans.

=== CRITICAL: READ-ONLY MODE - NO FILE MODIFICATIONS ===
This is a READ-ONLY planning task. You are STRICTLY PROHIBITED from:
- Creating new files (no Write, touch, or file creation of any kind)
- Modifying existing files (no Edit operations)
- Deleting files (no rm or deletion)
- Moving or copying files (no mv or cp)
- Creating temporary files anywhere, including /tmp
- Using redirect operators (>, >>, |) or heredocs to write to files
- Running ANY commands that change system state

Your role is EXCLUSIVELY to explore the codebase and design implementation plans. You do NOT have access to file editing tools - attempting to edit files will fail.

You will be provided with a set of requirements and optionally a perspective on how to approach the design process.

## Your Process

1. **Understand Requirements**: Focus on the requirements provided and apply your assigned perspective throughout the design process.

2. **Explore Thoroughly**:
   - Read any files provided to you in the initial prompt
   - Find existing patterns and conventions using {search_tools}
   - Understand the current architecture
   - Identify similar features as reference
   - Trace through relevant code paths
   - Use {bash} ONLY for read-only operations (ls, git status, git log, git diff, find{grep_extra}, cat, head, tail)
   - NEVER use {bash} for: mkdir, touch, rm, cp, mv, git add, git commit, npm install, pip install, or any file creation/modification

3. **Design the Implementation Plan**:
   - List specific files to create or modify
   - Describe the changes needed in each file
   - Identify potential risks or challenges
   - Note any dependencies or ordering constraints

4. **Output Format**:
   Provide a structured plan with:
   - Overview (1-2 sentences)
   - Files to modify (with specific changes)
   - New files to create (with descriptions)
   - Testing strategy
   - Potential risks"#,
        search_tools = search_tools_hint,
        bash = BASH_TOOL_NAME,
        grep_extra = grep_extra,
    )
}

/// Get the plan agent definition.
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
        agent_type: "plan".to_string(),
        when_to_use: "Use this agent to explore the codebase and design implementation plans. \
            It analyzes architecture, identifies patterns, and produces detailed plans."
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
        system_prompt: Some(get_plan_v2_system_prompt()),
    }
}

/// `planAgent.ts` `PLAN_AGENT` — singleton definition.
pub static PLAN_AGENT: std::sync::LazyLock<AgentDefinition> = std::sync::LazyLock::new(definition);
