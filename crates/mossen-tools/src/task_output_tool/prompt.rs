//! TaskOutputTool prompt and tool description text.

/// The tool prompt/description returned to the model.
pub fn build_prompt() -> &'static str {
    "DEPRECATED: Prefer using the Read tool on the task's output file path instead. \
Background tasks return their output file path in the tool result, and you receive \
a <task-notification> with the same path when the task completes — Read that file directly.\n\n\
- Retrieves output from a running or completed task (background shell, agent, or remote session)\n\
- Takes a task_id parameter identifying the task\n\
- Returns the task output along with status information\n\
- Use block=true (default) to wait for task completion\n\
- Use block=false for non-blocking check of current status\n\
- Task IDs can be found using the /tasks command\n\
- Works with all task types: background shells, async agents, and remote sessions"
}

/// Search hint for tool discovery.
pub const SEARCH_HINT: &str = "read output/logs from a background task";

/// Tool description text.
pub const DESCRIPTION: &str = "[Deprecated] — prefer Read on the task output file path";

/// Whether this tool should be deferred.
pub const SHOULD_DEFER: bool = true;

/// Tool aliases for backwards compatibility.
pub const ALIASES: &[&str] = &["AgentOutputTool", "BashOutputTool"];
