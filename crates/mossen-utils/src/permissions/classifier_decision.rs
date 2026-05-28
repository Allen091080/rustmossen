//! Classifier decision logic for auto mode.
//!
//! Determines which tools are safe to auto-allow without running the YOLO classifier.

use once_cell::sync::Lazy;
use std::collections::HashSet;

/// Tool name constants for permission checking.
pub const FILE_READ_TOOL_NAME: &str = "Read";
pub const GREP_TOOL_NAME: &str = "Grep";
pub const GLOB_TOOL_NAME: &str = "Glob";
pub const LSP_TOOL_NAME: &str = "LSP";
pub const TOOL_SEARCH_TOOL_NAME: &str = "ToolSearch";
pub const LIST_MCP_RESOURCES_TOOL_NAME: &str = "ListMcpResources";
pub const READ_MCP_RESOURCE_TOOL_NAME: &str = "ReadMcpResourceTool";
pub const TODO_WRITE_TOOL_NAME: &str = "TodoWrite";
pub const TASK_CREATE_TOOL_NAME: &str = "TaskCreate";
pub const TASK_GET_TOOL_NAME: &str = "TaskGet";
pub const TASK_UPDATE_TOOL_NAME: &str = "TaskUpdate";
pub const TASK_LIST_TOOL_NAME: &str = "TaskList";
pub const TASK_STOP_TOOL_NAME: &str = "TaskStop";
pub const TASK_OUTPUT_TOOL_NAME: &str = "TaskOutput";
pub const ASK_USER_QUESTION_TOOL_NAME: &str = "AskUserQuestion";
pub const ENTER_PLAN_MODE_TOOL_NAME: &str = "EnterPlanMode";
pub const EXIT_PLAN_MODE_TOOL_NAME: &str = "ExitPlanMode";
pub const TEAM_CREATE_TOOL_NAME: &str = "TeamCreate";
pub const TEAM_DELETE_TOOL_NAME: &str = "TeamDelete";
pub const SEND_MESSAGE_TOOL_NAME: &str = "SendMessage";
pub const SLEEP_TOOL_NAME: &str = "Sleep";
pub const YOLO_CLASSIFIER_TOOL_NAME: &str = "YoloClassifier";

/// Tools that are safe and don't need any classifier checking.
/// Used by the auto mode classifier to skip unnecessary API calls.
/// Does NOT include write/edit tools — those are handled by the
/// acceptEdits fast path (allowed in CWD, classified outside CWD).
static SAFE_YOLO_ALLOWLISTED_TOOLS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    // Read-only file operations
    set.insert(FILE_READ_TOOL_NAME);
    // Search / read-only
    set.insert(GREP_TOOL_NAME);
    set.insert(GLOB_TOOL_NAME);
    set.insert(LSP_TOOL_NAME);
    set.insert(TOOL_SEARCH_TOOL_NAME);
    set.insert(LIST_MCP_RESOURCES_TOOL_NAME);
    set.insert(READ_MCP_RESOURCE_TOOL_NAME);
    // Task management (metadata only)
    set.insert(TODO_WRITE_TOOL_NAME);
    set.insert(TASK_CREATE_TOOL_NAME);
    set.insert(TASK_GET_TOOL_NAME);
    set.insert(TASK_UPDATE_TOOL_NAME);
    set.insert(TASK_LIST_TOOL_NAME);
    set.insert(TASK_STOP_TOOL_NAME);
    set.insert(TASK_OUTPUT_TOOL_NAME);
    // Plan mode / UI
    set.insert(ASK_USER_QUESTION_TOOL_NAME);
    set.insert(ENTER_PLAN_MODE_TOOL_NAME);
    set.insert(EXIT_PLAN_MODE_TOOL_NAME);
    // Swarm coordination
    set.insert(TEAM_CREATE_TOOL_NAME);
    set.insert(TEAM_DELETE_TOOL_NAME);
    set.insert(SEND_MESSAGE_TOOL_NAME);
    // Misc safe
    set.insert(SLEEP_TOOL_NAME);
    // Internal classifier tool
    set.insert(YOLO_CLASSIFIER_TOOL_NAME);
    set
});

/// Check if a tool is on the safe allowlist for auto mode.
/// These tools are auto-approved without running the YOLO classifier.
pub fn is_auto_mode_allowlisted_tool(tool_name: &str) -> bool {
    SAFE_YOLO_ALLOWLISTED_TOOLS.contains(tool_name)
}
