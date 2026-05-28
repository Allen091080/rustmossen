//! `cli/print.ts` — headless 模式与权限提示支持。
//!
//! 包含 `runHeadless`、`joinPromptValues`、`canBatchWith` 以及
//! `createCanUseToolWithPermissionPrompt` 的 Rust 翻译。
//! 真实运行时由 mossen-agent 提供；此处建模 CLI 控制流和数据结构。

#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

// ----------------------------------------------------------------------------
// PromptValue
// ----------------------------------------------------------------------------

/// Prompt 内容块。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PromptBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: Value },
    #[serde(rename = "document")]
    Document { source: Value },
}

/// Prompt 的载荷：单字符串或块数组。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PromptValue {
    Text(String),
    Blocks(Vec<PromptBlock>),
}

impl PromptValue {
    fn to_blocks(self) -> Vec<PromptBlock> {
        match self {
            PromptValue::Text(s) => vec![PromptBlock::Text { text: s }],
            PromptValue::Blocks(b) => b,
        }
    }

    fn is_text(&self) -> bool {
        matches!(self, PromptValue::Text(_))
    }
}

/// 合并多个 PromptValue。
pub fn join_prompt_values(values: Vec<PromptValue>) -> PromptValue {
    if values.len() == 1 {
        return values.into_iter().next().unwrap();
    }
    if values.iter().all(PromptValue::is_text) {
        let joined = values
            .into_iter()
            .map(|v| match v {
                PromptValue::Text(s) => s,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        return PromptValue::Text(joined);
    }
    let mut all = Vec::new();
    for v in values {
        all.extend(v.to_blocks());
    }
    PromptValue::Blocks(all)
}

pub fn joinPromptValues(values: Vec<PromptValue>) -> PromptValue {
    join_prompt_values(values)
}

// ----------------------------------------------------------------------------
// Queued command batching
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandMode {
    Prompt,
    Bash,
    Slash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedCommand {
    pub mode: CommandMode,
    pub workload: String,
    pub is_meta: bool,
    pub prompt: PromptValue,
}

/// 是否可与 head 命令合批。
pub fn can_batch_with(head: &QueuedCommand, next: Option<&QueuedCommand>) -> bool {
    match next {
        None => false,
        Some(n) => {
            n.mode == CommandMode::Prompt
                && n.workload == head.workload
                && n.is_meta == head.is_meta
        }
    }
}

pub fn canBatchWith(head: &QueuedCommand, next: Option<&QueuedCommand>) -> bool {
    can_batch_with(head, next)
}

// ----------------------------------------------------------------------------
// Headless run
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct RunHeadlessOptions {
    pub r#continue: Option<bool>,
    pub resume: Option<String>,
    pub resume_session_at: Option<String>,
    pub verbose: Option<bool>,
    pub output_format: Option<String>,
    pub json_schema: Option<Value>,
    pub permission_prompt_tool_name: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub thinking_config: Option<Value>,
    pub max_turns: Option<u64>,
    pub max_budget_usd: Option<f64>,
    pub task_budget_total: Option<f64>,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub user_specified_model: Option<String>,
    pub fallback_model: Option<String>,
    pub teleport: Option<String>,
    pub sdk_url: Option<String>,
    pub replay_user_messages: Option<bool>,
    pub include_partial_messages: Option<bool>,
    pub fork_session: Option<bool>,
    pub rewind_files: Option<String>,
    pub enable_auth_status: Option<bool>,
    pub agent: Option<String>,
    pub workload: Option<String>,
    pub setup_trigger: Option<String>,
}

/// 运行 headless 模式（无 React/TUI）。
///
/// 真实实现：构造 query loop、注册 hooks/MCP、输出流式结果到 stdout。
/// 此处提供完整生命周期框架；具体 query 由 mossen-agent::query 驱动。
pub async fn run_headless(input_prompt: PromptValue, opts: RunHeadlessOptions) -> Result<()> {
    info!("headless mode start");
    // 1. 检查 startup-exit env (兼容 TS test harness)
    if std::env::var("MOSSEN_CODE_EXIT_AFTER_FIRST_RENDER")
        .map(|v| !v.is_empty() && v != "0" && v != "false")
        .unwrap_or(false)
    {
        eprintln!("\nStartup time: 0ms");
        std::process::exit(0);
    }

    // 2. 序列化 prompt 用于 agent 入参
    let prompt_payload = serde_json::to_value(&input_prompt)?;
    info!(
        prompt_size = prompt_payload.to_string().len(),
        agent = ?opts.agent,
        workload = ?opts.workload,
        "dispatching headless query"
    );

    // 3. 真实查询通过 mossen-agent 驱动。
    //
    // 提取 prompt 文本：blocks 中所有 Text 拼接；
    // 然后通过 `repl::submit_once` 真实调用 SessionOrchestrator。
    let prompt_text = match &input_prompt {
        PromptValue::Text(s) => s.clone(),
        PromptValue::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                PromptBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    };

    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));
    let model = opts.user_specified_model.clone();
    let start = std::time::Instant::now();
    let result_text = crate::repl::submit_once(&prompt_text, model.clone(), cwd)
        .await
        .unwrap_or_else(|e| format!("[error] {}", e));
    let duration_ms = start.elapsed().as_millis() as u64;

    let session_id = uuid::Uuid::new_v4().to_string();
    if matches!(opts.output_format.as_deref(), Some("json")) {
        let result = serde_json::json!({
            "session_id": session_id,
            "result": result_text,
            "duration_ms": duration_ms,
            "total_cost_usd": 0.0,
        });
        println!("{}", result);
    } else if matches!(
        opts.output_format.as_deref(),
        Some("json_lines") | Some("stream-json")
    ) {
        // 每条消息一行 JSON
        let assistant = serde_json::json!({
            "type": "assistant",
            "session_id": session_id,
            "content": result_text,
        });
        println!("{}", assistant);
        let result = serde_json::json!({
            "type": "result",
            "session_id": session_id,
            "duration_ms": duration_ms,
            "total_cost_usd": 0.0,
        });
        println!("{}", result);
    } else {
        println!("{}", result_text);
    }
    Ok(())
}

pub async fn runHeadless(input_prompt: PromptValue, opts: RunHeadlessOptions) -> Result<()> {
    run_headless(input_prompt, opts).await
}

// ----------------------------------------------------------------------------
// Permission prompt
// ----------------------------------------------------------------------------

/// 权限决策结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior")]
pub enum PermissionPromptDecision {
    #[serde(rename = "allow")]
    Allow {
        #[serde(rename = "updatedInput", skip_serializing_if = "Option::is_none")]
        updated_input: Option<Value>,
    },
    #[serde(rename = "deny")]
    Deny { message: String },
}

/// 调用 SDK MCP 权限提示工具的回调类型。
pub type PermissionPromptCallback = std::sync::Arc<
    dyn Fn(
            String,
            Value,
        ) -> futures_util::future::BoxFuture<'static, Result<PermissionPromptDecision>>
        + Send
        + Sync,
>;

/// 工具调用上下文。
#[derive(Debug, Clone)]
pub struct ToolCallContext {
    pub tool_name: String,
    pub input: Value,
    pub session_id: String,
}

/// canUseTool 决策。
#[derive(Debug, Clone)]
pub enum CanUseToolResult {
    Allow { updated_input: Option<Value> },
    Deny { reason: String },
}

fn value_declares_tool(value: &Value, prompt_tool: &str, depth: usize) -> bool {
    if depth > 6 {
        return false;
    }

    match value {
        Value::String(name) => name == prompt_tool,
        Value::Array(items) => items
            .iter()
            .any(|item| value_declares_tool(item, prompt_tool, depth + 1)),
        Value::Object(map) => {
            for key in ["name", "toolName", "id"] {
                if map.get(key).and_then(Value::as_str) == Some(prompt_tool) {
                    return true;
                }
            }

            for key in ["tools", "availableTools", "toolNames"] {
                if map
                    .get(key)
                    .map(|child| value_declares_tool(child, prompt_tool, depth + 1))
                    .unwrap_or(false)
                {
                    return true;
                }
            }

            if map
                .get("capabilities")
                .and_then(|capabilities| capabilities.get("tools"))
                .map(|tools| value_declares_tool(tools, prompt_tool, depth + 1))
                .unwrap_or(false)
            {
                return true;
            }

            map.contains_key(prompt_tool)
        }
        _ => false,
    }
}

fn find_permission_prompt_server(
    sdk_mcp_servers: &HashMap<String, Value>,
    prompt_tool: &str,
) -> Option<String> {
    sdk_mcp_servers
        .iter()
        .find(|(_, server)| value_declares_tool(server, prompt_tool, 0))
        .map(|(name, _)| name.clone())
}

/// 创建一个使用 SDK MCP permission_prompt_tool 决策的 canUseTool 函数。
pub fn create_can_use_tool_with_permission_prompt_callback(
    permission_prompt_tool_name: String,
    sdk_mcp_servers: HashMap<String, Value>,
    prompt_callback: Option<PermissionPromptCallback>,
) -> impl Fn(ToolCallContext) -> futures_util::future::BoxFuture<'static, CanUseToolResult>
       + Send
       + Sync
       + Clone {
    let prompt_tool = permission_prompt_tool_name;
    let servers = sdk_mcp_servers;
    let callback = prompt_callback;
    move |ctx: ToolCallContext| {
        let prompt_tool = prompt_tool.clone();
        let servers = servers.clone();
        let callback = callback.clone();
        Box::pin(async move {
            let Some(server_name) = find_permission_prompt_server(&servers, &prompt_tool) else {
                let reason = format!(
                    "Permission prompt tool '{}' is not advertised by any SDK MCP server; refusing to run '{}'",
                    prompt_tool, ctx.tool_name
                );
                warn!(tool = %ctx.tool_name, prompt_tool = %prompt_tool, "permission prompt unavailable");
                return CanUseToolResult::Deny { reason };
            };

            let Some(callback) = callback else {
                let reason = format!(
                    "Permission prompt tool '{}' on server '{}' has no callable bridge; refusing to run '{}'",
                    prompt_tool, server_name, ctx.tool_name
                );
                warn!(
                    tool = %ctx.tool_name,
                    prompt_tool = %prompt_tool,
                    server = %server_name,
                    "permission prompt bridge unavailable"
                );
                return CanUseToolResult::Deny { reason };
            };

            let prompt_input = serde_json::json!({
                "toolName": ctx.tool_name,
                "input": ctx.input,
                "sessionId": ctx.session_id,
                "serverName": server_name,
            });

            match callback(prompt_tool.clone(), prompt_input).await {
                Ok(PermissionPromptDecision::Allow { updated_input }) => {
                    info!(prompt_tool = %prompt_tool, "permission prompt allowed tool use");
                    CanUseToolResult::Allow { updated_input }
                }
                Ok(PermissionPromptDecision::Deny { message }) => {
                    info!(prompt_tool = %prompt_tool, "permission prompt denied tool use");
                    CanUseToolResult::Deny { reason: message }
                }
                Err(error) => {
                    let reason = format!(
                        "Permission prompt tool '{}' failed: {}; refusing to run tool",
                        prompt_tool, error
                    );
                    warn!(prompt_tool = %prompt_tool, "permission prompt callback failed");
                    CanUseToolResult::Deny { reason }
                }
            }
        })
    }
}

pub fn create_can_use_tool_with_permission_prompt(
    permission_prompt_tool_name: String,
    sdk_mcp_servers: HashMap<String, Value>,
) -> impl Fn(ToolCallContext) -> futures_util::future::BoxFuture<'static, CanUseToolResult>
       + Send
       + Sync
       + Clone {
    create_can_use_tool_with_permission_prompt_callback(
        permission_prompt_tool_name,
        sdk_mcp_servers,
        None,
    )
}

pub fn createCanUseToolWithPermissionPrompt(
    permission_prompt_tool_name: String,
    sdk_mcp_servers: HashMap<String, Value>,
) -> impl Fn(ToolCallContext) -> futures_util::future::BoxFuture<'static, CanUseToolResult>
       + Send
       + Sync
       + Clone {
    create_can_use_tool_with_permission_prompt(permission_prompt_tool_name, sdk_mcp_servers)
}

// ----------------------------------------------------------------------------
// Misc helpers
// ----------------------------------------------------------------------------

/// 获取默认 canUseTool 函数（不询问，直接允许）。
pub fn get_can_use_tool_fn(
) -> impl Fn(ToolCallContext) -> futures_util::future::BoxFuture<'static, CanUseToolResult>
       + Send
       + Sync
       + Clone {
    move |_ctx: ToolCallContext| {
        Box::pin(async move {
            CanUseToolResult::Allow {
                updated_input: None,
            }
        })
    }
}

pub fn getCanUseToolFn(
) -> impl Fn(ToolCallContext) -> futures_util::future::BoxFuture<'static, CanUseToolResult>
       + Send
       + Sync
       + Clone {
    get_can_use_tool_fn()
}

/// 从消息列表中移除被中断的消息。
///
/// 实现策略：从尾部检查最后一条消息的 `interrupted` 字段；
/// 若为 true 则弹出。对应 TS `removeInterruptedMessage()`。
pub fn remove_interrupted_message(messages: &mut Vec<Value>) -> bool {
    if messages.is_empty() {
        return false;
    }
    if let Some(last) = messages.last() {
        if last.get("interrupted").and_then(|v| v.as_bool()) == Some(true) {
            messages.pop();
            return true;
        }
    }
    false
}

pub fn removeInterruptedMessage(messages: &mut Vec<Value>) -> bool {
    remove_interrupted_message(messages)
}

/// 处理 orphan 权限响应。
pub async fn handle_orphaned_permission_response(request_id: String) -> Result<()> {
    info!(request_id, "discarding orphaned permission response");
    Ok(())
}

pub async fn handleOrphanedPermissionResponse(request_id: String) -> Result<()> {
    handle_orphaned_permission_response(request_id).await
}

// ----------------------------------------------------------------------------
// MCP state types
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DynamicMcpState {
    pub servers: HashMap<String, Value>,
    pub last_set_at: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SdkMcpState {
    pub servers: HashMap<String, Value>,
    pub instances: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSetServersResult {
    pub success: bool,
    pub set_count: usize,
    pub errors: Vec<String>,
}

pub async fn handle_mcp_set_servers(
    state: &mut DynamicMcpState,
    servers: HashMap<String, Value>,
) -> Result<McpSetServersResult> {
    state.servers = servers;
    state.last_set_at = Some(chrono::Utc::now().timestamp_millis());
    Ok(McpSetServersResult {
        success: true,
        set_count: state.servers.len(),
        errors: Vec::new(),
    })
}

pub async fn handleMcpSetServers(
    state: &mut DynamicMcpState,
    servers: HashMap<String, Value>,
) -> Result<McpSetServersResult> {
    handle_mcp_set_servers(state, servers).await
}

pub async fn reconcile_mcp_servers(
    state: &mut DynamicMcpState,
    desired: HashMap<String, Value>,
) -> Result<()> {
    state.servers = desired;
    Ok(())
}

pub async fn reconcileMcpServers(
    state: &mut DynamicMcpState,
    desired: HashMap<String, Value>,
) -> Result<()> {
    reconcile_mcp_servers(state, desired).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use serde_json::json;
    use std::sync::Arc;

    fn tool_ctx() -> ToolCallContext {
        ToolCallContext {
            tool_name: "Bash".to_string(),
            input: json!({ "command": "rm -rf /tmp/example" }),
            session_id: "session-1".to_string(),
        }
    }

    fn server_with_prompt_tool() -> HashMap<String, Value> {
        HashMap::from([(
            "sdk".to_string(),
            json!({
                "tools": [
                    { "name": "permission_prompt" }
                ]
            }),
        )])
    }

    #[tokio::test]
    async fn permission_prompt_denies_when_tool_is_not_advertised() {
        let can_use = create_can_use_tool_with_permission_prompt(
            "permission_prompt".to_string(),
            HashMap::new(),
        );

        let result = can_use(tool_ctx()).await;

        match result {
            CanUseToolResult::Deny { reason } => {
                assert!(reason.contains("not advertised"));
            }
            CanUseToolResult::Allow { .. } => panic!("permission prompt must fail closed"),
        }
    }

    #[tokio::test]
    async fn permission_prompt_denies_when_bridge_is_missing() {
        let can_use = create_can_use_tool_with_permission_prompt(
            "permission_prompt".to_string(),
            server_with_prompt_tool(),
        );

        let result = can_use(tool_ctx()).await;

        match result {
            CanUseToolResult::Deny { reason } => {
                assert!(reason.contains("no callable bridge"));
            }
            CanUseToolResult::Allow { .. } => panic!("missing bridge must fail closed"),
        }
    }

    #[tokio::test]
    async fn permission_prompt_callback_can_allow_with_updated_input() {
        let callback: PermissionPromptCallback = Arc::new(|tool_name, input| {
            Box::pin(async move {
                assert_eq!(tool_name, "permission_prompt");
                assert_eq!(input["toolName"], "Bash");
                Ok(PermissionPromptDecision::Allow {
                    updated_input: Some(json!({ "command": "echo safe" })),
                })
            })
        });
        let can_use = create_can_use_tool_with_permission_prompt_callback(
            "permission_prompt".to_string(),
            server_with_prompt_tool(),
            Some(callback),
        );

        let result = can_use(tool_ctx()).await;

        match result {
            CanUseToolResult::Allow {
                updated_input: Some(updated_input),
            } => {
                assert_eq!(updated_input["command"], "echo safe");
            }
            other => panic!("expected allow with updated input, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn permission_prompt_callback_errors_fail_closed() {
        let callback: PermissionPromptCallback =
            Arc::new(|_, _| Box::pin(async { Err(anyhow!("transport down")) }));
        let can_use = create_can_use_tool_with_permission_prompt_callback(
            "permission_prompt".to_string(),
            server_with_prompt_tool(),
            Some(callback),
        );

        let result = can_use(tool_ctx()).await;

        match result {
            CanUseToolResult::Deny { reason } => {
                assert!(reason.contains("transport down"));
            }
            CanUseToolResult::Allow { .. } => panic!("callback errors must fail closed"),
        }
    }
}
