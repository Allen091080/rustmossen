//! Testing-only tool that always pops up a permission dialog when called.
//! Disabled in production (isEnabled returns false unless in test mode).

use serde_json::{json, Value};

/// Tool name constant.
pub const TESTING_PERMISSION_TOOL_NAME: &str = "TestingPermission";

/// Maximum result size in characters.
pub const MAX_RESULT_SIZE_CHARS: usize = 100_000;

/// Permission check result.
#[derive(Debug, Clone)]
pub struct PermissionCheckResult {
    pub behavior: &'static str,
    pub message: String,
}

/// Check if the testing permission tool is enabled. Only in test mode.
pub fn is_enabled() -> bool {
    std::env::var("NODE_ENV")
        .map(|v| v == "test")
        .unwrap_or(false)
        || std::env::var("MOSSEN_ENV")
            .map(|v| v == "test")
            .unwrap_or(false)
}

/// The tool description.
pub fn description() -> &'static str {
    "Test tool that always asks for permission"
}

/// The tool prompt.
pub fn prompt() -> &'static str {
    "Test tool that always asks for permission before executing. Used for end-to-end testing."
}

/// User-facing name for the tool.
pub fn user_facing_name() -> &'static str {
    "TestingPermission"
}

/// Whether this tool is concurrency-safe.
pub fn is_concurrency_safe() -> bool {
    true
}

/// Whether this tool is read-only.
pub fn is_read_only() -> bool {
    true
}

/// Check permissions — this tool ALWAYS requires permission.
pub fn check_permissions() -> PermissionCheckResult {
    PermissionCheckResult {
        behavior: "ask",
        message: "Run test?".to_string(),
    }
}

/// The input schema (empty strict object).
pub fn input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

/// Execute the tool — simply returns success message.
pub fn call() -> String {
    format!("{} executed successfully", TESTING_PERMISSION_TOOL_NAME)
}

/// Map tool result to a tool_result block param.
pub fn map_tool_result_to_block_param(result: &str, tool_use_id: &str) -> Value {
    json!({
        "type": "tool_result",
        "content": result,
        "tool_use_id": tool_use_id
    })
}
