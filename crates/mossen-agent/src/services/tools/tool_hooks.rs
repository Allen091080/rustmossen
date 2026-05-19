//! Tool hooks — pre/post execution hooks for tools (permission tracking, analytics, caching).

use std::collections::HashMap;
use std::time::Instant;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use super::tool_execution::{ToolExecutionRequest, ToolExecutionResult};

/// Hook that runs before tool execution.
#[async_trait::async_trait]
pub trait PreToolHook: Send + Sync {
    async fn before_execute(&self, request: &ToolExecutionRequest) -> HookResult;
}

/// Hook that runs after tool execution.
#[async_trait::async_trait]
pub trait PostToolHook: Send + Sync {
    async fn after_execute(&self, request: &ToolExecutionRequest, result: &ToolExecutionResult);
}

/// Result from a pre-execution hook.
#[derive(Debug, Clone)]
pub enum HookResult {
    /// Continue with execution.
    Continue,
    /// Skip execution and return this result.
    Skip(String),
    /// Modify the request before execution.
    ModifyInput(Value),
}

/// Registry of tool hooks.
pub struct ToolHookRegistry {
    pre_hooks: Vec<Box<dyn PreToolHook>>,
    post_hooks: Vec<Box<dyn PostToolHook>>,
}

impl ToolHookRegistry {
    pub fn new() -> Self {
        Self {
            pre_hooks: Vec::new(),
            post_hooks: Vec::new(),
        }
    }

    pub fn register_pre_hook(&mut self, hook: Box<dyn PreToolHook>) {
        self.pre_hooks.push(hook);
    }

    pub fn register_post_hook(&mut self, hook: Box<dyn PostToolHook>) {
        self.post_hooks.push(hook);
    }

    /// Run all pre-execution hooks. Returns Skip if any hook requests it.
    pub async fn run_pre_hooks(&self, request: &ToolExecutionRequest) -> HookResult {
        for hook in &self.pre_hooks {
            match hook.before_execute(request).await {
                HookResult::Continue => continue,
                result => return result,
            }
        }
        HookResult::Continue
    }

    /// Run all post-execution hooks.
    pub async fn run_post_hooks(&self, request: &ToolExecutionRequest, result: &ToolExecutionResult) {
        for hook in &self.post_hooks {
            hook.after_execute(request, result).await;
        }
    }
}

impl Default for ToolHookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in hook: tracks file read state for compaction.
pub struct FileReadTrackingHook {
    pub read_state: std::sync::Arc<std::sync::Mutex<HashMap<String, FileReadState>>>,
}

#[derive(Debug, Clone)]
pub struct FileReadState {
    pub content: String,
    pub timestamp: u64,
}

#[async_trait::async_trait]
impl PostToolHook for FileReadTrackingHook {
    async fn after_execute(&self, request: &ToolExecutionRequest, result: &ToolExecutionResult) {
        if request.tool_name == "Read" && !result.is_error {
            if let Some(file_path) = request.input.get("file_path").and_then(|v| v.as_str()) {
                let mut state = self.read_state.lock().unwrap();
                state.insert(file_path.to_string(), FileReadState {
                    content: result.content.clone(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                });
            }
        }
    }
}

/// Built-in hook: logs tool execution analytics.
pub struct ToolAnalyticsHook;

#[async_trait::async_trait]
impl PostToolHook for ToolAnalyticsHook {
    async fn after_execute(&self, request: &ToolExecutionRequest, result: &ToolExecutionResult) {
        debug!(
            tool_name = %request.tool_name,
            duration_ms = result.duration_ms,
            is_error = result.is_error,
            "Tool executed"
        );
    }
}

// === Hook permission decision + post-use result types (TS `toolHooks.ts`) ===

/// Outcome variant of post-tool-use hooks. Either a message-update lazy
/// reference (rendered by the UI later) OR an in-place tool-output update.
#[derive(Debug, Clone)]
pub enum PostToolUseHooksResult {
    /// A pending message attachment / progress update referenced by id.
    MessageUpdate(serde_json::Value),
    /// The hook rewrote the tool output (MCP servers, transformers).
    UpdatedMcpToolOutput(serde_json::Value),
}

/// Decision returned by `resolve_hook_permission_decision`. Mirrors the TS
/// shape `{ decision: PermissionDecision; input: Record<string, unknown> }`.
#[derive(Debug, Clone)]
pub struct HookPermissionDecision {
    pub decision: HookPermissionBehavior,
    pub input: serde_json::Value,
}

/// Behavior of a permission decision — `allow` | `deny` | `ask`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPermissionBehavior {
    Allow,
    Deny,
    Ask,
}

/// Input to `resolve_hook_permission_decision`. Concrete bundle that the TS
/// signature passes as positional args.
pub struct HookPermissionInputs<'a> {
    pub hook_permission_result: Option<&'a HookPermissionDecision>,
    pub tool_name: &'a str,
    pub input: serde_json::Value,
    pub requires_user_interaction: bool,
    pub require_can_use_tool: bool,
}

/// Resolve a permission decision returned by a PreToolUse hook into an
/// effective `HookPermissionDecision` taking rule-based deny/ask overrides
/// into account.
///
/// This mirrors TS `resolveHookPermissionDecision` but trimmed to the core
/// decision-merging logic (rule checks and canUseTool prompts are wired by
/// the agent runtime; this fn is the pure decision merger).
pub fn resolve_hook_permission_decision(
    inputs: HookPermissionInputs<'_>,
    rule_check: Option<HookPermissionBehavior>,
) -> HookPermissionDecision {
    if let Some(prev) = inputs.hook_permission_result {
        match prev.decision {
            HookPermissionBehavior::Allow => {
                // Hook approved. Apply rule check overrides.
                let hook_input = if prev.input.is_null() {
                    inputs.input.clone()
                } else {
                    prev.input.clone()
                };
                let interaction_satisfied =
                    inputs.requires_user_interaction && !prev.input.is_null();
                if (inputs.requires_user_interaction && !interaction_satisfied)
                    || inputs.require_can_use_tool
                {
                    return HookPermissionDecision {
                        decision: HookPermissionBehavior::Ask,
                        input: hook_input,
                    };
                }
                match rule_check {
                    None => HookPermissionDecision {
                        decision: HookPermissionBehavior::Allow,
                        input: hook_input,
                    },
                    Some(HookPermissionBehavior::Deny) => HookPermissionDecision {
                        decision: HookPermissionBehavior::Deny,
                        input: hook_input,
                    },
                    Some(_) => HookPermissionDecision {
                        decision: HookPermissionBehavior::Ask,
                        input: hook_input,
                    },
                }
            }
            HookPermissionBehavior::Deny => HookPermissionDecision {
                decision: HookPermissionBehavior::Deny,
                input: inputs.input,
            },
            HookPermissionBehavior::Ask => HookPermissionDecision {
                decision: HookPermissionBehavior::Ask,
                input: if prev.input.is_null() {
                    inputs.input
                } else {
                    prev.input.clone()
                },
            },
        }
    } else {
        // No hook decision — default ask flow.
        HookPermissionDecision {
            decision: HookPermissionBehavior::Ask,
            input: inputs.input,
        }
    }
}
