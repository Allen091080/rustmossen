//! Top-level tool index — Rust mirror of `tools.ts`.
//!
//! `tools.ts` enumerates the entire built-in toolset and exposes the
//! preset/deny-filter helpers used to derive a concrete tool list per
//! permission context. The Rust port keeps the same names so call sites
//! ported from TS find their counterparts without rewriting.

use std::collections::HashSet;

/// `tools.ts` `TOOL_PRESETS`.
pub const TOOL_PRESETS: &[&str] = &["default"];

/// `tools.ts` `ToolPreset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolPreset {
    Default,
}

impl ToolPreset {
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolPreset::Default => "default",
        }
    }
}

/// `tools.ts` `REPL_ONLY_TOOLS` — primitives hidden by the REPL tool when
/// REPL mode is on (they are still accessible inside the REPL VM).
pub const REPL_ONLY_TOOLS: &[&str] = &[
    "Bash",
    "Read",
    "Edit",
    "Write",
    "Glob",
    "Grep",
    "NotebookEdit",
];

/// `tools.ts` `parseToolPreset`.
pub fn parse_tool_preset(preset: &str) -> Option<ToolPreset> {
    let lower = preset.to_lowercase();
    match lower.as_str() {
        "default" => Some(ToolPreset::Default),
        _ => None,
    }
}

/// Lightweight tool descriptor used by `tools.rs` filters.
#[derive(Debug, Clone, Default)]
pub struct ToolDescriptor {
    pub name: String,
    pub aliases: Vec<String>,
    pub enabled: bool,
    pub mcp_server_name: Option<String>,
    pub mcp_tool_name: Option<String>,
}

impl ToolDescriptor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            enabled: true,
            mcp_server_name: None,
            mcp_tool_name: None,
        }
    }
}

/// `tools.ts` `getToolsForDefaultPreset`.
pub fn get_tools_for_default_preset() -> Vec<String> {
    get_all_base_tools()
        .into_iter()
        .filter(|t| t.enabled)
        .map(|t| t.name)
        .collect()
}

/// `tools.ts` `getAllBaseTools` — the canonical list of built-in tools.
/// The Rust port enumerates the static names; runtime gating (feature flags,
/// USER_TYPE=internal) is applied via `enabled`.
pub fn get_all_base_tools() -> Vec<ToolDescriptor> {
    let user_type_internal = std::env::var("USER_TYPE").as_deref() == Ok("internal");
    let has_embedded = std::env::var("MOSSEN_EMBEDDED_SEARCH").as_deref() == Ok("1");
    let lsp_enabled = matches!(
        std::env::var("ENABLE_LSP_TOOL").as_deref(),
        Ok("1" | "true" | "TRUE")
    );
    let worktree_enabled = matches!(
        std::env::var("MOSSEN_WORKTREE_MODE").as_deref(),
        Ok("1" | "true" | "TRUE")
    );

    let mut tools: Vec<ToolDescriptor> = vec![
        ToolDescriptor::new("Task"), // AgentTool
        ToolDescriptor::new("TaskOutput"),
        ToolDescriptor::new("Bash"),
    ];
    if !has_embedded {
        tools.push(ToolDescriptor::new("Glob"));
        tools.push(ToolDescriptor::new("Grep"));
    }
    tools.extend([
        ToolDescriptor::new("ExitPlanMode"),
        ToolDescriptor::new("Read"),
        ToolDescriptor::new("Edit"),
        ToolDescriptor::new("Write"),
        ToolDescriptor::new("NotebookEdit"),
        ToolDescriptor::new("WebFetch"),
        ToolDescriptor::new("TodoWrite"),
        ToolDescriptor::new("WebSearch"),
        ToolDescriptor::new("TaskStop"),
        ToolDescriptor::new("AskUserQuestion"),
        ToolDescriptor::new("Skill"),
        ToolDescriptor::new("EnterPlanMode"),
    ]);
    if user_type_internal {
        tools.push(ToolDescriptor::new("Config"));
        tools.push(ToolDescriptor::new("Tungsten"));
    }
    if lsp_enabled {
        tools.push(ToolDescriptor::new("LSP"));
    }
    if worktree_enabled {
        tools.push(ToolDescriptor::new("EnterWorktree"));
        tools.push(ToolDescriptor::new("ExitWorktree"));
    }
    tools.extend([
        ToolDescriptor::new("SendMessage"),
        ToolDescriptor::new("Brief"),
        ToolDescriptor::new("CronCreate"),
        ToolDescriptor::new("CronDelete"),
        ToolDescriptor::new("CronList"),
        ToolDescriptor::new("ListMcpResources"),
        ToolDescriptor::new("ReadMcpResource"),
    ]);
    tools
}

/// `tools.ts` `filterToolsByDenyRules` — strips tools matched by blanket
/// deny rules in the permission context.
pub fn filter_tools_by_deny_rules<T: Clone>(
    tools: &[T],
    name_of: impl Fn(&T) -> &str,
    deny_names: &HashSet<String>,
) -> Vec<T> {
    tools
        .iter()
        .filter(|t| !deny_names.contains(name_of(t)))
        .cloned()
        .collect()
}

/// `tools.ts` `getTools` — produce the post-filter tool list. Honors:
///   - `MOSSEN_CODE_SIMPLE` simple-mode subset.
///   - deny rules from the permission context.
pub fn get_tools(deny_names: &HashSet<String>, simple_mode: bool) -> Vec<ToolDescriptor> {
    if simple_mode {
        let simple = vec![
            ToolDescriptor::new("Bash"),
            ToolDescriptor::new("Read"),
            ToolDescriptor::new("Edit"),
        ];
        return filter_tools_by_deny_rules(&simple, |t| &t.name, deny_names);
    }
    let special: HashSet<&str> = ["ListMcpResources", "ReadMcpResource", "synthetic_output"]
        .into_iter()
        .collect();
    let base: Vec<ToolDescriptor> = get_all_base_tools()
        .into_iter()
        .filter(|t| !special.contains(t.name.as_str()))
        .collect();
    filter_tools_by_deny_rules(&base, |t| &t.name, deny_names)
        .into_iter()
        .filter(|t| t.enabled)
        .collect()
}

/// `tools.ts` `assembleToolPool` — built-in tools + MCP tools, de-duplicated
/// by name (built-ins win).
pub fn assemble_tool_pool(
    builtin: Vec<ToolDescriptor>,
    mcp_tools: Vec<ToolDescriptor>,
    deny_names: &HashSet<String>,
) -> Vec<ToolDescriptor> {
    let allowed_mcp = filter_tools_by_deny_rules(&mcp_tools, |t| &t.name, deny_names);
    let mut sorted_builtin = builtin;
    sorted_builtin.sort_by(|a, b| a.name.cmp(&b.name));
    let mut sorted_mcp = allowed_mcp;
    sorted_mcp.sort_by(|a, b| a.name.cmp(&b.name));
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(sorted_builtin.len() + sorted_mcp.len());
    for t in sorted_builtin.into_iter().chain(sorted_mcp.into_iter()) {
        if seen.insert(t.name.clone()) {
            out.push(t);
        }
    }
    out
}

/// `tools.ts` `getMergedTools` — built-in tools + MCP tools (no dedup).
pub fn get_merged_tools(
    builtin: Vec<ToolDescriptor>,
    mcp_tools: Vec<ToolDescriptor>,
) -> Vec<ToolDescriptor> {
    let mut out = builtin;
    out.extend(mcp_tools);
    out
}

/// `tools.ts` `ALL_AGENT_DISALLOWED_TOOLS`.
pub const ALL_AGENT_DISALLOWED_TOOLS: &[&str] =
    &["Task", "ExitPlanMode", "EnterPlanMode", "TodoWrite"];

/// `tools.ts` `CUSTOM_AGENT_DISALLOWED_TOOLS`.
pub const CUSTOM_AGENT_DISALLOWED_TOOLS: &[&str] = &["Task", "EnterPlanMode", "ExitPlanMode"];

/// `tools.ts` `ASYNC_AGENT_ALLOWED_TOOLS`.
pub const ASYNC_AGENT_ALLOWED_TOOLS: &[&str] = &[
    "Read",
    "Glob",
    "Grep",
    "WebFetch",
    "WebSearch",
    "TaskOutput",
];

/// `tools.ts` `COORDINATOR_MODE_ALLOWED_TOOLS`.
pub const COORDINATOR_MODE_ALLOWED_TOOLS: &[&str] = &[
    "Task",
    "TaskStop",
    "TaskOutput",
    "SendMessage",
    "Read",
    "Grep",
    "Glob",
];
