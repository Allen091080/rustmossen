//! TaskOutputTool prompt and tool description text.

/// The tool prompt/description returned to the model.
pub fn build_prompt() -> &'static str {
    "- Retrieves output from a running or completed local task (background shell or agent)\n\
- Takes a task_id parameter identifying the task\n\
- Returns the task output along with status information\n\
- Use block=true (default) to wait for task completion; the default wait is 120 seconds\n\
- Use block=false for non-blocking check of current status\n\
- If retrieval_status is not_ready, the task is still running; call TaskOutput again with the same task_id\n\
- Task IDs can be found using the /tasks command\n\
- Works with local background shells and async agents"
}

/// Search hint for tool discovery.
pub const SEARCH_HINT: &str = "read output/logs from a background task";

/// Tool description text.
pub const DESCRIPTION: &str = "Read output from a running or completed background task";

/// Whether this tool should be deferred.
pub const SHOULD_DEFER: bool = true;

/// Tool aliases for backwards compatibility.
pub const ALIASES: &[&str] = &["AgentOutputTool", "BashOutputTool"];
