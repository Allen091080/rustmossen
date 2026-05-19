//! # mossen_code_guide_agent — Mossen Code Guide agent built-in definition
//!
//! Translates `tools/AgentTool/built-in/mossenCodeGuideAgent.ts`.

use crate::agent_tool::load_agents_dir::AgentDefinition;
use crate::agent_tool::utils::PermissionMode;

const MOSSEN_CODE_GUIDE_AGENT_TYPE: &str = "mossen-code-guide";
const MOSSEN_DOCS_URL: &str = "https://docs.mossen.ai/en/docs/";
const MOSSEN_PLATFORM_DOCS_URL: &str = "https://docs.mossen.ai/en/api/";

const WEB_FETCH_TOOL_NAME: &str = "WebFetch";
const WEB_SEARCH_TOOL_NAME: &str = "WebSearch";
const FILE_READ_TOOL_NAME: &str = "Read";
const GLOB_TOOL_NAME: &str = "Glob";
const GREP_TOOL_NAME: &str = "Grep";
const BASH_TOOL_NAME: &str = "Bash";
const SEND_MESSAGE_TOOL_NAME: &str = "SendMessage";

fn get_mossen_code_guide_base_prompt() -> String {
    let embedded = std::env::var("MOSSEN_EMBEDDED_SEARCH_TOOLS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let local_search_hint = if embedded {
        format!("`find`/`grep` via {}", BASH_TOOL_NAME)
    } else {
        format!("{}/{}", GLOB_TOOL_NAME, GREP_TOOL_NAME)
    };

    format!(
        r#"You are a knowledgeable guide for Mossen (the AI coding assistant CLI tool) and the Mossen API. Your job is to answer user questions about Mossen features, configuration, usage patterns, and the Mossen API by consulting official documentation.

**Documentation sources:**

- **Mossen docs** ({mossen_docs}): Fetch this for questions about the Mossen CLI tool, including:
  - Installation and setup
  - Configuration (settings.json, MOSSEN.md)
  - Slash commands and keyboard shortcuts
  - Agent system (custom agents, built-in agents)
  - MCP (Model Context Protocol) servers
  - Hooks (pre/post tool use)
  - IDE integrations
  - MCP integration in agents
  - Hosting and deployment
  - Cost tracking and context management

- **Mossen API docs** ({platform_docs}): Fetch this for questions about the Mossen API, including:
  - Messages API and streaming
  - Tool use (function calling)
  - Vision, PDF support, and citations
  - Extended thinking and structured outputs
  - MCP connector for remote MCP servers

**Approach:**
1. Determine which domain the user's question falls into
2. Use {web_fetch} to fetch the appropriate docs map
3. Identify the most relevant documentation URLs from the map
4. Fetch the specific documentation pages
5. Provide clear, actionable guidance based on official documentation
6. Use {web_search} if docs don't cover the topic
7. Reference local project files (MOSSEN.md, .mossen/ directory) when relevant using {local_search}

**Guidelines:**
- Always prioritize official documentation over assumptions
- Keep responses concise and actionable
- Include specific examples or code snippets when helpful
- Reference exact documentation URLs in your responses
- Help users discover features by proactively suggesting related commands, shortcuts, or capabilities"#,
        mossen_docs = MOSSEN_DOCS_URL,
        platform_docs = MOSSEN_PLATFORM_DOCS_URL,
        web_fetch = WEB_FETCH_TOOL_NAME,
        web_search = WEB_SEARCH_TOOL_NAME,
        local_search = local_search_hint,
    )
}

/// Get the Mossen Code Guide agent definition.
pub fn definition() -> AgentDefinition {
    let embedded = std::env::var("MOSSEN_EMBEDDED_SEARCH_TOOLS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let tools = if embedded {
        vec![
            BASH_TOOL_NAME.to_string(),
            FILE_READ_TOOL_NAME.to_string(),
            WEB_FETCH_TOOL_NAME.to_string(),
            WEB_SEARCH_TOOL_NAME.to_string(),
        ]
    } else {
        vec![
            GLOB_TOOL_NAME.to_string(),
            GREP_TOOL_NAME.to_string(),
            FILE_READ_TOOL_NAME.to_string(),
            WEB_FETCH_TOOL_NAME.to_string(),
            WEB_SEARCH_TOOL_NAME.to_string(),
        ]
    };

    AgentDefinition {
        agent_type: MOSSEN_CODE_GUIDE_AGENT_TYPE.to_string(),
        when_to_use: format!(
            "Use this agent when the user asks questions about: (1) Mossen (the CLI tool) - \
             features, hooks, slash commands, MCP servers, settings, IDE integrations; \
             (2) Mossen Agent SDK - building custom agents; (3) Mossen API - API usage, \
             tool use, and SDK usage. **IMPORTANT:** Before spawning a new agent, check if \
             there is already a running or recently completed mossen-code-guide agent that \
             you can continue via {}.",
            SEND_MESSAGE_TOOL_NAME
        ),
        tools: Some(tools),
        disallowed_tools: None,
        skills: None,
        mcp_servers: None,
        hooks: None,
        color: None,
        model: Some("haiku".to_string()),
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
        system_prompt: Some(get_mossen_code_guide_base_prompt()),
    }
}

/// `mossenCodeGuideAgent.ts` `MOSSEN_CODE_GUIDE_AGENT` — the singleton
/// definition built once on first access. Mirrors the TS `export const`.
pub static MOSSEN_CODE_GUIDE_AGENT: std::sync::LazyLock<AgentDefinition> =
    std::sync::LazyLock::new(definition);
