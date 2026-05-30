//! # mossen-tools
//!
//! Mossen 工具系统 — 实现内置工具（ShellExecutor、SourcePatcher、
//! FileInspector、SubagentLauncher 等），对接 Tool trait 定义和工具注册表。

#![allow(
    dead_code,
    unused_assignments,
    unused_imports,
    unused_must_use,
    unused_mut,
    unused_variables,
    clippy::manual_strip,
    clippy::needless_range_loop,
    clippy::nonminimal_bool,
    clippy::regex_creation_in_loops,
    clippy::should_implement_trait
)]

// ── P0 核心工具（13 个） ──────────────────────────────────────────
pub mod agent;
pub mod agent_tool;
pub mod ask_user;
pub mod ask_user_tool;
pub mod bash;
pub mod bash_tool;
pub mod brief;
pub mod brief_tool;
pub mod config_tool;
pub mod cost_query;
pub mod effort_control;
pub mod enter_plan_mode_tool;
pub mod enter_worktree_tool;
pub mod exit;
pub mod exit_plan_mode_tool;
pub mod exit_worktree_tool;
pub mod file_edit;
pub mod file_edit_tool;
pub mod file_read;
pub mod file_read_tool;
pub mod file_write_tool;
pub mod glob_tool;
pub mod grep_tool;
pub mod list_mcp_resources_tool;
pub mod lsp_tool;
pub mod mcp_tool_classify;
pub mod mcp_tool_ext;
pub mod notebook_edit_tool;
pub mod output_style;
pub mod powershell_tool;
pub mod push_notification_tool;
pub mod read_mcp_resource_tool;
pub mod remote_trigger_tool;
pub mod repl;
pub mod repl_tool;
pub mod schedule_cron_tool;
pub mod send_message_tool;
pub mod send_user_file_tool;
pub mod shared;
pub mod skill_discovery;
pub mod skill_tool;
pub mod sleep;
pub mod sleep_tool;
pub mod task_create_tool;
pub mod task_get_tool;
pub mod task_list_tool;
pub mod task_output_tool;
pub mod task_stop_tool;
pub mod task_update_tool;
pub mod team_create_tool;
pub mod team_delete_tool;
pub mod testing;
pub mod todo;
pub mod todo_write_tool;
pub mod tool_search_tool;
pub mod web_fetch_tool;
pub mod web_search_tool;
pub mod workflow_tool;

// ── P1 中等工具 ───────────────────────────────────────────────────
pub mod config;
pub mod enter_plan_mode;
pub mod enter_worktree;
pub mod exit_plan_mode;
pub mod exit_worktree;
pub mod file_write;
pub mod glob;
pub mod grep;
pub mod lsp;
pub mod notebook_edit;
pub mod send_message;
pub mod skill;
pub mod tool_search;
pub mod web_fetch;
pub mod web_search;

// ── P2 辅助工具 ───────────────────────────────────────────────────
pub mod cron_create;
pub mod cron_delete;
pub mod cron_list;
pub mod notification;
pub mod power_shell;
pub mod remote_trigger;
pub mod send_user_file;
pub mod synthetic_output;
pub mod task_create;
pub mod task_get;
pub(crate) mod task_hooks;
pub mod task_list;
pub mod task_output;
pub mod task_stop;
pub mod task_store;
pub mod task_update;
pub mod tool_value_shapes;
pub mod tungsten;

// ── TS-mirror tool aliases ────────────────────────────────────────
// Each TS `const XxxTool = buildTool({...})` export is aliased here to the
// corresponding Rust struct (e.g. `SkillTool` → `skill::CraftInvoker`).
pub mod tool_aliases;

// ── P3 扩展工具 ───────────────────────────────────────────────────
pub mod mcp_auth;
pub mod mcp_list;
pub mod mcp_read;
pub mod mcp_tool;
pub mod team_create;
pub mod team_delete;
pub mod workflow;

use mossen_agent::tool_registry::Tool;

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn internal_tools_enabled() -> bool {
    std::env::var("USER_TYPE").as_deref() == Ok("internal")
}

pub fn send_user_message_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_SEND_USER_MESSAGE_TOOL") || env_truthy("MOSSEN_BRIEF_ONLY")
}

pub fn repl_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_REPL_TOOL") || repl::is_repl_mode_enabled()
}

pub fn web_search_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_WEB_SEARCH_TOOL")
        || std::env::var("MOSSEN_WEB_SEARCH_ENDPOINT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .is_some()
}

pub fn cron_tools_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_CRON_TOOLS")
}

pub fn powershell_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_POWERSHELL_TOOL")
}

pub fn ask_user_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_ASK_USER_TOOL")
}

pub fn tool_search_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_TOOL_SEARCH")
}

pub fn structured_output_tool_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_STRUCTURED_OUTPUT_TOOL")
}

pub fn plan_mode_tools_enabled() -> bool {
    env_truthy("MOSSEN_ENABLE_PLAN_MODE_TOOLS")
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ToolRuntimeOptions {
    pub mcp_resources: bool,
}

#[cfg(test)]
pub(crate) fn dynamic_skill_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .expect("dynamic skill test lock poisoned")
}

/// 获取所有 P0 内置工具实例。
pub fn all_p0_tools() -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = vec![
        Box::new(mossen_agent::goal::GetGoalTool),
        Box::new(mossen_agent::goal::CreateGoalTool),
        Box::new(mossen_agent::goal::UpdateGoalTool),
        Box::new(sleep::DeferralTimer),
        Box::new(todo::TaskNotePad),
        Box::new(agent::SubagentLauncher),
        Box::new(bash::ShellExecutor),
        Box::new(file_edit::SourcePatcher),
        Box::new(file_read::FileInspector),
    ];
    if send_user_message_tool_enabled() {
        tools.push(Box::new(brief::SummaryCard));
    }
    if repl_tool_enabled() {
        tools.push(Box::new(repl::SandboxedRunner));
    }
    if ask_user_tool_enabled() {
        tools.push(Box::new(ask_user::UserProbe));
    }
    tools
}

/// 获取所有 P1 中等优先级工具实例。
pub fn all_p1_tools() -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = vec![
        Box::new(file_write::FileComposer),
        Box::new(glob::PathDiscoverer),
        Box::new(grep::ContentScanner),
        Box::new(notebook_edit::NotebookPatcher),
        Box::new(enter_worktree::BranchIsolator),
        Box::new(exit_worktree::BranchRejoin),
        Box::new(web_fetch::NetRetriever),
        Box::new(skill::CraftInvoker),
    ];
    if plan_mode_tools_enabled() {
        tools.push(Box::new(enter_plan_mode::PlanGate));
        tools.push(Box::new(exit_plan_mode::PlanRelease));
    }
    if web_search_tool_enabled() {
        tools.push(Box::new(web_search::WebExplorer));
    }
    if tool_search_tool_enabled() {
        tools.push(Box::new(tool_search::InstrumentFinder));
    }
    if internal_tools_enabled() {
        tools.push(Box::new(config::SettingsTuner));
    }
    tools
}

/// 获取所有 P2 辅助工具实例。
pub fn all_p2_tools() -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = vec![
        Box::new(notification::AlertDispatcher),
        Box::new(task_create::WorkItemForge),
        Box::new(task_get::WorkItemQuery),
        Box::new(task_list::WorkItemIndex),
        Box::new(task_update::WorkItemMutator),
        Box::new(task_stop::HaltSignal),
        Box::new(task_output::ResultEmitter),
        Box::new(send_user_file::FileDelivery),
    ];
    if structured_output_tool_enabled() {
        tools.push(Box::new(synthetic_output::StructuredEmitter));
    }
    if powershell_tool_enabled() {
        tools.push(Box::new(power_shell::WinShellExecutor));
    }
    if cron_tools_enabled() {
        tools.push(Box::new(cron_create::ScheduleForge));
        tools.push(Box::new(cron_delete::ScheduleRevoke));
        tools.push(Box::new(cron_list::ScheduleIndex));
    }
    tools
}

/// 获取所有 P3 扩展工具实例。
pub fn all_p3_tools() -> Vec<Box<dyn Tool>> {
    all_p3_tools_for_runtime(ToolRuntimeOptions::default())
}

pub fn all_p3_tools_for_runtime(options: ToolRuntimeOptions) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = Vec::new();
    if options.mcp_resources {
        tools.push(Box::new(mcp_list::BridgeInventory));
        tools.push(Box::new(mcp_read::BridgeReader));
    }
    tools
}

/// 获取所有工具实例（P0 + P1 + P2 + P3）。
pub fn all_tools() -> Vec<Box<dyn Tool>> {
    all_tools_for_runtime(ToolRuntimeOptions::default())
}

pub fn all_tools_for_runtime(options: ToolRuntimeOptions) -> Vec<Box<dyn Tool>> {
    let mut tools = all_p0_tools();
    tools.extend(all_p1_tools());
    tools.extend(all_p2_tools());
    tools.extend(all_p3_tools_for_runtime(options));
    tools
}

pub fn all_tool_names_for_runtime(options: ToolRuntimeOptions) -> Vec<String> {
    all_tools_for_runtime(options)
        .into_iter()
        .map(|tool| tool.name().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn tool_names() -> std::collections::HashSet<String> {
        all_tools()
            .into_iter()
            .map(|tool| tool.name().to_string())
            .collect()
    }

    fn tool_names_for_runtime(options: ToolRuntimeOptions) -> std::collections::HashSet<String> {
        all_tools_for_runtime(options)
            .into_iter()
            .map(|tool| tool.name().to_string())
            .collect()
    }

    #[test]
    fn personal_default_tool_surface_excludes_unwired_optional_tools() {
        let _lock = dynamic_skill_test_lock();
        let _lsp = EnvRestore::remove("ENABLE_LSP_TOOL");
        let _teams = EnvRestore::remove("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS");
        let _remote = EnvRestore::remove("MOSSEN_ENABLE_REMOTE_TRIGGER_TOOL");
        let _workflow = EnvRestore::remove("MOSSEN_ENABLE_WORKFLOW_TOOL");
        let _web_search = EnvRestore::remove("MOSSEN_ENABLE_WEB_SEARCH_TOOL");
        let _web_search_endpoint = EnvRestore::remove("MOSSEN_WEB_SEARCH_ENDPOINT");
        let _cron = EnvRestore::remove("MOSSEN_ENABLE_CRON_TOOLS");
        let _powershell = EnvRestore::remove("MOSSEN_ENABLE_POWERSHELL_TOOL");
        let _ask_user = EnvRestore::remove("MOSSEN_ENABLE_ASK_USER_TOOL");
        let _send_user_message = EnvRestore::remove("MOSSEN_ENABLE_SEND_USER_MESSAGE_TOOL");
        let _brief_only = EnvRestore::remove("MOSSEN_BRIEF_ONLY");
        let _repl_tool = EnvRestore::remove("MOSSEN_ENABLE_REPL_TOOL");
        let _repl_mode = EnvRestore::remove("MOSSEN_REPL_MODE");
        let _code_repl = EnvRestore::remove("MOSSEN_CODE_REPL");
        let _tool_search = EnvRestore::remove("MOSSEN_ENABLE_TOOL_SEARCH");
        let _structured_output = EnvRestore::remove("MOSSEN_ENABLE_STRUCTURED_OUTPUT_TOOL");
        let _plan_mode_tools = EnvRestore::remove("MOSSEN_ENABLE_PLAN_MODE_TOOLS");
        let _user_type = EnvRestore::remove("USER_TYPE");

        let names = tool_names();
        for hidden in [
            "AskUserQuestion",
            "Config",
            "CronCreate",
            "CronDelete",
            "CronList",
            "CostQuery",
            "EffortControl",
            "EnterPlanMode",
            "Exit",
            "ExitPlanMode",
            "LSP",
            "ListMcpResources",
            "OutputStyle",
            "PowerShell",
            "ReadMcpResource",
            "REPL",
            "SendUserMessage",
            "SendMessage",
            "StructuredOutput",
            "TeamCreate",
            "TeamDelete",
            "ToolSearch",
            "RemoteTrigger",
            "Tungsten",
            "WebSearch",
            "Workflow",
        ] {
            assert!(
                !names.contains(hidden),
                "{hidden} must not be exposed in the personal default tool surface"
            );
        }
    }

    #[test]
    fn personal_default_tool_definitions_do_not_surface_unfinished_text() {
        let _lock = dynamic_skill_test_lock();
        let _lsp = EnvRestore::remove("ENABLE_LSP_TOOL");
        let _teams = EnvRestore::remove("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS");
        let _remote = EnvRestore::remove("MOSSEN_ENABLE_REMOTE_TRIGGER_TOOL");
        let _workflow = EnvRestore::remove("MOSSEN_ENABLE_WORKFLOW_TOOL");
        let _web_search = EnvRestore::remove("MOSSEN_ENABLE_WEB_SEARCH_TOOL");
        let _web_search_endpoint = EnvRestore::remove("MOSSEN_WEB_SEARCH_ENDPOINT");
        let _cron = EnvRestore::remove("MOSSEN_ENABLE_CRON_TOOLS");
        let _powershell = EnvRestore::remove("MOSSEN_ENABLE_POWERSHELL_TOOL");
        let _ask_user = EnvRestore::remove("MOSSEN_ENABLE_ASK_USER_TOOL");
        let _send_user_message = EnvRestore::remove("MOSSEN_ENABLE_SEND_USER_MESSAGE_TOOL");
        let _brief_only = EnvRestore::remove("MOSSEN_BRIEF_ONLY");
        let _repl_tool = EnvRestore::remove("MOSSEN_ENABLE_REPL_TOOL");
        let _repl_mode = EnvRestore::remove("MOSSEN_REPL_MODE");
        let _code_repl = EnvRestore::remove("MOSSEN_CODE_REPL");
        let _tool_search = EnvRestore::remove("MOSSEN_ENABLE_TOOL_SEARCH");
        let _structured_output = EnvRestore::remove("MOSSEN_ENABLE_STRUCTURED_OUTPUT_TOOL");
        let _plan_mode_tools = EnvRestore::remove("MOSSEN_ENABLE_PLAN_MODE_TOOLS");
        let _user_type = EnvRestore::remove("USER_TYPE");

        let forbidden_terms = [
            "stub",
            "placeholder",
            "not implemented",
            "unimplemented",
            "unavailable",
            "reconstructed source build",
            "sendmessage",
            "teamcreate",
            "teamdelete",
            "teammate",
            "swarm",
            "remote session",
            "remote sessions",
            "hosted",
        ];
        for tool in all_tools() {
            let definition = tool.definition();
            let rendered = serde_json::to_string(&definition).expect("tool definition json");
            let lowered = rendered.to_ascii_lowercase();
            for term in forbidden_terms {
                assert!(
                    !lowered.contains(term),
                    "{} definition surfaced unfinished text `{}`: {}",
                    definition.name,
                    term,
                    rendered
                );
            }
        }

        let task_output_prompt = crate::task_output_tool::prompt::build_prompt().to_lowercase();
        for term in forbidden_terms {
            assert!(
                !task_output_prompt.contains(term),
                "TaskOutput prompt surfaced personal-version-out-of-scope text `{}`: {}",
                term,
                task_output_prompt
            );
        }
    }

    #[test]
    fn explicit_feature_gates_expose_wired_optional_tools_and_keep_unwired_hidden() {
        let _lock = dynamic_skill_test_lock();
        let _lsp = EnvRestore::set("ENABLE_LSP_TOOL", "1");
        let _teams = EnvRestore::set("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS", "1");
        let _remote = EnvRestore::set("MOSSEN_ENABLE_REMOTE_TRIGGER_TOOL", "1");
        let _workflow = EnvRestore::set("MOSSEN_ENABLE_WORKFLOW_TOOL", "1");
        let _web_search = EnvRestore::set("MOSSEN_ENABLE_WEB_SEARCH_TOOL", "1");
        let _cron = EnvRestore::set("MOSSEN_ENABLE_CRON_TOOLS", "1");
        let _powershell = EnvRestore::set("MOSSEN_ENABLE_POWERSHELL_TOOL", "1");
        let _ask_user = EnvRestore::set("MOSSEN_ENABLE_ASK_USER_TOOL", "1");
        let _send_user_message = EnvRestore::set("MOSSEN_ENABLE_SEND_USER_MESSAGE_TOOL", "1");
        let _repl_tool = EnvRestore::set("MOSSEN_ENABLE_REPL_TOOL", "1");
        let _tool_search = EnvRestore::set("MOSSEN_ENABLE_TOOL_SEARCH", "1");
        let _structured_output = EnvRestore::set("MOSSEN_ENABLE_STRUCTURED_OUTPUT_TOOL", "1");
        let _plan_mode_tools = EnvRestore::set("MOSSEN_ENABLE_PLAN_MODE_TOOLS", "1");
        let _user_type = EnvRestore::set("USER_TYPE", "internal");

        let names = tool_names();
        for exposed in [
            "AskUserQuestion",
            "Config",
            "CronCreate",
            "CronDelete",
            "CronList",
            "EnterPlanMode",
            "ExitPlanMode",
            "PowerShell",
            "REPL",
            "SendUserMessage",
            "StructuredOutput",
            "ToolSearch",
            "WebSearch",
        ] {
            assert!(
                names.contains(exposed),
                "{exposed} should be available only after its explicit feature gate"
            );
        }

        for hidden in [
            "LSP",
            "RemoteTrigger",
            "SendMessage",
            "TeamCreate",
            "TeamDelete",
            "Tungsten",
            "Workflow",
        ] {
            assert!(
                !names.contains(hidden),
                "{hidden} must stay hidden until its execution path is wired"
            );
        }
    }

    #[test]
    fn mcp_resource_tools_require_runtime_opt_in() {
        let names = tool_names();
        assert!(!names.contains("ListMcpResources"));
        assert!(!names.contains("ReadMcpResource"));

        let names: std::collections::HashSet<String> = all_tools_for_runtime(ToolRuntimeOptions {
            mcp_resources: true,
        })
        .into_iter()
        .map(|tool| tool.name().to_string())
        .collect();
        assert!(names.contains("ListMcpResources"));
        assert!(names.contains("ReadMcpResource"));
    }

    #[test]
    fn tools_index_default_names_are_executable_registry_tools() {
        let _lock = dynamic_skill_test_lock();
        let _lsp = EnvRestore::remove("ENABLE_LSP_TOOL");
        let _teams = EnvRestore::remove("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS");
        let _web_search = EnvRestore::remove("MOSSEN_ENABLE_WEB_SEARCH_TOOL");
        let _web_search_endpoint = EnvRestore::remove("MOSSEN_WEB_SEARCH_ENDPOINT");
        let _cron = EnvRestore::remove("MOSSEN_ENABLE_CRON_TOOLS");
        let _ask_user = EnvRestore::remove("MOSSEN_ENABLE_ASK_USER_TOOL");
        let _send_user_message = EnvRestore::remove("MOSSEN_ENABLE_SEND_USER_MESSAGE_TOOL");
        let _brief_only = EnvRestore::remove("MOSSEN_BRIEF_ONLY");
        let _plan_mode_tools = EnvRestore::remove("MOSSEN_ENABLE_PLAN_MODE_TOOLS");
        let _mcp_resources = EnvRestore::remove("MOSSEN_ENABLE_MCP_RESOURCE_TOOLS");
        let _worktree_mode = EnvRestore::remove("MOSSEN_WORKTREE_MODE");
        let _user_type = EnvRestore::remove("USER_TYPE");

        let registry = tool_names_for_runtime(ToolRuntimeOptions::default());
        let index = mossen_agent::tools_index::get_all_base_tools();
        for tool in index {
            assert!(
                registry.contains(&tool.name),
                "tools_index exposed {} but the executable registry did not contain it",
                tool.name
            );
        }
    }

    #[test]
    fn tools_index_opt_in_names_are_executable_registry_tools() {
        let _lock = dynamic_skill_test_lock();
        let _web_search = EnvRestore::set("MOSSEN_ENABLE_WEB_SEARCH_TOOL", "1");
        let _cron = EnvRestore::set("MOSSEN_ENABLE_CRON_TOOLS", "1");
        let _ask_user = EnvRestore::set("MOSSEN_ENABLE_ASK_USER_TOOL", "1");
        let _send_user_message = EnvRestore::set("MOSSEN_ENABLE_SEND_USER_MESSAGE_TOOL", "1");
        let _plan_mode_tools = EnvRestore::set("MOSSEN_ENABLE_PLAN_MODE_TOOLS", "1");
        let _mcp_resources = EnvRestore::set("MOSSEN_ENABLE_MCP_RESOURCE_TOOLS", "1");
        let _worktree_mode = EnvRestore::set("MOSSEN_WORKTREE_MODE", "1");
        let _user_type = EnvRestore::set("USER_TYPE", "internal");

        let registry = tool_names_for_runtime(ToolRuntimeOptions {
            mcp_resources: true,
        });
        let index = mossen_agent::tools_index::get_all_base_tools();
        for tool in index {
            assert!(
                registry.contains(&tool.name),
                "tools_index exposed {} but the executable registry did not contain it",
                tool.name
            );
        }
    }
}
