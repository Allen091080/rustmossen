//! Agent tool constants.

/// The canonical agent tool name.
pub const AGENT_TOOL_NAME: &str = "Agent";
/// Legacy wire name for backward compat (permission rules, hooks, resumed sessions).
pub const LEGACY_AGENT_TOOL_NAME: &str = "Task";
/// Verification agent type identifier.
pub const VERIFICATION_AGENT_TYPE: &str = "verification";

/// Built-in agents that run once and return a report — skip agentId/SendMessage trailer.
pub const ONE_SHOT_BUILTIN_AGENT_TYPES: &[&str] = &["Explore", "Plan"];

/// Check if an agent type is a one-shot built-in (no continuation).
pub fn is_one_shot_builtin(agent_type: &str) -> bool {
    ONE_SHOT_BUILTIN_AGENT_TYPES.contains(&agent_type)
}
