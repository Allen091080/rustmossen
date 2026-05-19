//! # Tools (tools.ts)
//!
//! 工具名称常量与工具集合定义。

use std::collections::HashSet;

use once_cell::sync::Lazy;

// Tool name constants (originally imported from individual tool modules)
pub const TASK_OUTPUT_TOOL_NAME: &str = "TaskOutput";
pub const EXIT_PLAN_MODE_V2_TOOL_NAME: &str = "ExitPlanMode";
pub const ENTER_PLAN_MODE_TOOL_NAME: &str = "EnterPlanMode";
pub const AGENT_TOOL_NAME: &str = "Agent";
pub const ASK_USER_QUESTION_TOOL_NAME: &str = "AskUserQuestion";
pub const TASK_STOP_TOOL_NAME: &str = "TaskStop";
pub const FILE_READ_TOOL_NAME: &str = "Read";
pub const WEB_SEARCH_TOOL_NAME: &str = "WebSearch";
pub const TODO_WRITE_TOOL_NAME: &str = "TodoWrite";
pub const GREP_TOOL_NAME: &str = "Grep";
pub const WEB_FETCH_TOOL_NAME: &str = "WebFetch";
pub const GLOB_TOOL_NAME: &str = "Glob";
pub const FILE_EDIT_TOOL_NAME: &str = "Edit";
pub const FILE_WRITE_TOOL_NAME: &str = "Write";
pub const NOTEBOOK_EDIT_TOOL_NAME: &str = "NotebookEdit";
pub const SKILL_TOOL_NAME: &str = "Skill";
pub const SEND_MESSAGE_TOOL_NAME: &str = "SendMessage";
pub const TASK_CREATE_TOOL_NAME: &str = "TaskCreate";
pub const TASK_GET_TOOL_NAME: &str = "TaskGet";
pub const TASK_LIST_TOOL_NAME: &str = "TaskList";
pub const TASK_UPDATE_TOOL_NAME: &str = "TaskUpdate";
pub const TOOL_SEARCH_TOOL_NAME: &str = "ToolSearch";
pub const SYNTHETIC_OUTPUT_TOOL_NAME: &str = "SyntheticOutput";
pub const ENTER_WORKTREE_TOOL_NAME: &str = "EnterWorktree";
pub const EXIT_WORKTREE_TOOL_NAME: &str = "ExitWorktree";
pub const WORKFLOW_TOOL_NAME: &str = "Workflow";
pub const CRON_CREATE_TOOL_NAME: &str = "CronCreate";
pub const CRON_DELETE_TOOL_NAME: &str = "CronDelete";
pub const CRON_LIST_TOOL_NAME: &str = "CronList";
pub const BASH_TOOL_NAME: &str = "Bash";
pub const SLEEP_TOOL_NAME: &str = "Sleep";
pub const VERIFICATION_AGENT_TYPE: &str = "verification";

// Shell tool names
pub const SHELL_TOOL_NAMES: &[&str] = &[BASH_TOOL_NAME];

/// Tools disallowed for ALL agents.
/// In TS, AGENT_TOOL_NAME is conditionally included based on USER_TYPE === 'ant'.
pub fn all_agent_disallowed_tools(is_ant: bool, workflow_scripts_enabled: bool) -> HashSet<&'static str> {
    let mut s = HashSet::new();
    s.insert(TASK_OUTPUT_TOOL_NAME);
    s.insert(EXIT_PLAN_MODE_V2_TOOL_NAME);
    s.insert(ENTER_PLAN_MODE_TOOL_NAME);
    // Allow Agent tool for agents when user is ant (enables nested agents)
    if !is_ant {
        s.insert(AGENT_TOOL_NAME);
    }
    s.insert(ASK_USER_QUESTION_TOOL_NAME);
    s.insert(TASK_STOP_TOOL_NAME);
    // Prevent recursive workflow execution inside subagents.
    if workflow_scripts_enabled {
        s.insert(WORKFLOW_TOOL_NAME);
    }
    s
}

/// Tools disallowed for custom agents (same as all agent disallowed).
pub fn custom_agent_disallowed_tools(is_ant: bool, workflow_scripts_enabled: bool) -> HashSet<&'static str> {
    all_agent_disallowed_tools(is_ant, workflow_scripts_enabled)
}

/// Async Agent Allowed Tools (source of truth).
pub static ASYNC_AGENT_ALLOWED_TOOLS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(FILE_READ_TOOL_NAME);
    s.insert(WEB_SEARCH_TOOL_NAME);
    s.insert(TODO_WRITE_TOOL_NAME);
    s.insert(GREP_TOOL_NAME);
    s.insert(WEB_FETCH_TOOL_NAME);
    s.insert(GLOB_TOOL_NAME);
    for name in SHELL_TOOL_NAMES {
        s.insert(name);
    }
    s.insert(FILE_EDIT_TOOL_NAME);
    s.insert(FILE_WRITE_TOOL_NAME);
    s.insert(NOTEBOOK_EDIT_TOOL_NAME);
    s.insert(SKILL_TOOL_NAME);
    s.insert(SYNTHETIC_OUTPUT_TOOL_NAME);
    s.insert(TOOL_SEARCH_TOOL_NAME);
    s.insert(ENTER_WORKTREE_TOOL_NAME);
    s.insert(EXIT_WORKTREE_TOOL_NAME);
    s
});

/// Tools allowed only for in-process teammates (not general async agents).
/// These are injected by inProcessRunner.ts and allowed through filterToolsForAgent
/// via isInProcessTeammate() check.
pub fn in_process_teammate_allowed_tools(agent_triggers_enabled: bool) -> HashSet<&'static str> {
    let mut s = HashSet::new();
    s.insert(TASK_CREATE_TOOL_NAME);
    s.insert(TASK_GET_TOOL_NAME);
    s.insert(TASK_LIST_TOOL_NAME);
    s.insert(TASK_UPDATE_TOOL_NAME);
    s.insert(SEND_MESSAGE_TOOL_NAME);
    // Teammate-created crons are tagged with the creating agentId and routed to
    // that teammate's pendingUserMessages queue (see useScheduledTasks.ts).
    if agent_triggers_enabled {
        s.insert(CRON_CREATE_TOOL_NAME);
        s.insert(CRON_DELETE_TOOL_NAME);
        s.insert(CRON_LIST_TOOL_NAME);
    }
    s
}

/// Tools allowed in coordinator mode - only output and agent management tools.
pub static COORDINATOR_MODE_ALLOWED_TOOLS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(AGENT_TOOL_NAME);
    s.insert(TASK_STOP_TOOL_NAME);
    s.insert(SEND_MESSAGE_TOOL_NAME);
    s.insert(SYNTHETIC_OUTPUT_TOOL_NAME);
    s
});
