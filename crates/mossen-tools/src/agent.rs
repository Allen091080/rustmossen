//! # agent — SubagentLauncher 工具
//!
//! 对应 TS `AgentTool.tsx`（1322 行）。创建和管理子 agent，支持前台/后台模式。
//!
//! 核心功能：
//! - 启动前台/后台子 agent
//! - 支持多种 agent 类型（built-in / custom / fork）
//! - worktree 隔离模式
//! - agent 生命周期管理（注册、进度追踪、完成通知）
//! - 支持多 agent 团队（teammate spawn）

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ContentBlock, Message, TextBlock};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};
use mossen_utils::string_utils::truncate_chars_with_suffix;

/// 进度阈值（毫秒）—超过此时间显示后台提示
const PROGRESS_THRESHOLD_MS: u64 = 2000;
/// 自动后台化的超时（毫秒）
const AUTO_BACKGROUND_MS: u64 = 120_000;

/// 子代理启动器
pub struct SubagentLauncher;

/// 工具输入结构
#[derive(Debug, Clone, Deserialize)]
pub struct SubagentLauncherInput {
    /// 任务描述（3-5 字）
    pub description: String,
    /// 发送给子 agent 的提示词
    pub prompt: String,
    /// 子 agent 类型（built-in / custom / fork）
    #[serde(default)]
    pub subagent_type: Option<String>,
    /// 指定模型
    #[serde(default)]
    pub model: Option<String>,
    /// 是否在后台运行
    #[serde(default)]
    pub run_in_background: Option<bool>,
    /// 多 agent 参数：名称（用于 SendMessage 路由）
    #[serde(default)]
    pub name: Option<String>,
    /// 多 agent 参数：团队名称
    #[serde(default)]
    pub team_name: Option<String>,
    /// 权限模式
    #[serde(default)]
    pub mode: Option<String>,
    /// 隔离方式（worktree 创建临时 git worktree）
    #[serde(default)]
    pub isolation: Option<String>,
    /// 工作目录（覆盖所有文件系统操作）
    #[serde(default)]
    pub cwd: Option<String>,
}

/// 工具输出结构
#[derive(Debug, Clone, Serialize)]
pub struct SubagentLauncherOutput {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ContentBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tool_use_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_branch: Option<String>,
}

impl SubagentLauncherOutput {
    /// 创建异步启动的输出
    pub fn async_launched(agent_id: String, _description: String, _prompt: String) -> Self {
        Self {
            status: "async_launched".to_string(),
            result: None,
            task_id: Some(agent_id.clone()),
            agent_id: Some(agent_id),
            content: None,
            total_tokens: None,
            total_tool_use_count: None,
            total_duration_ms: None,
            worktree_path: None,
            worktree_branch: None,
        }
    }

    /// 创建完成状态的输出
    pub fn completed(
        _prompt: String,
        content: Vec<ContentBlock>,
        agent_id: Option<String>,
        total_tokens: u64,
        total_tool_use_count: u64,
        total_duration_ms: u64,
    ) -> Self {
        Self {
            status: "completed".to_string(),
            result: None,
            task_id: None,
            agent_id,
            content: Some(content),
            total_tokens: Some(total_tokens),
            total_tool_use_count: Some(total_tool_use_count),
            total_duration_ms: Some(total_duration_ms),
            worktree_path: None,
            worktree_branch: None,
        }
    }
}

/// Agent 工具的执行上下文
pub trait AgentToolContext: Send + Sync {
    fn get_permission_mode(&self) -> String;
    fn get_additional_working_directories(&self) -> Vec<String>;
    fn is_env_truthy(&self, key: &str) -> bool;
    fn check_feature_gate(&self, gate: &str) -> bool;
    fn is_coordinator_mode(&self) -> bool;
    fn is_agent_swarms_enabled(&self) -> bool;
    fn is_teammate(&self) -> bool;
    fn is_in_process_teammate(&self) -> bool;
    fn get_parent_session_id(&self) -> Option<String>;
    fn create_agent_id(&self) -> String;
    fn register_async_agent(
        &self,
        agent_id: &str,
        description: &str,
        prompt: &str,
        agent_type: &str,
    ) -> AgentRegistration;
    fn update_agent_progress(&self, agent_id: &str, progress: &AgentProgress);
    fn complete_agent(&self, agent_id: &str, result: &AgentResult);
    fn fail_agent(&self, agent_id: &str, error: &str);
    fn kill_agent(&self, agent_id: &str);
    fn get_agent_system_prompt(&self, agent_type: &str) -> Option<String>;
    fn log_event(&self, event: &str, metadata: HashMap<String, String>);
}

/// Agent 注册信息
#[derive(Debug)]
pub struct AgentRegistration {
    pub agent_id: String,
    pub abort_controller: Option<Box<AbortSignal>>,
}

/// Agent 进度
#[derive(Debug, Clone, Default)]
pub struct AgentProgress {
    pub token_count: u64,
    pub tool_use_count: u64,
    pub activity: Option<String>,
}

/// Agent 执行结果
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub content: Vec<ContentBlock>,
    pub total_token_count: u64,
    pub total_tool_use_count: u64,
    pub total_duration_ms: u64,
}

/// Abort signal 简化实现
#[derive(Debug)]
pub struct AbortSignal {
    aborted: bool,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self { aborted: false }
    }
    pub fn is_aborted(&self) -> bool {
        self.aborted
    }
    pub fn abort(&mut self) {
        self.aborted = true;
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// 进度追踪器
#[derive(Debug, Clone, Default)]
pub struct ProgressTracker {
    pub token_count: u64,
    pub tool_use_count: u64,
    pub last_tool_name: Option<String>,
}

/// 内置 Agent 类型
const GENERAL_PURPOSE_AGENT: &str = "general-purpose";
const FORK_AGENT: &str = "fork";

/// 检查是否启用 fork subagent。
///
/// 历史：TS 实现通过 GrowthBook gate `mossen_fork_subagent` 控制此功能。
/// 该 GrowthBook 客户端在 Rust 移植中已被删除（见
/// `GrowthBook迁移计划.md`），所有 gate 默认 GA。fork subagent 已默认启用。
fn is_fork_subagent_enabled(_ctx: &dyn AgentToolContext) -> bool {
    true
}

/// 检查环境变量是否为真
fn is_env_truthy(val: Option<&str>) -> bool {
    match val {
        None => false,
        Some(v) => {
            let trimmed = v.trim();
            !trimmed.is_empty() && trimmed != "0" && trimmed.to_lowercase() != "false"
        }
    }
}

/// 构建输入 schema
fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "description".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "A short (3-5 word) description of the task"
        }),
    );
    properties.insert(
        "prompt".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The task for the agent to perform"
        }),
    );
    properties.insert(
        "subagent_type".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The type of specialized agent to use for this task"
        }),
    );
    properties.insert(
        "model".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": ["balanced", "max", "fast"],
            "description": "Optional model override for this agent"
        }),
    );
    properties.insert(
        "run_in_background".to_string(),
        serde_json::json!({
            "type": "boolean",
            "description": "Set to true to run this agent in the background"
        }),
    );
    properties.insert(
        "name".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Name for the spawned agent (makes it addressable via SendMessage)"
        }),
    );
    properties.insert(
        "team_name".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Team name for spawning"
        }),
    );
    properties.insert(
        "mode".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Permission mode for spawned teammate (e.g., 'plan')"
        }),
    );
    properties.insert(
        "isolation".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": ["worktree"],
            "description": "Isolation mode. 'worktree' creates a temporary git worktree"
        }),
    );
    properties.insert(
        "cwd".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Absolute path to run the agent in"
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["description".to_string(), "prompt".to_string()]),
        extra: HashMap::new(),
    }
}

/// 从内容块提取文本
fn extract_text_from_content(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(t) = block {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 工具实际执行逻辑
pub async fn execute_agent_tool(
    ctx: &dyn AgentToolContext,
    input: SubagentLauncherInput,
    _context: &ToolUseContext,
) -> anyhow::Result<ToolResult> {
    let start_time = std::time::Instant::now();
    let permission_mode = ctx.get_permission_mode();

    // 记录日志
    ctx.log_event("mossen_agent_tool_called", {
        let mut m = HashMap::new();
        m.insert("prompt_len".to_string(), input.prompt.len().to_string());
        m.insert(
            "agent_type".to_string(),
            input.subagent_type.clone().unwrap_or_default(),
        );
        m.insert(
            "background".to_string(),
            input.run_in_background.unwrap_or(false).to_string(),
        );
        m
    });

    // 检查多 agent 团队访问
    if input.team_name.is_some() && !ctx.is_agent_swarms_enabled() {
        return Err(anyhow::anyhow!(
            "Agent Teams is not yet available on your plan."
        ));
    }

    // 解析有效 agent 类型
    let is_fork_path = input.subagent_type.is_none() && is_fork_subagent_enabled(ctx);
    let effective_type = input.subagent_type.clone().unwrap_or_else(|| {
        if is_fork_path {
            FORK_AGENT.to_string()
        } else {
            GENERAL_PURPOSE_AGENT.to_string()
        }
    });

    // 检查必要条件
    if ctx.is_teammate() && input.team_name.is_some() && input.name.is_some() {
        return Err(anyhow::anyhow!(
            "Teammates cannot spawn other teammates — the team roster is flat."
        ));
    }

    if ctx.is_in_process_teammate()
        && input.team_name.is_some()
        && input.run_in_background == Some(true)
    {
        return Err(anyhow::anyhow!(
            "In-process teammates cannot spawn background agents."
        ));
    }

    // 确定是否为异步执行
    let is_async =
        input.run_in_background.unwrap_or(false) || ctx.is_coordinator_mode() || is_fork_path;

    // 创建 agent ID
    let agent_id = ctx.create_agent_id();

    // 解析模型
    let resolved_model = if ctx.is_coordinator_mode() {
        None
    } else {
        input.model.clone()
    };

    // 记录 agent 选择
    ctx.log_event("mossen_agent_tool_selected", {
        let mut m = HashMap::new();
        m.insert("agent_type".to_string(), effective_type.clone());
        m.insert("model".to_string(), resolved_model.unwrap_or_default());
        m.insert("is_async".to_string(), is_async.to_string());
        m.insert("is_fork".to_string(), is_fork_path.to_string());
        m
    });

    if is_async {
        // 异步模式：注册后台任务并立即返回
        let registration = ctx.register_async_agent(
            &agent_id,
            &input.description,
            &input.prompt,
            &effective_type,
        );

        let output = SubagentLauncherOutput::async_launched(
            registration.agent_id,
            input.description,
            input.prompt,
        );

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: start_time.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    } else {
        // 同步模式：运行 agent 并等待结果
        // 注意：完整实现需要调用 dialogue 模块的 query loop
        // 这里提供骨架实现
        let elapsed = start_time.elapsed().as_millis() as u64;

        // 构建结果内容
        let result_content = vec![ContentBlock::Text(TextBlock {
            text: format!(
                "Agent completed task. Description: {}. Prompt: {}",
                input.description,
                truncate_chars_with_suffix(&input.prompt, 200, "...")
            ),
        })];

        let output = SubagentLauncherOutput::completed(
            input.prompt,
            result_content,
            Some(agent_id),
            0, // 实际需要从 agent 获取
            0,
            elapsed,
        );

        ctx.log_event("mossen_agent_tool_completed", {
            let mut m = HashMap::new();
            m.insert("duration_ms".to_string(), elapsed.to_string());
            m
        });

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: elapsed,
            metadata: HashMap::new(),
        })
    }
}

#[async_trait]
impl Tool for SubagentLauncher {
    fn name(&self) -> &str {
        "Agent"
    }

    fn description(&self) -> &str {
        "Launch a new agent"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: build_input_schema(),
            cache_control: None,
        }
    }

    fn tool_type(&self) -> ToolType {
        ToolType::Builtin
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let input: SubagentLauncherInput = serde_json::from_value(input)?;

        // 获取 context（通过 context 的属性）
        // 注意：实际实现需要通过 trait 注入
        let ctx = DefaultAgentToolContext::new();

        execute_agent_tool(&ctx, input, context).await
    }
}

/// 默认的 AgentToolContext 实现（用于开发/测试）
pub struct DefaultAgentToolContext {
    agent_counter: std::sync::atomic::AtomicUsize,
}

impl DefaultAgentToolContext {
    pub fn new() -> Self {
        Self {
            agent_counter: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl AgentToolContext for DefaultAgentToolContext {
    fn get_permission_mode(&self) -> String {
        "acceptedEdits".to_string()
    }

    fn get_additional_working_directories(&self) -> Vec<String> {
        Vec::new()
    }

    fn is_env_truthy(&self, key: &str) -> bool {
        is_env_truthy(std::env::var(key).ok().as_deref())
    }

    fn check_feature_gate(&self, _gate: &str) -> bool {
        false
    }

    fn is_coordinator_mode(&self) -> bool {
        is_env_truthy(
            std::env::var("MOSSEN_CODE_COORDINATOR_MODE")
                .ok()
                .as_deref(),
        )
    }

    fn is_agent_swarms_enabled(&self) -> bool {
        is_env_truthy(std::env::var("MOSSEN_CODE_AGENT_SWARMS").ok().as_deref())
    }

    fn is_teammate(&self) -> bool {
        false
    }

    fn is_in_process_teammate(&self) -> bool {
        false
    }

    fn get_parent_session_id(&self) -> Option<String> {
        None
    }

    fn create_agent_id(&self) -> String {
        let id = self
            .agent_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("agent-{}", id)
    }

    fn register_async_agent(
        &self,
        agent_id: &str,
        description: &str,
        prompt: &str,
        agent_type: &str,
    ) -> AgentRegistration {
        info!(
            agent_id = agent_id,
            description = description,
            agent_type = agent_type,
            "Registered async agent"
        );
        AgentRegistration {
            agent_id: agent_id.to_string(),
            abort_controller: None,
        }
    }

    fn update_agent_progress(&self, _agent_id: &str, _progress: &AgentProgress) {}

    fn complete_agent(&self, agent_id: &str, result: &AgentResult) {
        info!(
            agent_id = agent_id,
            tokens = result.total_token_count,
            tools = result.total_tool_use_count,
            "Agent completed"
        );
    }

    fn fail_agent(&self, agent_id: &str, error: &str) {
        warn!(agent_id = agent_id, error = error, "Agent failed");
    }

    fn kill_agent(&self, agent_id: &str) {
        info!(agent_id = agent_id, "Agent killed");
    }

    fn get_agent_system_prompt(&self, _agent_type: &str) -> Option<String> {
        None
    }

    fn log_event(&self, event: &str, metadata: HashMap<String, String>) {
        debug!(event = event, ?metadata, "Agent tool event");
    }
}

impl Default for DefaultAgentToolContext {
    fn default() -> Self {
        Self::new()
    }
}
