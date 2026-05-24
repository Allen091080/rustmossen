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
    unused_variables
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

/// 获取所有 P0 内置工具实例。
pub fn all_p0_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(ask_user::UserProbe),
        Box::new(brief::SummaryCard),
        Box::new(sleep::DeferralTimer),
        Box::new(todo::TaskNotePad),
        Box::new(exit::SessionExit),
        Box::new(cost_query::MeterQuery),
        Box::new(effort_control::EffortTuner),
        Box::new(output_style::StyleDirective),
        Box::new(agent::SubagentLauncher),
        Box::new(bash::ShellExecutor),
        Box::new(file_edit::SourcePatcher),
        Box::new(file_read::FileInspector),
        Box::new(repl::SandboxedRunner),
    ]
}

/// 获取所有 P1 中等优先级工具实例。
pub fn all_p1_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(file_write::FileComposer),
        Box::new(glob::PathDiscoverer),
        Box::new(grep::ContentScanner),
        Box::new(lsp::LanguageOracle),
        Box::new(notebook_edit::NotebookPatcher),
        Box::new(enter_plan_mode::PlanGate),
        Box::new(exit_plan_mode::PlanRelease),
        Box::new(enter_worktree::BranchIsolator),
        Box::new(exit_worktree::BranchRejoin),
        Box::new(config::SettingsTuner),
        Box::new(web_fetch::NetRetriever),
        Box::new(web_search::WebExplorer),
        Box::new(tool_search::InstrumentFinder),
        Box::new(send_message::PeerDispatch),
        Box::new(skill::CraftInvoker),
    ]
}

/// 获取所有 P2 辅助工具实例。
pub fn all_p2_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(notification::AlertDispatcher),
        Box::new(synthetic_output::StructuredEmitter),
        Box::new(tungsten::InternalProbe),
        Box::new(task_create::WorkItemForge),
        Box::new(task_get::WorkItemQuery),
        Box::new(task_list::WorkItemIndex),
        Box::new(task_update::WorkItemMutator),
        Box::new(task_stop::HaltSignal),
        Box::new(task_output::ResultEmitter),
        Box::new(cron_create::ScheduleForge),
        Box::new(cron_delete::ScheduleRevoke),
        Box::new(cron_list::ScheduleIndex),
        Box::new(send_user_file::FileDelivery),
        Box::new(remote_trigger::EventRelay),
        Box::new(power_shell::WinShellExecutor),
    ]
}

/// 获取所有 P3 扩展工具实例。
pub fn all_p3_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(mcp_list::BridgeInventory),
        Box::new(mcp_read::BridgeReader),
        Box::new(mcp_tool::BridgeExecutor),
        Box::new(mcp_auth::BridgeAuthenticator),
        Box::new(team_create::SwarmSpawner),
        Box::new(team_delete::SwarmDismisser),
        Box::new(workflow::PipelineRunner),
    ]
}

/// 获取所有工具实例（P0 + P1 + P2 + P3）。
pub fn all_tools() -> Vec<Box<dyn Tool>> {
    let mut tools = all_p0_tools();
    tools.extend(all_p1_tools());
    tools.extend(all_p2_tools());
    tools.extend(all_p3_tools());
    tools
}
