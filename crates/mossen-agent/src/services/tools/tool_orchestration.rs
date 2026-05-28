//! Tool orchestration — coordinates multi-tool execution flows (parallel, sequential, retry).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::debug;

use super::tool_execution::{
    execute_tool, ToolExecutionContext, ToolExecutionRequest, ToolExecutionResult,
    ToolPermissionChecker,
};

/// Orchestration mode for tool execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestrationMode {
    /// Execute tools sequentially in order.
    Sequential,
    /// Execute independent tools in parallel.
    Parallel,
}

/// Execute a batch of tools with the given orchestration mode.
pub async fn execute_tool_batch(
    requests: Vec<ToolExecutionRequest>,
    context: &ToolExecutionContext,
    permission_checker: &dyn ToolPermissionChecker,
    mode: OrchestrationMode,
) -> Vec<ToolExecutionResult> {
    match mode {
        OrchestrationMode::Sequential => {
            let mut results = Vec::with_capacity(requests.len());
            for request in requests {
                let result = execute_tool(&request, context, permission_checker).await;
                results.push(result.unwrap_or_else(|e| ToolExecutionResult {
                    tool_use_id: request.tool_use_id.clone(),
                    content: format!("Error: {}", e),
                    is_error: true,
                    duration_ms: 0,
                    metadata: HashMap::new(),
                }));
            }
            results
        }
        OrchestrationMode::Parallel => {
            let futures: Vec<_> = requests
                .into_iter()
                .map(|request| {
                    let ctx = context.clone();
                    async move {
                        let result = execute_tool(&request, &ctx, permission_checker).await;
                        result.unwrap_or_else(|e| ToolExecutionResult {
                            tool_use_id: request.tool_use_id.clone(),
                            content: format!("Error: {}", e),
                            is_error: true,
                            duration_ms: 0,
                            metadata: HashMap::new(),
                        })
                    }
                })
                .collect();
            futures::future::join_all(futures).await
        }
    }
}

/// Determine if two tool executions can safely run in parallel.
pub fn can_parallelize(a: &ToolExecutionRequest, b: &ToolExecutionRequest) -> bool {
    // Read-only tools can always run in parallel
    let read_only =
        |name: &str| matches!(name, "Read" | "Glob" | "Grep" | "WebSearch" | "WebFetch");

    if read_only(&a.tool_name) && read_only(&b.tool_name) {
        return true;
    }

    // Tools that modify different files can run in parallel
    if let (Some(path_a), Some(path_b)) = (
        a.input.get("file_path").and_then(|v| v.as_str()),
        b.input.get("file_path").and_then(|v| v.as_str()),
    ) {
        if path_a != path_b && read_only(&a.tool_name) {
            return true;
        }
    }

    false
}

/// TS `type MessageUpdate` — discriminated union of in-flight message
/// updates emitted by the orchestrator (the lazy form is
/// `MessageUpdateLazy` in `tool_execution`).
#[derive(Debug, Clone)]
pub enum MessageUpdate {
    /// Append a fully-formed message.
    Append(serde_json::Value),
    /// Replace an existing message by id with new content.
    Replace {
        id: String,
        value: serde_json::Value,
    },
    /// Remove an existing message by id (e.g. after squashing).
    Remove(String),
}
