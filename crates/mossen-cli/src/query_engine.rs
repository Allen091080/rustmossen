//! Query & Tool 模块 — 翻译自根目录核心文件
//!
//! 包含：
//! - Tool.ts → 工具系统类型与 trait
//! - query.ts → 查询执行引擎
//! - QueryEngine.ts → 查询引擎入口
//! - tools.ts → 工具注册表
//! - setup.ts → 设置初始化
//! - main.tsx → 主应用框架（非 React 部分）

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════════
// Tool.ts — 工具系统类型定义
// ═══════════════════════════════════════════════════════════════════════════════

/// 工具输入 JSON Schema。
pub type ToolInputJsonSchema = serde_json::Value;

/// 查询链跟踪。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryChainTracking {
    pub chain_id: String,
    pub depth: u32,
}

/// 输入验证结果。
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Valid,
    Invalid { message: String, error_code: i32 },
}

/// 工具权限上下文。
#[derive(Debug, Clone, Default)]
pub struct ToolPermissionContext {
    pub mode: String,
    pub additional_working_directories: HashMap<String, AdditionalWorkingDirectory>,
    pub always_allow_rules: HashMap<String, Vec<serde_json::Value>>,
    pub always_deny_rules: HashMap<String, Vec<serde_json::Value>>,
    pub always_ask_rules: HashMap<String, Vec<serde_json::Value>>,
    pub is_bypass_permissions_mode_available: bool,
    pub is_auto_mode_available: bool,
    pub should_avoid_permission_prompts: bool,
    pub await_automated_checks_before_dialog: bool,
    pub pre_plan_mode: Option<String>,
}

/// 额外工作目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdditionalWorkingDirectory {
    pub path: String,
    pub writable: bool,
}

/// 获取空的工具权限上下文。
pub fn get_empty_tool_permission_context() -> ToolPermissionContext {
    ToolPermissionContext {
        mode: "default".to_string(),
        ..Default::default()
    }
}

/// 压缩进度事件。
#[derive(Debug, Clone)]
pub enum CompactProgressEvent {
    HooksStart { hook_type: String },
    CompactStart,
    CompactEnd,
}

/// 工具使用上下文。
#[derive(Debug, Clone)]
pub struct ToolUseContext {
    pub options: ToolUseOptions,
    pub messages: Vec<serde_json::Value>,
    pub user_modified: bool,
    pub tool_use_id: Option<String>,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub require_can_use_tool: bool,
}

/// 工具使用选项。
#[derive(Debug, Clone)]
pub struct ToolUseOptions {
    pub debug: bool,
    pub main_loop_model: String,
    pub verbose: bool,
    pub is_non_interactive_session: bool,
    pub custom_system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub query_source: Option<String>,
    pub max_budget_usd: Option<f64>,
}

/// 工具结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_messages: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_meta: Option<serde_json::Value>,
}

/// 工具进度数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgressData {
    #[serde(rename = "type")]
    pub progress_type: String,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 进度回调。
pub type ToolCallProgress = Box<dyn Fn(String, ToolProgressData) + Send + Sync>;

/// 搜索或读取命令检查结果。
#[derive(Debug, Clone, Default)]
pub struct SearchOrReadInfo {
    pub is_search: bool,
    pub is_read: bool,
    pub is_list: bool,
}

/// 检查工具是否匹配名称（含别名）。
pub fn tool_matches_name(name: &str, tool_name: &str, aliases: &[String]) -> bool {
    name == tool_name || aliases.iter().any(|a| a == name)
}

/// 从工具列表中按名称查找。
pub fn find_tool_by_name<'a>(tools: &'a [ToolDefinition], name: &str) -> Option<&'a ToolDefinition> {
    tools.iter().find(|t| tool_matches_name(name, &t.name, &t.aliases))
}

/// 工具定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_hint: Option<String>,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_json_schema: Option<ToolInputJsonSchema>,
    pub is_enabled: bool,
    pub is_read_only: bool,
    pub is_concurrency_safe: bool,
    pub max_result_size_chars: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should_defer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_load: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_info: Option<McpToolInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    pub is_mcp: bool,
    pub is_lsp: bool,
}

/// MCP 工具信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub server_name: String,
    pub tool_name: String,
}

/// 过滤工具进度消息（排除 hook 进度）。
pub fn filter_tool_progress_messages(
    messages: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    messages
        .iter()
        .filter(|msg| {
            msg.get("data")
                .and_then(|d| d.get("type"))
                .and_then(|t| t.as_str())
                .map(|t| t != "hook_progress")
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// tools.ts — 工具注册表
// ═══════════════════════════════════════════════════════════════════════════════

/// 获取所有内置工具名称。
pub fn get_all_tool_names() -> Vec<&'static str> {
    vec![
        "BashTool",
        "FileReadTool",
        "FileWriteTool",
        "FileEditTool",
        "GlobTool",
        "GrepTool",
        "AgentTool",
        "SkillTool",
        "WebFetchTool",
        "NotebookEditTool",
        "TaskStopTool",
        "BriefTool",
    ]
}

/// 根据特性标志获取活跃工具名称。
pub fn get_active_tool_names() -> Vec<String> {
    let mut tools: Vec<String> = get_all_tool_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    // 条件工具
    if std::env::var("MOSSEN_PROACTIVE").ok().as_deref() == Some("1") {
        tools.push("SleepTool".to_string());
    }
    if std::env::var("MOSSEN_MONITOR_TOOL").ok().as_deref() == Some("1") {
        tools.push("MonitorTool".to_string());
    }

    tools
}

/// 工具列表类型别名。
pub type Tools = Vec<ToolDefinition>;

/// 获取初始工具列表。
pub fn get_initial_tools() -> Tools {
    let names = get_active_tool_names();
    names
        .iter()
        .map(|name| ToolDefinition {
            name: name.clone(),
            aliases: Vec::new(),
            search_hint: None,
            description: format!("{} tool", name),
            input_schema: serde_json::json!({"type": "object"}),
            input_json_schema: None,
            is_enabled: true,
            is_read_only: matches!(
                name.as_str(),
                "FileReadTool" | "GlobTool" | "GrepTool" | "WebFetchTool"
            ),
            is_concurrency_safe: matches!(
                name.as_str(),
                "FileReadTool" | "GlobTool" | "GrepTool" | "WebFetchTool"
            ),
            max_result_size_chars: if matches!(name.as_str(), "FileReadTool") {
                usize::MAX
            } else {
                30_000
            },
            should_defer: None,
            always_load: None,
            mcp_info: None,
            strict: None,
            is_mcp: false,
            is_lsp: false,
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// query.ts — 查询执行引擎（核心逻辑）
// ═══════════════════════════════════════════════════════════════════════════════

/// 查询源标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuerySource {
    User,
    Tool,
    AutoCompact,
    Resume,
    Retry,
    Headless,
    Sdk,
}

/// 查询配置。
#[derive(Debug, Clone)]
pub struct QueryConfig {
    pub model: String,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub thinking_enabled: bool,
    pub thinking_budget: Option<u32>,
    pub stop_sequences: Vec<String>,
    pub temperature: Option<f32>,
}

/// 查询结果。
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub messages: Vec<serde_json::Value>,
    pub stop_reason: StopReason,
    pub usage: QueryUsage,
    pub duration_ms: u64,
}

/// 停止原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    StopSequence,
    UserInterrupt,
    Error,
}

/// 查询用量。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

/// 自动压缩跟踪状态。
#[derive(Debug, Clone, Default)]
pub struct AutoCompactTrackingState {
    pub last_compact_token_count: u64,
    pub compact_count: u32,
    pub token_warning_state: TokenWarningState,
}

/// Token 警告状态。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TokenWarningState {
    #[default]
    None,
    Warning,
    Critical,
}

/// 计算 token 警告状态。
pub fn calculate_token_warning_state(
    input_tokens: u64,
    context_window: u64,
) -> TokenWarningState {
    let ratio = input_tokens as f64 / context_window as f64;
    if ratio > 0.9 {
        TokenWarningState::Critical
    } else if ratio > 0.7 {
        TokenWarningState::Warning
    } else {
        TokenWarningState::None
    }
}

/// 检查是否启用了自动压缩。
pub fn is_auto_compact_enabled() -> bool {
    std::env::var("MOSSEN_CODE_DISABLE_AUTO_COMPACT")
        .map(|v| v != "1" && v != "true")
        .unwrap_or(true)
}

/// 构建查询配置。
pub fn build_query_config(
    model: &str,
    system_prompt: &str,
    thinking_enabled: bool,
    thinking_budget: Option<u32>,
) -> QueryConfig {
    let max_tokens = if thinking_enabled { 16384 } else { 8192 };

    QueryConfig {
        model: model.to_string(),
        max_tokens,
        system_prompt: system_prompt.to_string(),
        thinking_enabled,
        thinking_budget,
        stop_sequences: Vec::new(),
        temperature: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// QueryEngine.ts — 查询引擎入口
// ═══════════════════════════════════════════════════════════════════════════════

/// SDK 权限模式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdkPermissionMode {
    Default,
    AutoAccept,
    BypassAll,
}

/// SDK 消息重放。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkUserMessageReplay {
    pub content: String,
    pub images: Vec<serde_json::Value>,
}

/// SDK 状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 查询引擎。
pub struct QueryEngine {
    model: String,
    system_prompt: String,
    messages: Vec<serde_json::Value>,
    tools: Tools,
    abort_controller: Arc<tokio::sync::Notify>,
    is_running: bool,
    conversation_id: Option<String>,
    thinking_config: ThinkingConfig,
}

/// 思考模式配置。
#[derive(Debug, Clone)]
pub struct ThinkingConfig {
    pub enabled: bool,
    pub budget: Option<u32>,
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        Self {
            enabled: should_enable_thinking_by_default(),
            budget: None,
        }
    }
}

/// 是否默认启用思考模式。
pub fn should_enable_thinking_by_default() -> bool {
    std::env::var("MOSSEN_CODE_THINKING")
        .map(|v| v != "0" && v != "false")
        .unwrap_or(true)
}

impl QueryEngine {
    /// 创建新的查询引擎。
    pub fn new(
        model: String,
        system_prompt: String,
        tools: Tools,
        thinking_config: ThinkingConfig,
    ) -> Self {
        Self {
            model,
            system_prompt,
            messages: Vec::new(),
            tools,
            abort_controller: Arc::new(tokio::sync::Notify::new()),
            is_running: false,
            conversation_id: Some(uuid::Uuid::new_v4().to_string()),
            thinking_config,
        }
    }

    /// 获取当前消息列表。
    pub fn messages(&self) -> &[serde_json::Value] {
        &self.messages
    }

    /// 添加用户消息。
    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(serde_json::json!({
            "role": "user",
            "content": content,
        }));
    }

    /// 中止当前查询。
    pub fn abort(&self) {
        self.abort_controller.notify_waiters();
    }

    /// 检查是否正在运行。
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// 获取对话 ID。
    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    /// 获取模型名称。
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 清空消息。
    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// setup.ts — 初始化流程
// ═══════════════════════════════════════════════════════════════════════════════

/// 设置参数。
#[derive(Debug, Clone)]
pub struct SetupParams {
    pub cwd: String,
    pub permission_mode: String,
    pub allow_dangerously_skip_permissions: bool,
    pub worktree_enabled: bool,
    pub worktree_name: Option<String>,
    pub tmux_enabled: bool,
    pub custom_session_id: Option<String>,
    pub worktree_pr_number: Option<u64>,
    pub messaging_socket_path: Option<String>,
}

/// 执行初始化设置。
pub async fn setup(params: SetupParams) -> anyhow::Result<()> {
    tracing::info!("setup_started");

    // 设置工作目录
    let cwd = std::path::PathBuf::from(&params.cwd);
    if cwd.exists() {
        std::env::set_current_dir(&cwd)?;
    }

    // 查找 git root
    let git_root = find_git_root(&params.cwd).await;
    if let Some(ref root) = git_root {
        tracing::info!(git_root = %root, "Found git root");
    }

    // 初始化配置
    mossen_utils::config::enable_configs();

    // 初始化会话记忆
    tracing::debug!("Initializing session memory");

    // 权限模式
    tracing::info!(
        mode = %params.permission_mode,
        skip_permissions = params.allow_dangerously_skip_permissions,
        "Permission mode configured"
    );

    // Worktree 设置
    if params.worktree_enabled {
        if let Some(ref name) = params.worktree_name {
            tracing::info!(worktree = %name, "Worktree enabled");
        }
    }

    tracing::info!("setup_completed");
    Ok(())
}

/// 查找 git 根目录。
async fn find_git_root(cwd: &str) -> Option<String> {
    let output = tokio::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        Some(
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string(),
        )
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// main.tsx 核心逻辑（非 React 部分）
// ═══════════════════════════════════════════════════════════════════════════════

/// 应用启动模式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupMode {
    /// 交互式 REPL。
    Interactive,
    /// 一次性查询。
    Oneshot { prompt: String },
    /// 恢复会话。
    Resume { session_id: Option<String> },
    /// 打印模式（非交互）。
    Print { prompt: String },
    /// SDK 模式。
    Sdk,
    /// 子命令。
    SubCommand { name: String },
}

/// 初始化前的预检。
pub async fn pre_flight_checks() -> anyhow::Result<()> {
    // 检查终端大小
    if let Ok((cols, _rows)) = crossterm::terminal::size() {
        if cols < 40 {
            tracing::warn!(cols = cols, "Terminal width very small, may cause display issues");
        }
    }

    // 检查磁盘空间
    let config_dir = mossen_utils::env::get_mossen_config_home_dir();
    if !config_dir.exists() {
        tokio::fs::create_dir_all(&config_dir).await?;
    }

    Ok(())
}

/// 延迟预取（在主 UI 启动后执行）。
pub fn start_deferred_prefetches() {
    // 后台预热缓存
    tokio::spawn(async {
        // 预取 API key 验证
        tracing::debug!("Starting deferred prefetches");

        // 预取 MCP 服务器配置
        let cwd = std::env::current_dir().unwrap_or_default();
        let global_dir = mossen_utils::env::get_mossen_config_home_dir();
        let _ = mossen_mcp::config::load_merged_configs(&cwd, &global_dir).await;

        tracing::debug!("Deferred prefetches completed");
    });
}

/// 确定启动模式。
pub fn determine_startup_mode(
    prompt: Option<&str>,
    resume: bool,
    session_id: Option<&str>,
    print: bool,
    sub_command: Option<&str>,
) -> StartupMode {
    if let Some(cmd) = sub_command {
        return StartupMode::SubCommand {
            name: cmd.to_string(),
        };
    }

    if resume {
        return StartupMode::Resume {
            session_id: session_id.map(|s| s.to_string()),
        };
    }

    if let Some(p) = prompt {
        if print {
            return StartupMode::Print {
                prompt: p.to_string(),
            };
        }
        return StartupMode::Oneshot {
            prompt: p.to_string(),
        };
    }

    if std::env::var("MOSSEN_AGENT_SDK_MODE").ok().as_deref() == Some("1") {
        return StartupMode::Sdk;
    }

    StartupMode::Interactive
}
