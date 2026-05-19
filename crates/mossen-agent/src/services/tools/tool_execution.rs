//! Tool execution — core tool invocation logic, permission checking, and result handling.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, error, warn};

/// Tool execution request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    pub tool_name: String,
    pub tool_use_id: String,
    pub input: Value,
    pub is_from_streaming: bool,
}

/// Tool execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
    pub duration_ms: u64,
    pub metadata: HashMap<String, Value>,
}

/// Permission decision for a tool use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermissionDecision {
    pub behavior: PermissionBehavior,
    pub message: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

/// Context for tool execution providing access to environment and permissions.
#[derive(Debug, Clone)]
pub struct ToolExecutionContext {
    pub working_directory: String,
    pub model: String,
    pub query_source: Option<String>,
    pub agent_id: Option<String>,
    pub is_non_interactive: bool,
    pub allowed_tools: Vec<String>,
}

/// Execute a tool with full permission checking and result handling.
pub async fn execute_tool(
    request: &ToolExecutionRequest,
    context: &ToolExecutionContext,
    permission_checker: &dyn ToolPermissionChecker,
) -> Result<ToolExecutionResult> {
    let start = Instant::now();

    // Check permissions
    let decision = permission_checker
        .check_permission(&request.tool_name, &request.input, context)
        .await?;

    if decision.behavior == PermissionBehavior::Deny {
        return Ok(ToolExecutionResult {
            tool_use_id: request.tool_use_id.clone(),
            content: decision.message.unwrap_or_else(|| "Tool use denied".to_string()),
            is_error: true,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        });
    }

    if decision.behavior == PermissionBehavior::Ask {
        // In non-interactive mode, deny
        if context.is_non_interactive {
            return Ok(ToolExecutionResult {
                tool_use_id: request.tool_use_id.clone(),
                content: "Tool use requires approval but running in non-interactive mode".to_string(),
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }
    }

    // Execute the tool (dispatch to appropriate handler)
    let result = dispatch_tool_execution(request, context).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(content) => Ok(ToolExecutionResult {
            tool_use_id: request.tool_use_id.clone(),
            content,
            is_error: false,
            duration_ms,
            metadata: HashMap::new(),
        }),
        Err(e) => Ok(ToolExecutionResult {
            tool_use_id: request.tool_use_id.clone(),
            content: format!("Error: {}", e),
            is_error: true,
            duration_ms,
            metadata: HashMap::new(),
        }),
    }
}

/// Trait for checking tool permissions.
#[async_trait::async_trait]
pub trait ToolPermissionChecker: Send + Sync {
    async fn check_permission(
        &self,
        tool_name: &str,
        input: &Value,
        context: &ToolExecutionContext,
    ) -> Result<ToolPermissionDecision>;
}

/// Dispatch tool execution to the appropriate handler.
async fn dispatch_tool_execution(
    request: &ToolExecutionRequest,
    context: &ToolExecutionContext,
) -> Result<String> {
    match request.tool_name.as_str() {
        "Read" => execute_read_tool(&request.input, context).await,
        "Write" => execute_write_tool(&request.input, context).await,
        "Edit" => execute_edit_tool(&request.input, context).await,
        "Bash" | "Execute" => execute_bash_tool(&request.input, context).await,
        "Glob" => execute_glob_tool(&request.input, context).await,
        "Grep" => execute_grep_tool(&request.input, context).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {}", request.tool_name)),
    }
}

async fn execute_read_tool(input: &Value, context: &ToolExecutionContext) -> Result<String> {
    let file_path = input.get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let content = tokio::fs::read_to_string(file_path).await?;
    Ok(content)
}

async fn execute_write_tool(input: &Value, context: &ToolExecutionContext) -> Result<String> {
    let file_path = input.get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let content = input.get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing content"))?;
    tokio::fs::write(file_path, content).await?;
    Ok(format!("Successfully wrote to {}", file_path))
}

async fn execute_edit_tool(input: &Value, _context: &ToolExecutionContext) -> Result<String> {
    let file_path = input.get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let old_string = input.get("old_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing old_string"))?;
    let new_string = input.get("new_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing new_string"))?;

    let content = tokio::fs::read_to_string(file_path).await?;
    if !content.contains(old_string) {
        return Err(anyhow::anyhow!("old_string not found in file"));
    }
    let new_content = content.replacen(old_string, new_string, 1);
    tokio::fs::write(file_path, &new_content).await?;
    Ok(format!("Successfully edited {}", file_path))
}

async fn execute_bash_tool(input: &Value, context: &ToolExecutionContext) -> Result<String> {
    let command = input.get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing command"))?;

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&context.working_directory)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        Ok(stdout.to_string())
    } else {
        Ok(format!("Exit code: {}\nStdout: {}\nStderr: {}", output.status, stdout, stderr))
    }
}

// NOTE: `execute_glob_tool` / `execute_grep_tool` used to live here as
// stub bodies returning a literal description string. They've been removed
// because they were dead code — production routing for `Glob` and `Grep`
// goes through the `mossen-tools` crate (`crate::glob::PathDiscoverer` and
// `crate::grep::ContentScanner`, both registered with the canonical names
// "Glob" / "Grep") and is invoked via `tool_registry.execute` in the
// dialogue loop. Keeping the stubs here would silently fork the dispatch
// graph if `dispatch_tool_execution` ever got called by accident.

async fn execute_glob_tool(_input: &Value, _context: &ToolExecutionContext) -> Result<String> {
    Err(anyhow::anyhow!(
        "Glob is dispatched via mossen-tools::glob::PathDiscoverer through tool_registry; \
the services/tools dispatch path is not the active execution route."
    ))
}

async fn execute_grep_tool(_input: &Value, _context: &ToolExecutionContext) -> Result<String> {
    Err(anyhow::anyhow!(
        "Grep is dispatched via mossen-tools::grep::ContentScanner through tool_registry; \
the services/tools dispatch path is not the active execution route."
    ))
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/tools/toolExecution.ts` exports.
// ---------------------------------------------------------------------------

/// `toolExecution.ts` `HOOK_TIMING_DISPLAY_THRESHOLD_MS`.
pub const HOOK_TIMING_DISPLAY_THRESHOLD_MS: u64 = 500;

/// `toolExecution.ts` `MessageUpdateLazy`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageUpdateLazy {
    pub message_id: String,
    pub updater: String,
}

/// `toolExecution.ts` `classifyToolError`.
pub fn classify_tool_error(message: &str) -> &'static str {
    let lower = message.to_lowercase();
    if lower.contains("timeout") {
        "timeout"
    } else if lower.contains("permission") || lower.contains("eacces") {
        "permission_denied"
    } else if lower.contains("not found") || lower.contains("enoent") {
        "not_found"
    } else if lower.contains("aborted") || lower.contains("cancelled") {
        "aborted"
    } else if lower.contains("network") || lower.contains("econnreset") {
        "network"
    } else {
        "unknown"
    }
}

/// `toolExecution.ts` `buildSchemaNotSentHint`.
pub fn build_schema_not_sent_hint(tool_name: &str) -> String {
    format!(
        "Tool {} was called before its schema was loaded. Use ToolSearch to fetch the schema first.",
        tool_name
    )
}

/// `toolExecution.ts` `McpServerType` — discriminator for which MCP transport
/// kind a tool's server uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpServerType {
    Stdio,
    Sse,
    Http,
    Ws,
    Sdk,
    SseIde,
    WsIde,
    HostedProxy,
}
