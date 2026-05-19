//! # tool_aliases — TS-mirror const exports for tool singletons.
//!
//! TypeScript exports each tool as a `const XxxTool = buildTool({...})` value.
//! Rust uses distinct struct names per tool (e.g. `SourcePatcher` for FileEditTool).
//! This module exposes type aliases matching the original TS export names so that
//! downstream consumers (and the export-coverage scanner) can refer to them
//! using the canonical TypeScript identifiers.
//!
//! Every alias resolves to a real, registered Tool struct that implements the
//! `mossen_agent::tool_registry::Tool` trait, and a zero-arg factory function
//! returning a fresh instance is provided alongside each alias.

#![allow(non_camel_case_types)]

// ── Tool type aliases — match TS `const XxxTool` export names ────────────────

/// `tools/SkillTool/SkillTool.ts` `SkillTool` — alias of `skill::CraftInvoker`.
pub type SkillTool = crate::skill::CraftInvoker;
/// Factory: returns a fresh `SkillTool` instance.
pub fn skill_tool() -> SkillTool { crate::skill::CraftInvoker }

/// `tools/FileEditTool/FileEditTool.ts` `FileEditTool` — alias of `file_edit::SourcePatcher`.
pub type FileEditTool = crate::file_edit::SourcePatcher;
pub fn file_edit_tool() -> FileEditTool { crate::file_edit::SourcePatcher }

/// `tools/GrepTool/GrepTool.ts` `GrepTool` — alias of `grep::ContentScanner`.
pub type GrepTool = crate::grep::ContentScanner;
pub fn grep_tool() -> GrepTool { crate::grep::ContentScanner }

/// `tools/TaskOutputTool/TaskOutputTool.tsx` `TaskOutputTool` — alias of `task_output::ResultEmitter`.
pub type TaskOutputTool = crate::task_output::ResultEmitter;
pub fn task_output_tool() -> TaskOutputTool { crate::task_output::ResultEmitter }

/// `tools/ExitPlanModeTool/ExitPlanModeV2Tool.ts` `ExitPlanModeV2Tool` — alias of `exit_plan_mode::PlanRelease`.
pub type ExitPlanModeV2Tool = crate::exit_plan_mode::PlanRelease;
pub fn exit_plan_mode_v2_tool() -> ExitPlanModeV2Tool { crate::exit_plan_mode::PlanRelease }

/// `tools/NotebookEditTool/NotebookEditTool.ts` `NotebookEditTool` — alias of `notebook_edit::NotebookPatcher`.
pub type NotebookEditTool = crate::notebook_edit::NotebookPatcher;
pub fn notebook_edit_tool() -> NotebookEditTool { crate::notebook_edit::NotebookPatcher }

/// `tools/ToolSearchTool/ToolSearchTool.ts` `ToolSearchTool` — alias of `tool_search::InstrumentFinder`.
pub type ToolSearchTool = crate::tool_search::InstrumentFinder;
pub fn tool_search_tool() -> ToolSearchTool { crate::tool_search::InstrumentFinder }

/// `tools/WebSearchTool/WebSearchTool.ts` `WebSearchTool` — alias of `web_search::WebExplorer`.
pub type WebSearchTool = crate::web_search::WebExplorer;
pub fn web_search_tool() -> WebSearchTool { crate::web_search::WebExplorer }

/// `tools/ConfigTool/ConfigTool.ts` `ConfigTool` — alias of `config::SettingsTuner`.
pub type ConfigTool = crate::config::SettingsTuner;
pub fn config_tool() -> ConfigTool { crate::config::SettingsTuner }

/// `tools/FileWriteTool/FileWriteTool.ts` `FileWriteTool` — alias of `file_write::FileComposer`.
pub type FileWriteTool = crate::file_write::FileComposer;
pub fn file_write_tool() -> FileWriteTool { crate::file_write::FileComposer }

/// `tools/TaskUpdateTool/TaskUpdateTool.ts` `TaskUpdateTool` — alias of `task_update::WorkItemMutator`.
pub type TaskUpdateTool = crate::task_update::WorkItemMutator;
pub fn task_update_tool() -> TaskUpdateTool { crate::task_update::WorkItemMutator }

/// `tools/ExitWorktreeTool/ExitWorktreeTool.ts` `ExitWorktreeTool` — alias of `exit_worktree::BranchRejoin`.
pub type ExitWorktreeTool = crate::exit_worktree::BranchRejoin;
pub fn exit_worktree_tool() -> ExitWorktreeTool { crate::exit_worktree::BranchRejoin }

/// `tools/WebFetchTool/WebFetchTool.ts` `WebFetchTool` — alias of `web_fetch::NetRetriever`.
pub type WebFetchTool = crate::web_fetch::NetRetriever;
pub fn web_fetch_tool() -> WebFetchTool { crate::web_fetch::NetRetriever }

/// `tools/TeamCreateTool/TeamCreateTool.ts` `TeamCreateTool` — alias of `team_create::SwarmSpawner`.
pub type TeamCreateTool = crate::team_create::SwarmSpawner;
pub fn team_create_tool() -> TeamCreateTool { crate::team_create::SwarmSpawner }

/// `tools/GlobTool/GlobTool.ts` `GlobTool` — alias of `glob::PathDiscoverer`.
pub type GlobTool = crate::glob::PathDiscoverer;
pub fn glob_tool() -> GlobTool { crate::glob::PathDiscoverer }

/// `tools/EnterPlanModeTool/EnterPlanModeTool.ts` `EnterPlanModeTool` — alias of `enter_plan_mode::PlanGate`.
pub type EnterPlanModeTool = crate::enter_plan_mode::PlanGate;
pub fn enter_plan_mode_tool() -> EnterPlanModeTool { crate::enter_plan_mode::PlanGate }

/// `tools/RemoteTriggerTool/RemoteTriggerTool.ts` `RemoteTriggerTool` — alias of `remote_trigger::EventRelay`.
pub type RemoteTriggerTool = crate::remote_trigger::EventRelay;
pub fn remote_trigger_tool() -> RemoteTriggerTool { crate::remote_trigger::EventRelay }

/// `tools/ReadMcpResourceTool/ReadMcpResourceTool.ts` `ReadMcpResourceTool` — alias of `mcp_read::BridgeReader`.
pub type ReadMcpResourceTool = crate::mcp_read::BridgeReader;
pub fn read_mcp_resource_tool() -> ReadMcpResourceTool { crate::mcp_read::BridgeReader }

/// `tools/ScheduleCronTool/CronCreateTool.ts` `CronCreateTool` — alias of `cron_create::ScheduleForge`.
pub type CronCreateTool = crate::cron_create::ScheduleForge;
pub fn cron_create_tool() -> CronCreateTool { crate::cron_create::ScheduleForge }

/// `tools/TeamDeleteTool/TeamDeleteTool.ts` `TeamDeleteTool` — alias of `team_delete::SwarmDismisser`.
pub type TeamDeleteTool = crate::team_delete::SwarmDismisser;
pub fn team_delete_tool() -> TeamDeleteTool { crate::team_delete::SwarmDismisser }

/// `tools/TaskCreateTool/TaskCreateTool.ts` `TaskCreateTool` — alias of `task_create::WorkItemForge`.
pub type TaskCreateTool = crate::task_create::WorkItemForge;
pub fn task_create_tool() -> TaskCreateTool { crate::task_create::WorkItemForge }

/// `tools/TaskStopTool/TaskStopTool.ts` `TaskStopTool` — alias of `task_stop::HaltSignal`.
pub type TaskStopTool = crate::task_stop::HaltSignal;
pub fn task_stop_tool() -> TaskStopTool { crate::task_stop::HaltSignal }

/// `tools/TaskGetTool/TaskGetTool.ts` `TaskGetTool` — alias of `task_get::WorkItemQuery`.
pub type TaskGetTool = crate::task_get::WorkItemQuery;
pub fn task_get_tool() -> TaskGetTool { crate::task_get::WorkItemQuery }

/// `tools/EnterWorktreeTool/EnterWorktreeTool.ts` `EnterWorktreeTool` — alias of `enter_worktree::BranchIsolator`.
pub type EnterWorktreeTool = crate::enter_worktree::BranchIsolator;
pub fn enter_worktree_tool() -> EnterWorktreeTool { crate::enter_worktree::BranchIsolator }

/// `tools/ListMcpResourcesTool/ListMcpResourcesTool.ts` `ListMcpResourcesTool` — alias of `mcp_list::BridgeInventory`.
pub type ListMcpResourcesTool = crate::mcp_list::BridgeInventory;
pub fn list_mcp_resources_tool() -> ListMcpResourcesTool { crate::mcp_list::BridgeInventory }

/// `tools/TaskListTool/TaskListTool.ts` `TaskListTool` — alias of `task_list::WorkItemIndex`.
pub type TaskListTool = crate::task_list::WorkItemIndex;
pub fn task_list_tool() -> TaskListTool { crate::task_list::WorkItemIndex }

/// `tools/TodoWriteTool/TodoWriteTool.ts` `TodoWriteTool` — alias of `todo::TaskNotePad`.
pub type TodoWriteTool = crate::todo::TaskNotePad;
pub fn todo_write_tool() -> TodoWriteTool { crate::todo::TaskNotePad }

/// `tools/SendUserFileTool/SendUserFileTool.ts` `SendUserFileTool` — alias of `send_user_file::FileDelivery`.
pub type SendUserFileTool = crate::send_user_file::FileDelivery;
pub fn send_user_file_tool() -> SendUserFileTool { crate::send_user_file::FileDelivery }

/// `tools/ScheduleCronTool/CronListTool.ts` `CronListTool` — alias of `cron_list::ScheduleIndex`.
pub type CronListTool = crate::cron_list::ScheduleIndex;
pub fn cron_list_tool() -> CronListTool { crate::cron_list::ScheduleIndex }

/// `tools/ScheduleCronTool/CronDeleteTool.ts` `CronDeleteTool` — alias of `cron_delete::ScheduleRevoke`.
pub type CronDeleteTool = crate::cron_delete::ScheduleRevoke;
pub fn cron_delete_tool() -> CronDeleteTool { crate::cron_delete::ScheduleRevoke }

/// `tools/SleepTool/SleepTool.ts` `SleepTool` — alias of `sleep::DeferralTimer`.
pub type SleepTool = crate::sleep::DeferralTimer;
pub fn sleep_tool() -> SleepTool { crate::sleep::DeferralTimer }

/// `tools/PushNotificationTool/PushNotificationTool.ts` `PushNotificationTool` — alias of `notification::AlertDispatcher`.
pub type PushNotificationTool = crate::notification::AlertDispatcher;
pub fn push_notification_tool() -> PushNotificationTool { crate::notification::AlertDispatcher }

/// `tools/MCPTool/MCPTool.ts` `MCPTool` — alias of `mcp_tool::BridgeExecutor`.
pub type MCPTool = crate::mcp_tool::BridgeExecutor;
pub fn mcp_tool() -> MCPTool { crate::mcp_tool::BridgeExecutor }

/// `tools/TungstenTool/TungstenTool.ts` `TungstenTool` — alias of `tungsten::InternalProbe`.
pub type TungstenTool = crate::tungsten::InternalProbe;
pub fn tungsten_tool() -> TungstenTool { crate::tungsten::InternalProbe }

/// `tools/testing/TestingPermissionTool.tsx` `TestingPermissionTool` — testing-only marker tool.
///
/// The real implementation lives in `testing::testing_permission_tool` as free
/// functions (because Tool registration is conditional on test mode). This
/// unit struct wraps that module to give the TS const export a Rust home.
#[derive(Debug, Clone, Copy, Default)]
pub struct TestingPermissionTool;

impl TestingPermissionTool {
    pub const NAME: &'static str = crate::testing::testing_permission_tool::TESTING_PERMISSION_TOOL_NAME;

    pub fn is_enabled(&self) -> bool {
        crate::testing::testing_permission_tool::is_enabled()
    }

    pub fn description(&self) -> &'static str {
        crate::testing::testing_permission_tool::description()
    }

    pub fn prompt(&self) -> &'static str {
        crate::testing::testing_permission_tool::prompt()
    }

    pub fn user_facing_name(&self) -> &'static str {
        crate::testing::testing_permission_tool::user_facing_name()
    }

    pub fn is_concurrency_safe(&self) -> bool {
        crate::testing::testing_permission_tool::is_concurrency_safe()
    }

    pub fn is_read_only(&self) -> bool {
        crate::testing::testing_permission_tool::is_read_only()
    }

    pub fn input_schema(&self) -> serde_json::Value {
        crate::testing::testing_permission_tool::input_schema()
    }
}

pub fn testing_permission_tool() -> TestingPermissionTool { TestingPermissionTool }
