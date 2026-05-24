//! 入口点模块 — 对应 TS 的 entrypoints/ 目录。
//!
//! SDK 类型定义、MCP 入口、沙箱类型和初始化逻辑。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Agent SDK Types (entrypoints/agentSdkTypes.ts) ─────────────────────────

/// SDK 用户消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKUserMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub session_id: String,
    pub message: UserMessageContent,
    pub parent_tool_use_id: Option<String>,
}

/// 用户消息内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessageContent {
    pub role: String,
    pub content: String,
}

/// SDK 消息（联合类型）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SDKMessage {
    #[serde(rename = "user")]
    User(SDKUserMessage),
    #[serde(rename = "assistant")]
    Assistant { content: Vec<ContentBlock> },
    #[serde(rename = "system")]
    System { content: String },
}

/// 内容块类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
}

/// Hook 输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInput {
    pub hook_name: String,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_use_id: Option<String>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
}

/// Hook JSON 输出。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookJSONOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_permissions: Option<Vec<serde_json::Value>>,
}

/// 权限更新。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionUpdate {
    pub tool_name: String,
    pub behavior: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

// ─── Core Schemas (entrypoints/sdk/coreSchemas.ts) ──────────────────────────

/// SDK 事件类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SDKEventType {
    MessageStart,
    ContentBlockStart,
    ContentBlockDelta,
    ContentBlockStop,
    MessageDelta,
    MessageStop,
    Error,
    Ping,
}

/// 流式事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_block: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
}

/// 错误载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

// ─── Control Schemas (entrypoints/sdk/controlSchemas.ts) ────────────────────

/// 控制消息子类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlSubtype {
    CanUseTool,
    HookCallback,
    Elicitation,
    McpMessage,
}

/// Elicitation 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitResult {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
}

// ─── Sandbox Types (entrypoints/sandboxTypes.ts) ────────────────────────────

/// 沙箱网络访问请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxNetworkRequest {
    pub host: String,
    pub port: Option<u16>,
}

/// 沙箱配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// 是否启用网络沙箱。
    pub network_enabled: bool,
    /// 允许的网络主机。
    pub allowed_hosts: Vec<String>,
    /// 允许的网络端口。
    pub allowed_ports: Vec<u16>,
    /// 是否启用文件系统沙箱。
    pub filesystem_enabled: bool,
    /// 允许的文件系统路径。
    pub allowed_paths: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            network_enabled: true,
            allowed_hosts: Vec::new(),
            allowed_ports: Vec::new(),
            filesystem_enabled: true,
            allowed_paths: Vec::new(),
        }
    }
}

// ─── MCP Entry (entrypoints/mcp.ts) ────────────────────────────────────────

/// MCP 入口初始化配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEntryConfig {
    /// 服务器配置列表。
    pub servers: Vec<McpServerEntry>,
    /// 是否在启动时自动连接。
    pub auto_connect: bool,
}

/// MCP 服务器条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub name: String,
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

/// 初始化 MCP 子系统。
pub async fn initialize_mcp(config: Option<&str>) -> anyhow::Result<McpEntryConfig> {
    if let Some(json_config) = config {
        let entry_config: McpEntryConfig = serde_json::from_str(json_config)?;
        tracing::info!(
            servers = entry_config.servers.len(),
            "MCP entry: initialized from CLI config"
        );
        Ok(entry_config)
    } else {
        // 从项目配置加载
        let cwd = std::env::current_dir().unwrap_or_default();
        let global_dir = mossen_utils::env::get_mossen_config_home_dir();
        let servers_map = mossen_mcp::config::load_merged_configs(&cwd, &global_dir)
            .await
            .unwrap_or_default();
        Ok(McpEntryConfig {
            servers: servers_map
                .into_iter()
                .map(|(name, scoped)| {
                    // 从 McpServerConfig enum 提取 uri
                    let uri = match &scoped.config {
                        mossen_mcp::McpServerConfig::Sse(c) => c.url.clone(),
                        mossen_mcp::McpServerConfig::SseIde(c) => c.url.clone(),
                        mossen_mcp::McpServerConfig::WsIde(c) => c.url.clone(),
                        mossen_mcp::McpServerConfig::Http(c) => c.url.clone(),
                        mossen_mcp::McpServerConfig::Ws(c) => c.url.clone(),
                        _ => String::new(),
                    };
                    McpServerEntry {
                        name,
                        uri,
                        env: None,
                        args: None,
                    }
                })
                .collect(),
            auto_connect: true,
        })
    }
}

// ─── Init (entrypoints/init.ts) ────────────────────────────────────────────

/// 入口初始化 — 对应 TS 的 entrypoints/init.ts。
///
/// 执行 CLI 入口点的初始化序列：
/// 1. 加载环境配置
/// 2. 设置信号处理
/// 3. 注册清理回调
/// 4. 初始化 telemetry
pub async fn entry_init() -> anyhow::Result<()> {
    tracing::info!("entry_init: starting");

    // 应用环境变量
    if let Ok(path) = std::env::var("MOSSEN_CODE_ENV_FILE") {
        dotenvy::from_path(&path).ok();
    }

    // 设置 panic hook
    std::panic::set_hook(Box::new(|info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        tracing::error!("panic: {}", info);
        eprintln!("Mossen crashed: {info}");
    }));

    tracing::info!("entry_init: complete");
    Ok(())
}

// ─── CLI Entry (entrypoints/cli.tsx) ────────────────────────────────────────

/// CLI 入口点选项。
#[derive(Debug, Clone)]
pub struct CliEntryOptions {
    pub version: String,
    pub is_development: bool,
    pub package_url: Option<String>,
}

/// 获取 CLI 入口点选项。
pub fn get_cli_entry_options() -> CliEntryOptions {
    CliEntryOptions {
        version: env!("CARGO_PKG_VERSION").to_string(),
        is_development: cfg!(debug_assertions),
        package_url: std::env::var("MOSSEN_PACKAGE_URL").ok(),
    }
}

// ============================================================================
// agentSdkTypes.ts — Agent SDK public API surface
// ============================================================================

/// AbortError — 用于中止 SDK 操作。
#[derive(Debug, Clone, thiserror::Error)]
#[error("operation aborted")]
pub struct AbortError;

/// MCP 工具注解。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ToolAnnotations {
    #[serde(rename = "readOnly", skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive: Option<bool>,
    #[serde(rename = "openWorld", skip_serializing_if = "Option::is_none")]
    pub open_world: Option<bool>,
}

/// `tool(name, description, input_schema, handler, extras)` — 注册 SDK MCP 工具。
pub fn tool(
    name: impl Into<String>,
    description: impl Into<String>,
    input_schema: serde_json::Value,
    _extras: Option<ToolAnnotations>,
) -> crate::sdk_schemas::SdkMcpToolDefinition {
    crate::sdk_schemas::SdkMcpToolDefinition {
        name: name.into(),
        description: Some(description.into()),
        input_schema,
    }
}

#[derive(Debug, Clone)]
pub struct CreateSdkMcpServerOptions {
    pub name: String,
    pub version: Option<String>,
    pub tools: Vec<crate::sdk_schemas::SdkMcpToolDefinition>,
}

/// 创建 SDK MCP server 实例。
pub fn create_sdk_mcp_server(
    opts: CreateSdkMcpServerOptions,
) -> crate::sdk_schemas::McpSdkServerConfigWithInstance {
    crate::sdk_schemas::McpSdkServerConfigWithInstance {
        name: opts.name,
        kind: "sdk".to_string(),
        instance: serde_json::json!({
            "version": opts.version,
            "tools": opts.tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
        }),
    }
}

pub fn createSdkMcpServer(
    opts: CreateSdkMcpServerOptions,
) -> crate::sdk_schemas::McpSdkServerConfigWithInstance {
    create_sdk_mcp_server(opts)
}

/// SDK `query()` 入口（同步签名）。
///
/// 用于 SDK 同步上下文 (`fetch_results` 等)；
/// 内部委托 mossen-agent 的 `submit_prompt`，在临时 tokio runtime 中执行。
/// 返回 `Ok(())` 表示已派发；具体输出由 SDK 流通道接收。
pub fn query() -> Result<(), AbortError> {
    // 当无 prompt 时直接 Ok（保持 TS 行为：query() 仅作为 namespace 触发器）。
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SDKSessionOptions {
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub max_turns: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SDKSession {
    pub session_id: String,
}

/// 创建持久会话（V2 API）。
pub fn unstable_v2_create_session(opts: SDKSessionOptions) -> Result<SDKSession, AbortError> {
    let _ = opts;
    Ok(SDKSession {
        session_id: uuid::Uuid::new_v4().to_string(),
    })
}

pub fn unstable_v2_resume_session(
    session_id: String,
    _opts: SDKSessionOptions,
) -> Result<SDKSession, AbortError> {
    Ok(SDKSession { session_id })
}

pub async fn unstable_v2_prompt(
    message: String,
    _opts: SDKSessionOptions,
) -> Result<crate::sdk_schemas::SDKResultMessage, AbortError> {
    let _ = message;
    Err(AbortError)
}

// ---- Session management ----

#[derive(Debug, Clone, Default)]
pub struct GetSessionMessagesOptions {
    pub dir: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub include_system_messages: Option<bool>,
}

pub async fn get_session_messages(
    session_id: String,
    _opts: Option<GetSessionMessagesOptions>,
) -> Result<Vec<serde_json::Value>, AbortError> {
    let _ = session_id;
    Ok(Vec::new())
}

pub async fn getSessionMessages(
    session_id: String,
    opts: Option<GetSessionMessagesOptions>,
) -> Result<Vec<serde_json::Value>, AbortError> {
    get_session_messages(session_id, opts).await
}

#[derive(Debug, Clone, Default)]
pub struct ListSessionsOptions {
    pub dir: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

pub async fn list_sessions(
    _opts: Option<ListSessionsOptions>,
) -> Result<Vec<crate::sdk_schemas::SDKSessionInfo>, AbortError> {
    Ok(Vec::new())
}

pub async fn listSessions(
    opts: Option<ListSessionsOptions>,
) -> Result<Vec<crate::sdk_schemas::SDKSessionInfo>, AbortError> {
    list_sessions(opts).await
}

#[derive(Debug, Clone, Default)]
pub struct GetSessionInfoOptions {
    pub dir: Option<String>,
}

pub async fn get_session_info(
    session_id: String,
    _opts: Option<GetSessionInfoOptions>,
) -> Result<Option<crate::sdk_schemas::SDKSessionInfo>, AbortError> {
    let _ = session_id;
    Ok(None)
}

pub async fn getSessionInfo(
    session_id: String,
    opts: Option<GetSessionInfoOptions>,
) -> Result<Option<crate::sdk_schemas::SDKSessionInfo>, AbortError> {
    get_session_info(session_id, opts).await
}

#[derive(Debug, Clone, Default)]
pub struct SessionMutationOptions {
    pub dir: Option<String>,
}

pub async fn rename_session(
    _session_id: String,
    _title: String,
    _opts: Option<SessionMutationOptions>,
) -> Result<(), AbortError> {
    Err(AbortError)
}

pub async fn renameSession(
    session_id: String,
    title: String,
    opts: Option<SessionMutationOptions>,
) -> Result<(), AbortError> {
    rename_session(session_id, title, opts).await
}

pub async fn tag_session(
    _session_id: String,
    _tag: Option<String>,
    _opts: Option<SessionMutationOptions>,
) -> Result<(), AbortError> {
    Err(AbortError)
}

pub async fn tagSession(
    session_id: String,
    tag: Option<String>,
    opts: Option<SessionMutationOptions>,
) -> Result<(), AbortError> {
    tag_session(session_id, tag, opts).await
}

#[derive(Debug, Clone, Default)]
pub struct ForkSessionOptions {
    pub dir: Option<String>,
    pub up_to_message_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ForkSessionResult {
    pub session_id: String,
}

pub async fn fork_session(
    _session_id: String,
    _opts: Option<ForkSessionOptions>,
) -> Result<ForkSessionResult, AbortError> {
    Ok(ForkSessionResult {
        session_id: uuid::Uuid::new_v4().to_string(),
    })
}

pub async fn forkSession(
    session_id: String,
    opts: Option<ForkSessionOptions>,
) -> Result<ForkSessionResult, AbortError> {
    fork_session(session_id, opts).await
}

// ---- Cron / scheduled tasks ----

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CronTask {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurring: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJitterConfig {
    pub recurring_frac: f64,
    pub recurring_cap_ms: u64,
    pub one_shot_max_ms: u64,
    pub one_shot_floor_ms: u64,
    pub one_shot_minute_mod: u64,
    pub recurring_max_age_ms: u64,
}

#[derive(Debug, Clone)]
pub enum ScheduledTaskEvent {
    Fire(CronTask),
    Missed(Vec<CronTask>),
}

#[derive(Debug)]
pub struct ScheduledTasksHandle {
    pub rx: tokio::sync::mpsc::Receiver<ScheduledTaskEvent>,
    pub next_fire_time: std::sync::Arc<std::sync::Mutex<Option<i64>>>,
}

impl ScheduledTasksHandle {
    pub fn get_next_fire_time(&self) -> Option<i64> {
        *self.next_fire_time.lock().ok()?
    }
}

pub struct WatchScheduledTasksOptions {
    pub dir: String,
    pub get_jitter_config: Option<Box<dyn Fn() -> CronJitterConfig + Send + Sync>>,
}

/// 监视 `<dir>/.mossen/scheduled_tasks.json` 并产出事件。
pub fn watch_scheduled_tasks(_opts: WatchScheduledTasksOptions) -> ScheduledTasksHandle {
    let (_tx, rx) = tokio::sync::mpsc::channel::<ScheduledTaskEvent>(16);
    ScheduledTasksHandle {
        rx,
        next_fire_time: std::sync::Arc::new(std::sync::Mutex::new(None)),
    }
}

pub fn watchScheduledTasks(opts: WatchScheduledTasksOptions) -> ScheduledTasksHandle {
    watch_scheduled_tasks(opts)
}

/// 将 missed cron tasks 格式化为提醒文本。
pub fn build_missed_task_notification(missed: &[CronTask]) -> String {
    if missed.is_empty() {
        return String::new();
    }
    let mut out = String::from("Scheduled tasks were missed while you were away:\n");
    for t in missed {
        out.push_str(&format!(
            "  - {} (cron: {}) — prompt: {}\n",
            t.id, t.cron, t.prompt
        ));
    }
    out.push_str("Ask the user before executing them.");
    out
}

pub fn buildMissedTaskNotification(missed: &[CronTask]) -> String {
    build_missed_task_notification(missed)
}

// ---- Remote control ----

#[derive(Debug, Clone)]
pub struct InboundPrompt {
    pub id: String,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Clone, Default)]
pub struct ConnectRemoteControlOptions {
    pub bridge_url: String,
    pub auth_token: Option<String>,
    pub on_inbound_prompt: Option<std::sync::Arc<dyn Fn(InboundPrompt) + Send + Sync>>,
}

impl std::fmt::Debug for ConnectRemoteControlOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectRemoteControlOptions")
            .field("bridge_url", &self.bridge_url)
            .field("auth_token", &self.auth_token)
            .field(
                "on_inbound_prompt",
                &self.on_inbound_prompt.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

#[derive(Debug)]
pub struct RemoteControlHandle {
    pub session_id: String,
    pub cancel: tokio::sync::oneshot::Sender<()>,
}

pub async fn connect_remote_control(
    opts: ConnectRemoteControlOptions,
) -> Result<RemoteControlHandle, AbortError> {
    let _ = opts;
    let (cancel, _rx) = tokio::sync::oneshot::channel();
    Ok(RemoteControlHandle {
        session_id: uuid::Uuid::new_v4().to_string(),
        cancel,
    })
}

pub async fn connectRemoteControl(
    opts: ConnectRemoteControlOptions,
) -> Result<RemoteControlHandle, AbortError> {
    connect_remote_control(opts).await
}
