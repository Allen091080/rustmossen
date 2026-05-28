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

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn web_search_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_WEB_SEARCH_TOOL")
        || std::env::var("MOSSEN_WEB_SEARCH_ENDPOINT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .is_some()
}

fn cron_tools_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_CRON_TOOLS")
}

fn ask_user_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_ASK_USER_TOOL")
}

fn send_user_message_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_SEND_USER_MESSAGE_TOOL") || env_truthy("MOSSEN_BRIEF_ONLY")
}

fn plan_mode_tools_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_PLAN_MODE_TOOLS")
}

fn mcp_resource_tools_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_MCP_RESOURCE_TOOLS")
}

/// `tools.ts` `getAllBaseTools` — the canonical list of built-in tools.
/// The Rust port enumerates the static names; runtime gating (feature flags,
/// USER_TYPE=internal) is applied via `enabled`.
pub fn get_all_base_tools() -> Vec<ToolDescriptor> {
    let user_type_internal = std::env::var("USER_TYPE").as_deref() == Ok("internal");
    let has_embedded = std::env::var("MOSSEN_EMBEDDED_SEARCH").as_deref() == Ok("1");
    let worktree_enabled = matches!(
        std::env::var("MOSSEN_WORKTREE_MODE").as_deref(),
        Ok("1" | "true" | "TRUE")
    );

    let mut tools: Vec<ToolDescriptor> = vec![
        ToolDescriptor::new("Agent"),
        ToolDescriptor::new("TaskOutput"),
        ToolDescriptor::new("Bash"),
    ];
    if !has_embedded {
        tools.push(ToolDescriptor::new("Glob"));
        tools.push(ToolDescriptor::new("Grep"));
    }
    tools.extend([
        ToolDescriptor::new("Read"),
        ToolDescriptor::new("Edit"),
        ToolDescriptor::new("Write"),
        ToolDescriptor::new("NotebookEdit"),
        ToolDescriptor::new("WebFetch"),
        ToolDescriptor::new("TodoWrite"),
        ToolDescriptor::new("TaskStop"),
        ToolDescriptor::new("Skill"),
    ]);
    if plan_mode_tools_enabled() {
        tools.push(ToolDescriptor::new("EnterPlanMode"));
        tools.push(ToolDescriptor::new("ExitPlanMode"));
    }
    if send_user_message_tool_enabled() {
        tools.push(ToolDescriptor::new("SendUserMessage"));
    }
    if ask_user_tool_enabled() {
        tools.push(ToolDescriptor::new("AskUserQuestion"));
    }
    if web_search_tool_enabled() {
        tools.push(ToolDescriptor::new("WebSearch"));
    }
    if user_type_internal {
        tools.push(ToolDescriptor::new("Config"));
    }
    if worktree_enabled {
        tools.push(ToolDescriptor::new("EnterWorktree"));
        tools.push(ToolDescriptor::new("ExitWorktree"));
    }
    if mcp_resource_tools_enabled() {
        tools.push(ToolDescriptor::new("ListMcpResources"));
        tools.push(ToolDescriptor::new("ReadMcpResource"));
    }
    if cron_tools_enabled() {
        tools.push(ToolDescriptor::new("CronCreate"));
        tools.push(ToolDescriptor::new("CronDelete"));
        tools.push(ToolDescriptor::new("CronList"));
    }
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
pub const ALL_AGENT_DISALLOWED_TOOLS: &[&str] = &[
    "Agent",
    "Task",
    "ExitPlanMode",
    "EnterPlanMode",
    "TodoWrite",
];

/// `tools.ts` `CUSTOM_AGENT_DISALLOWED_TOOLS`.
pub const CUSTOM_AGENT_DISALLOWED_TOOLS: &[&str] =
    &["Agent", "Task", "EnterPlanMode", "ExitPlanMode"];

/// `tools.ts` `ASYNC_AGENT_ALLOWED_TOOLS`.
pub const ASYNC_AGENT_ALLOWED_TOOLS: &[&str] = &["Read", "Glob", "Grep", "WebFetch", "TaskOutput"];

/// `tools.ts` `COORDINATOR_MODE_ALLOWED_TOOLS`.
pub const COORDINATOR_MODE_ALLOWED_TOOLS: &[&str] = &[
    "Agent",
    "Task",
    "TaskStop",
    "TaskOutput",
    "Read",
    "Grep",
    "Glob",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    struct EnvRestore {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvRestore {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }

        fn remove(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn tool_names() -> HashSet<String> {
        get_all_base_tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect()
    }

    #[test]
    fn personal_default_index_excludes_unwired_optional_tools() {
        let _lock = env_lock();
        let _web = EnvRestore::remove("MOSSEN_ENABLE_WEB_SEARCH_TOOL");
        let _web_endpoint = EnvRestore::remove("MOSSEN_WEB_SEARCH_ENDPOINT");
        let _cron = EnvRestore::remove("MOSSEN_ENABLE_CRON_TOOLS");
        let _ask_user = EnvRestore::remove("MOSSEN_ENABLE_ASK_USER_TOOL");
        let _send_user_message = EnvRestore::remove("MOSSEN_ENABLE_SEND_USER_MESSAGE_TOOL");
        let _brief_only = EnvRestore::remove("MOSSEN_BRIEF_ONLY");
        let _plan_mode = EnvRestore::remove("MOSSEN_ENABLE_PLAN_MODE_TOOLS");
        let _mcp_resources = EnvRestore::remove("MOSSEN_ENABLE_MCP_RESOURCE_TOOLS");
        let _lsp = EnvRestore::remove("ENABLE_LSP_TOOL");
        let _teams = EnvRestore::remove("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS");
        let _user_type = EnvRestore::remove("USER_TYPE");

        let names = tool_names();
        for hidden in [
            "AskUserQuestion",
            "Brief",
            "EnterPlanMode",
            "ExitPlanMode",
            "ListMcpResources",
            "ReadMcpResource",
            "SendUserMessage",
            "SendMessage",
            "Tungsten",
            "LSP",
            "WebSearch",
            "CronCreate",
            "CronDelete",
            "CronList",
        ] {
            assert!(
                !names.contains(hidden),
                "{hidden} must not be in the personal default tool index"
            );
        }
    }

    #[test]
    fn explicit_index_gates_expose_web_search_and_cron_tools() {
        let _lock = env_lock();
        let _web = EnvRestore::set("MOSSEN_ENABLE_WEB_SEARCH_TOOL", "1");
        let _cron = EnvRestore::set("MOSSEN_ENABLE_CRON_TOOLS", "1");
        let _ask_user = EnvRestore::set("MOSSEN_ENABLE_ASK_USER_TOOL", "1");
        let _send_user_message = EnvRestore::set("MOSSEN_ENABLE_SEND_USER_MESSAGE_TOOL", "1");
        let _plan_mode = EnvRestore::set("MOSSEN_ENABLE_PLAN_MODE_TOOLS", "1");
        let _mcp_resources = EnvRestore::set("MOSSEN_ENABLE_MCP_RESOURCE_TOOLS", "1");
        let _lsp = EnvRestore::set("ENABLE_LSP_TOOL", "1");
        let _teams = EnvRestore::set("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS", "1");
        let _user_type = EnvRestore::set("USER_TYPE", "internal");

        let names = tool_names();
        for exposed in [
            "AskUserQuestion",
            "Config",
            "EnterPlanMode",
            "ExitPlanMode",
            "ListMcpResources",
            "ReadMcpResource",
            "SendUserMessage",
            "WebSearch",
            "CronCreate",
            "CronDelete",
            "CronList",
        ] {
            assert!(
                names.contains(exposed),
                "{exposed} should be present after its explicit feature gate"
            );
        }
        for hidden in [
            "LSP",
            "SendMessage",
            "Tungsten",
            "Workflow",
            "RemoteTrigger",
        ] {
            assert!(
                !names.contains(hidden),
                "{hidden} must stay out of the index until its runtime path is wired"
            );
        }
    }

    #[test]
    fn coordinator_allowlist_excludes_unwired_personal_runtime_tools() {
        for hidden in [
            "SendMessage",
            "TeamCreate",
            "TeamDelete",
            "Workflow",
            "RemoteTrigger",
        ] {
            assert!(
                !COORDINATOR_MODE_ALLOWED_TOOLS.contains(&hidden),
                "{hidden} must not be advertised as coordinator-allowed in the personal runtime"
            );
        }
    }
}
