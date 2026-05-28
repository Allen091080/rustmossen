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
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;
use tracing::{debug, info, warn};

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ContentBlock, Message, TextBlock};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};
use mossen_utils::hooks_utils::{
    execute_subagent_start_hooks, AggregatedHookResult, TOOL_HOOK_EXECUTION_TIMEOUT_MS,
};
use mossen_utils::string_utils::truncate_chars_with_suffix;

/// 进度阈值（毫秒）—超过此时间显示后台提示
const PROGRESS_THRESHOLD_MS: u64 = 2000;
/// 自动后台化的超时（毫秒）
const AUTO_BACKGROUND_MS: u64 = 120_000;
const AGENT_SUBPROCESS_BIN_ENV: &str = "MOSSEN_AGENT_SUBPROCESS_BIN";
const AGENT_SUBPROCESS_DEPTH_ENV: &str = "MOSSEN_AGENT_SUBPROCESS_DEPTH";
const AGENT_SUBPROCESS_ID_ENV: &str = "MOSSEN_AGENT_SUBPROCESS_ID";
const AGENT_SUBPROCESS_TYPE_ENV: &str = "MOSSEN_AGENT_SUBPROCESS_TYPE";
const MAX_AGENT_SUBPROCESS_DEPTH: u32 = 4;
static AGENT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

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
    pub fn async_launched(
        task_id: String,
        agent_id: String,
        description: String,
        _prompt: String,
    ) -> Self {
        let result = format!(
            "Background agent task registered for `{description}`. This is not a completion signal; call TaskOutput with task_id `{task_id}` and report success only after it returns a ready completed result."
        );
        Self {
            status: "async_launched".to_string(),
            result: Some(result),
            task_id: Some(task_id),
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

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: "error".to_string(),
            result: Some(message.into()),
            task_id: None,
            agent_id: None,
            content: None,
            total_tokens: None,
            total_tool_use_count: None,
            total_duration_ms: None,
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

fn parse_input(input: Value) -> Result<SubagentLauncherInput, String> {
    match input {
        Value::Null => {
            Err("Agent requires a JSON object with `description` and `prompt`; received null."
                .to_string())
        }
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("Agent received invalid input: {error}. Expected object: {{\"description\":\"...\",\"prompt\":\"...\"}}.")
        }),
        other => Err(format!(
            "Agent requires a JSON object with `description` and `prompt`; received {}.",
            other
        )),
    }
}

fn error_result(message: impl Into<String>) -> anyhow::Result<ToolResult> {
    Ok(ToolResult {
        output: serde_json::to_string(&SubagentLauncherOutput::error(message))?,
        is_error: true,
        duration_ms: 0,
        metadata: HashMap::new(),
    })
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

#[derive(Debug, Clone)]
struct SubagentProcessResult {
    output: String,
    exit_code: Option<i32>,
    success: bool,
    duration_ms: u64,
}

fn current_subprocess_depth() -> u32 {
    std::env::var(AGENT_SUBPROCESS_DEPTH_ENV)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0)
}

fn resolve_subagent_executable() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var(AGENT_SUBPROCESS_BIN_ENV) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    std::env::current_exe().map_err(|e| anyhow::anyhow!("failed to resolve current exe: {}", e))
}

fn resolve_agent_cwd(input: &SubagentLauncherInput, context: &ToolUseContext) -> String {
    input
        .cwd
        .as_ref()
        .filter(|cwd| !cwd.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| context.cwd.clone())
}

fn build_subagent_prompt(input: &SubagentLauncherInput, effective_type: &str) -> String {
    format!(
        "You are a Mossen sub-agent launched by a parent session.\n\
         Agent type: {effective_type}\n\
         Task description: {description}\n\n\
         Complete the assigned task directly. Do not launch additional Agent/Task sub-agents \
         unless the prompt explicitly requires nested delegation. Keep the final report concise \
         and factual.\n\n\
         Task:\n{prompt}",
        description = input.description,
        prompt = input.prompt,
    )
}

fn append_subagent_start_contexts(input: &mut SubagentLauncherInput, contexts: Vec<String>) {
    let contexts: Vec<String> = contexts
        .into_iter()
        .filter_map(|context| {
            let trimmed = context.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect();
    if contexts.is_empty() {
        return;
    }
    input
        .prompt
        .push_str("\n\nSubagentStart hook additional context:\n");
    input.prompt.push_str(&contexts.join("\n"));
}

fn collect_hook_additional_contexts(results: &[AggregatedHookResult]) -> Vec<String> {
    results
        .iter()
        .flat_map(|result| {
            result
                .additional_contexts
                .clone()
                .unwrap_or_default()
                .into_iter()
        })
        .filter(|context| !context.trim().is_empty())
        .collect()
}

async fn run_subagent_start_hooks(
    context: &ToolUseContext,
    agent_id: &str,
    agent_type: &str,
) -> Vec<String> {
    let Some(hooks_context) = crate::task_hooks::runtime_hook_context(context) else {
        return Vec::new();
    };
    let results = execute_subagent_start_hooks(
        hooks_context.as_ref(),
        agent_id,
        agent_type,
        None,
        TOOL_HOOK_EXECUTION_TIMEOUT_MS,
    )
    .await;
    collect_hook_additional_contexts(&results)
}

fn format_subagent_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("{stdout}\n\n[stderr]\n{stderr}"),
        (true, true) => String::new(),
    }
}

async fn run_subagent_process(
    input: SubagentLauncherInput,
    effective_type: String,
    cwd: String,
    agent_id: String,
) -> anyhow::Result<SubagentProcessResult> {
    let depth = current_subprocess_depth();
    if depth >= MAX_AGENT_SUBPROCESS_DEPTH {
        anyhow::bail!(
            "Agent subprocess depth limit reached ({}). Refusing nested spawn.",
            MAX_AGENT_SUBPROCESS_DEPTH
        );
    }

    let executable = resolve_subagent_executable()?;
    let prompt = build_subagent_prompt(&input, &effective_type);
    let start = std::time::Instant::now();
    let mut command = Command::new(executable);
    command
        .arg("--oneshot")
        .arg(prompt)
        .arg("--emit")
        .arg("text")
        .arg("--cwd")
        .arg(&cwd)
        .arg("--agent")
        .arg(&effective_type)
        .env(AGENT_SUBPROCESS_DEPTH_ENV, (depth + 1).to_string())
        .env(AGENT_SUBPROCESS_ID_ENV, &agent_id)
        .env(AGENT_SUBPROCESS_TYPE_ENV, &effective_type)
        .current_dir(&cwd);

    if let Some(model) = input
        .model
        .as_ref()
        .filter(|model| !model.trim().is_empty())
    {
        command.arg("--model").arg(model);
    }

    let output = command.output().await?;
    let output_text = format_subagent_output(&output.stdout, &output.stderr);
    Ok(SubagentProcessResult {
        output: output_text,
        exit_code: output.status.code(),
        success: output.status.success(),
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// 工具实际执行逻辑
pub async fn execute_agent_tool(
    ctx: &dyn AgentToolContext,
    mut input: SubagentLauncherInput,
    context: &ToolUseContext,
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
        // 异步模式：注册后台任务、启动子 agent 进程并立即返回
        let registration = ctx.register_async_agent(
            &agent_id,
            &input.description,
            &input.prompt,
            &effective_type,
        );
        let task_id = registration.agent_id.clone();
        let created_hook = crate::task_hooks::task_created(
            context,
            &task_id,
            &input.description,
            Some(input.prompt.as_str()),
        )
        .await;
        if let Some(message) = created_hook.block_message {
            return error_result(message);
        }
        crate::task_store::create_background_agent_task(
            task_id.clone(),
            registration.agent_id.clone(),
            effective_type.clone(),
            input.description.clone(),
            input.prompt.clone(),
            resolve_agent_cwd(&input, context),
        );
        let mut child_input = input.clone();
        let subagent_contexts =
            run_subagent_start_hooks(context, &registration.agent_id, &effective_type).await;
        append_subagent_start_contexts(&mut child_input, subagent_contexts);
        let child_effective_type = effective_type.clone();
        let child_cwd = resolve_agent_cwd(&child_input, context);
        let child_task_id = task_id.clone();
        let child_agent_id = registration.agent_id.clone();
        let child_task_subject = input.description.clone();
        let child_task_description = input.prompt.clone();
        let hook_context_for_task = crate::task_hooks::runtime_hook_context(context);
        let permission_mode_for_task =
            crate::task_hooks::permission_mode(context).map(str::to_string);
        info!(
            task_id = %child_task_id,
            agent_id = %registration.agent_id,
            agent_type = %effective_type,
            cwd = %child_cwd,
            "Background agent subprocess spawned"
        );
        tokio::spawn(async move {
            let result =
                run_subagent_process(child_input, child_effective_type, child_cwd, child_agent_id)
                    .await;
            match result {
                Ok(result) => {
                    let status = if result.success {
                        "completed"
                    } else {
                        "failed"
                    };
                    info!(
                        task_id = %child_task_id,
                        status = status,
                        exit_code = ?result.exit_code,
                        duration_ms = result.duration_ms,
                        "Background agent subprocess finished"
                    );
                    if status == "completed" {
                        let completed_hook = crate::task_hooks::task_completed_with_context(
                            hook_context_for_task.as_deref(),
                            permission_mode_for_task.as_deref(),
                            &child_task_id,
                            &child_task_subject,
                            Some(child_task_description.as_str()),
                        )
                        .await;
                        if let Some(message) = completed_hook.block_message {
                            let _ = crate::task_store::block_task_completion(
                                &child_task_id,
                                result.output,
                                result.exit_code,
                                message,
                            );
                            return;
                        }
                    }
                    let _ = crate::task_store::finish_background_agent_task(
                        &child_task_id,
                        status,
                        result.output,
                        result.exit_code,
                    );
                }
                Err(err) => {
                    warn!(
                        task_id = %child_task_id,
                        error = %err,
                        "Background agent subprocess failed before completion"
                    );
                    let _ = crate::task_store::finish_background_agent_task(
                        &child_task_id,
                        "failed",
                        format!("Agent subprocess failed: {err}"),
                        None,
                    );
                }
            }
        });

        let output = SubagentLauncherOutput::async_launched(
            task_id,
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
        // 同步模式：运行子 agent 进程并等待结果
        let subagent_contexts = run_subagent_start_hooks(context, &agent_id, &effective_type).await;
        append_subagent_start_contexts(&mut input, subagent_contexts);
        let cwd = resolve_agent_cwd(&input, context);
        let child =
            run_subagent_process(input.clone(), effective_type.clone(), cwd, agent_id.clone())
                .await?;
        let result_content = vec![ContentBlock::Text(TextBlock {
            text: child.output.clone(),
        })];

        let output = SubagentLauncherOutput::completed(
            input.prompt,
            result_content,
            Some(agent_id),
            0,
            0,
            child.duration_ms,
        );

        ctx.log_event("mossen_agent_tool_completed", {
            let mut m = HashMap::new();
            m.insert("duration_ms".to_string(), child.duration_ms.to_string());
            m
        });

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: !child.success,
            duration_ms: child.duration_ms,
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
        let input = match parse_input(input) {
            Ok(input) => input,
            Err(message) => return error_result(message),
        };
        if input.description.trim().is_empty() {
            return error_result("Agent requires a non-empty `description` string.");
        }
        if input.prompt.trim().is_empty() {
            return error_result("Agent requires a non-empty `prompt` string.");
        }

        // 获取 context（通过 context 的属性）
        // 注意：实际实现需要通过 trait 注入
        let ctx = DefaultAgentToolContext::new();

        execute_agent_tool(&ctx, input, context).await
    }
}

/// 默认的 AgentToolContext 实现（用于开发/测试）
pub struct DefaultAgentToolContext;

impl DefaultAgentToolContext {
    pub fn new() -> Self {
        Self
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
        is_env_truthy(
            std::env::var("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS")
                .ok()
                .as_deref(),
        ) || is_env_truthy(std::env::var("MOSSEN_CODE_AGENT_SWARMS").ok().as_deref())
            || std::env::args().any(|arg| arg == "--agent-teams")
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
        let counter = AGENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("agent-{}-{}", counter, uuid::Uuid::new_v4())
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

#[cfg(test)]
mod tests {
    use super::{SubagentLauncher, AGENT_SUBPROCESS_BIN_ENV, AGENT_SUBPROCESS_DEPTH_ENV};
    use crate::task_output::ResultEmitter;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use mossen_utils::hooks_utils::{
        register_runtime_hooks_context, unregister_runtime_hooks_context, HookMatcher, HooksContext,
    };
    use serde_json::Value;
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex, OnceLock};

    struct EnvRestore {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvRestore {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }

        fn remove(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("agent env test lock poisoned")
    }

    fn test_context(cwd: &str) -> ToolUseContext {
        ToolUseContext {
            cwd: cwd.to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        }
    }

    struct HookRegistration {
        id: String,
    }

    impl Drop for HookRegistration {
        fn drop(&mut self) {
            unregister_runtime_hooks_context(&self.id);
        }
    }

    fn hooked_context(
        cwd: &std::path::Path,
        hooks: Vec<(&str, String)>,
    ) -> (ToolUseContext, HookRegistration) {
        let mut registered_hooks = HashMap::new();
        for (event, command) in hooks {
            registered_hooks.insert(
                event.to_string(),
                vec![HookMatcher {
                    matcher: None,
                    hooks: vec![serde_json::json!({
                        "type": "command",
                        "command": command,
                        "timeout": 1
                    })],
                    plugin_root: None,
                    plugin_id: None,
                    plugin_name: None,
                    skill_root: None,
                    skill_name: None,
                }],
            );
        }
        let hooks_context = Arc::new(HooksContext {
            session_id: "agent-task-hook-test".to_string(),
            original_cwd: cwd.to_string_lossy().to_string(),
            project_root: cwd.to_string_lossy().to_string(),
            is_non_interactive: true,
            trust_accepted: true,
            hooks_config_snapshot: None,
            registered_hooks: Some(registered_hooks),
            disable_all_hooks: false,
            managed_hooks_only: false,
            main_thread_agent_type: Some("main".to_string()),
            custom_backend_enabled: false,
            simple_mode: false,
            get_transcript_path: Arc::new(|session_id| format!("/tmp/{session_id}.jsonl")),
            get_agent_transcript_path: Arc::new(|agent_id| format!("/tmp/agent-{agent_id}.jsonl")),
            log_debug: Arc::new(|_| {}),
            log_error: Arc::new(|_| {}),
            log_event: Arc::new(|_, _| {}),
            get_settings: Arc::new(|| None),
            get_settings_for_source: Arc::new(|_| None),
            invalidate_session_env_cache: Arc::new(|| {}),
            dynamic_hook_executor: None,
            subprocess_env: std::env::vars().collect(),
            allowed_official_marketplace_names: HashSet::new(),
        });
        let id = register_runtime_hooks_context(hooks_context);
        let mut context = test_context(cwd.to_string_lossy().as_ref());
        context.extra.insert(
            crate::task_hooks::HOOK_CONTEXT_ID_EXTRA_KEY.to_string(),
            serde_json::json!(id.clone()),
        );
        (context, HookRegistration { id })
    }

    fn agent_index(task_id: &str) -> usize {
        task_id
            .split('-')
            .nth(1)
            .expect("agent id index segment")
            .parse()
            .expect("agent id index")
    }

    #[tokio::test]
    async fn agent_null_input_returns_structured_tool_error() {
        let context = test_context(".");

        let result = SubagentLauncher
            .execute(serde_json::Value::Null, &context)
            .await
            .expect("agent launch");
        let output: Value = serde_json::from_str(&result.output).expect("agent json");

        assert!(result.is_error);
        assert_eq!(output["status"], "error");
        assert!(output["result"]
            .as_str()
            .unwrap_or_default()
            .contains("description"));
    }

    #[tokio::test]
    async fn agent_empty_prompt_returns_structured_tool_error() {
        let context = test_context(".");

        let result = SubagentLauncher
            .execute(
                serde_json::json!({
                    "description": "empty prompt",
                    "prompt": ""
                }),
                &context,
            )
            .await
            .expect("agent launch");
        let output: Value = serde_json::from_str(&result.output).expect("agent json");

        assert!(result.is_error);
        assert_eq!(output["status"], "error");
        assert!(output["result"]
            .as_str()
            .unwrap_or_default()
            .contains("prompt"));
    }

    #[cfg(unix)]
    fn write_fake_subagent_bin(dir: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join("fake-mossen-subagent");
        std::fs::write(
            &path,
            "#!/bin/sh\nprintf 'fake-subagent-output\\n'\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\"; done\n",
        )
        .expect("write fake subagent bin");
        let mut perms = std::fs::metadata(&path)
            .expect("fake bin metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod fake bin");
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn agent_background_task_output_returns_subprocess_result() {
        let _env_guard = env_test_lock();
        let _store_guard = crate::task_store::test_store_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let fake_bin = write_fake_subagent_bin(temp.path());
        let _bin_restore = EnvRestore::set(
            AGENT_SUBPROCESS_BIN_ENV,
            fake_bin.to_string_lossy().as_ref(),
        );
        let _depth_restore = EnvRestore::remove(AGENT_SUBPROCESS_DEPTH_ENV);
        let context = test_context(temp.path().to_string_lossy().as_ref());

        let launch = SubagentLauncher
            .execute(
                serde_json::json!({
                    "description": "scan marker",
                    "prompt": "find subagent-smoke-marker",
                    "subagent_type": "general-purpose",
                    "run_in_background": true
                }),
                &context,
            )
            .await
            .expect("agent launch");
        assert!(!launch.is_error);
        let launch_output: Value = serde_json::from_str(&launch.output).expect("agent json");
        assert_eq!(launch_output["status"], "async_launched");
        assert!(launch_output["result"]
            .as_str()
            .unwrap_or_default()
            .contains("TaskOutput"));
        let task_id = launch_output["task_id"]
            .as_str()
            .expect("task_id")
            .to_string();

        let task_output = ResultEmitter
            .execute(
                serde_json::json!({
                    "task_id": task_id,
                    "block": true,
                    "timeout": 10_000,
                }),
                &context,
            )
            .await
            .expect("TaskOutput");
        let output: Value = serde_json::from_str(&task_output.output).expect("TaskOutput json");
        assert_eq!(output["retrieval_status"], "ready");
        assert_eq!(output["task"]["task_type"], "background_agent");
        assert_eq!(output["task"]["status"], "completed");
        assert!(output["task"]["output"]
            .as_str()
            .unwrap_or_default()
            .contains("subagent-smoke-marker"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn agent_background_task_lifecycle_hooks_fire() {
        let _env_guard = env_test_lock();
        let _store_guard = crate::task_store::test_store_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let fake_bin = write_fake_subagent_bin(temp.path());
        let _bin_restore = EnvRestore::set(
            AGENT_SUBPROCESS_BIN_ENV,
            fake_bin.to_string_lossy().as_ref(),
        );
        let _depth_restore = EnvRestore::remove(AGENT_SUBPROCESS_DEPTH_ENV);
        let created_marker = temp.path().join("agent_task_created_marker");
        let subagent_start_marker = temp.path().join("agent_subagent_start_marker");
        let completed_marker = temp.path().join("agent_task_completed_marker");
        let created_arg = created_marker.to_string_lossy().replace('\'', "'\\''");
        let subagent_start_arg = subagent_start_marker
            .to_string_lossy()
            .replace('\'', "'\\''");
        let completed_arg = completed_marker.to_string_lossy().replace('\'', "'\\''");
        let subagent_start_output = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "SubagentStart",
                "additionalContext": "subagent-start-extra-context"
            }
        })
        .to_string()
        .replace('\'', "'\\''");
        let (context, _hook_registration) = hooked_context(
            temp.path(),
            vec![
                (
                    "TaskCreated",
                    format!("printf agent-task-created > '{created_arg}'"),
                ),
                (
                    "SubagentStart",
                    format!(
                        "printf agent-subagent-start > '{subagent_start_arg}'; printf '%s' '{subagent_start_output}'"
                    ),
                ),
                (
                    "TaskCompleted",
                    format!("printf agent-task-completed > '{completed_arg}'"),
                ),
            ],
        );

        let launch = SubagentLauncher
            .execute(
                serde_json::json!({
                    "description": "hooked agent",
                    "prompt": "find agent-hook-marker",
                    "subagent_type": "general-purpose",
                    "run_in_background": true
                }),
                &context,
            )
            .await
            .expect("agent launch");
        assert!(!launch.is_error, "{}", launch.output);
        let launch_output: Value = serde_json::from_str(&launch.output).expect("agent json");
        let task_id = launch_output["task_id"]
            .as_str()
            .expect("task_id")
            .to_string();

        let task_output = ResultEmitter
            .execute(
                serde_json::json!({
                    "task_id": task_id,
                    "block": true,
                    "timeout": 10_000,
                }),
                &context,
            )
            .await
            .expect("TaskOutput");
        let output: Value = serde_json::from_str(&task_output.output).expect("TaskOutput json");
        assert_eq!(output["retrieval_status"], "ready");
        assert_eq!(output["task"]["status"], "completed");
        assert!(output["task"]["output"]
            .as_str()
            .unwrap_or_default()
            .contains("subagent-start-extra-context"));
        assert_eq!(
            tokio::fs::read_to_string(created_marker)
                .await
                .expect("TaskCreated marker"),
            "agent-task-created"
        );
        assert_eq!(
            tokio::fs::read_to_string(subagent_start_marker)
                .await
                .expect("SubagentStart marker"),
            "agent-subagent-start"
        );
        assert_eq!(
            tokio::fs::read_to_string(completed_marker)
                .await
                .expect("TaskCompleted marker"),
            "agent-task-completed"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn parallel_agent_launches_get_distinct_task_output_records() {
        let _env_guard = env_test_lock();
        let _store_guard = crate::task_store::test_store_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let fake_bin = write_fake_subagent_bin(temp.path());
        let _bin_restore = EnvRestore::set(
            AGENT_SUBPROCESS_BIN_ENV,
            fake_bin.to_string_lossy().as_ref(),
        );
        let _depth_restore = EnvRestore::remove(AGENT_SUBPROCESS_DEPTH_ENV);
        let context = test_context(temp.path().to_string_lossy().as_ref());

        let mut task_ids = Vec::new();
        for marker in ["first-parallel-marker", "second-parallel-marker"] {
            let launch = SubagentLauncher
                .execute(
                    serde_json::json!({
                        "description": marker,
                        "prompt": format!("find {marker}"),
                        "subagent_type": "general-purpose",
                        "run_in_background": true
                    }),
                    &context,
                )
                .await
                .expect("agent launch");
            assert!(!launch.is_error, "{}", launch.output);
            let launch_output: Value = serde_json::from_str(&launch.output).expect("agent json");
            let task_id = launch_output["task_id"]
                .as_str()
                .expect("task_id")
                .to_string();
            task_ids.push(task_id);
        }

        assert_ne!(
            task_ids[0], task_ids[1],
            "parallel agents must not share task ids"
        );
        assert!(
            agent_index(&task_ids[1]) > agent_index(&task_ids[0]),
            "parallel agents should get visible monotonic prefixes: {:?}",
            task_ids
        );
        assert_eq!(crate::task_store::list_tasks().len(), 2);

        for task_id in task_ids {
            let task_output = ResultEmitter
                .execute(
                    serde_json::json!({
                        "task_id": task_id,
                        "block": true,
                        "timeout": 10_000,
                    }),
                    &context,
                )
                .await
                .expect("TaskOutput");
            let output: Value = serde_json::from_str(&task_output.output).expect("TaskOutput json");
            assert_eq!(output["retrieval_status"], "ready");
            assert_eq!(output["task"]["task_type"], "background_agent");
            assert_eq!(output["task"]["status"], "completed");
            assert!(output["task"]["output"]
                .as_str()
                .unwrap_or_default()
                .contains("fake-subagent-output"));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn concurrent_agent_launches_keep_short_task_ids_resolvable() {
        let _env_guard = env_test_lock();
        let _store_guard = crate::task_store::test_store_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let fake_bin = write_fake_subagent_bin(temp.path());
        let _bin_restore = EnvRestore::set(
            AGENT_SUBPROCESS_BIN_ENV,
            fake_bin.to_string_lossy().as_ref(),
        );
        let _depth_restore = EnvRestore::remove(AGENT_SUBPROCESS_DEPTH_ENV);
        let context = test_context(temp.path().to_string_lossy().as_ref());
        let launcher = SubagentLauncher;

        let first = launcher.execute(
            serde_json::json!({
                "description": "concurrent first",
                "prompt": "find concurrent-first-marker",
                "subagent_type": "general-purpose",
                "run_in_background": true
            }),
            &context,
        );
        let second = launcher.execute(
            serde_json::json!({
                "description": "concurrent second",
                "prompt": "find concurrent-second-marker",
                "subagent_type": "general-purpose",
                "run_in_background": true
            }),
            &context,
        );
        let third = launcher.execute(
            serde_json::json!({
                "description": "concurrent third",
                "prompt": "find concurrent-third-marker",
                "subagent_type": "general-purpose",
                "run_in_background": true
            }),
            &context,
        );

        let launches = tokio::join!(first, second, third);
        let mut task_ids = Vec::new();
        for launch in [launches.0, launches.1, launches.2] {
            let launch = launch.expect("agent launch");
            assert!(!launch.is_error, "{}", launch.output);
            let launch_output: Value = serde_json::from_str(&launch.output).expect("agent json");
            assert_eq!(launch_output["status"], "async_launched");
            task_ids.push(
                launch_output["task_id"]
                    .as_str()
                    .expect("task_id")
                    .to_string(),
            );
        }

        let unique_full_ids: HashSet<_> = task_ids.iter().cloned().collect();
        assert_eq!(
            unique_full_ids.len(),
            task_ids.len(),
            "concurrent agents must not share full task ids: {:?}",
            task_ids
        );

        let short_ids: Vec<String> = task_ids
            .iter()
            .map(|id| id.split('-').take(2).collect::<Vec<_>>().join("-"))
            .collect();
        let unique_short_ids: HashSet<_> = short_ids.iter().cloned().collect();
        assert_eq!(
            unique_short_ids.len(),
            short_ids.len(),
            "visible task id prefixes must stay unique for TaskOutput recovery: {:?}",
            task_ids
        );

        for short_id in short_ids {
            let task_output = ResultEmitter
                .execute(
                    serde_json::json!({
                        "task_id": short_id,
                        "block": true,
                        "timeout": 10_000,
                    }),
                    &context,
                )
                .await
                .expect("TaskOutput");
            let output: Value = serde_json::from_str(&task_output.output).expect("TaskOutput json");
            assert_eq!(output["retrieval_status"], "ready", "{output}");
            assert_eq!(output["task"]["task_type"], "background_agent");
            assert_eq!(output["task"]["status"], "completed");
            assert!(output["task"]["output"]
                .as_str()
                .unwrap_or_default()
                .contains("fake-subagent-output"));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_registry_executes_agent_then_taskoutput() {
        let _env_guard = env_test_lock();
        let _store_guard = crate::task_store::test_store_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let fake_bin = write_fake_subagent_bin(temp.path());
        let _bin_restore = EnvRestore::set(
            AGENT_SUBPROCESS_BIN_ENV,
            fake_bin.to_string_lossy().as_ref(),
        );
        let _depth_restore = EnvRestore::remove(AGENT_SUBPROCESS_DEPTH_ENV);
        let context = test_context(temp.path().to_string_lossy().as_ref());
        let mut registry = mossen_agent::tool_registry::ToolRegistry::new();
        registry.register_all(crate::all_tools_for_runtime(
            crate::ToolRuntimeOptions::default(),
        ));

        let launch = registry
            .execute(
                "Agent",
                serde_json::json!({
                    "description": "registry chain",
                    "prompt": "find registry-chain-marker",
                    "subagent_type": "general-purpose",
                    "run_in_background": true
                }),
                &context,
            )
            .await
            .expect("Agent registry launch");
        assert!(!launch.is_error, "{}", launch.output);
        let launch_output: Value = serde_json::from_str(&launch.output).expect("Agent json");
        assert_eq!(launch_output["status"], "async_launched");
        assert!(launch_output["result"]
            .as_str()
            .unwrap_or_default()
            .contains("TaskOutput"));
        let task_id = launch_output["task_id"].as_str().expect("task_id");

        let task_output = registry
            .execute(
                "TaskOutput",
                serde_json::json!({
                    "task_id": task_id,
                    "block": true,
                    "timeout": 10_000
                }),
                &context,
            )
            .await
            .expect("TaskOutput registry lookup");
        let output: Value = serde_json::from_str(&task_output.output).expect("TaskOutput json");
        assert_eq!(output["retrieval_status"], "ready");
        assert_eq!(output["task"]["task_type"], "background_agent");
        assert_eq!(output["task"]["status"], "completed");
        assert!(output["task"]["output"]
            .as_str()
            .unwrap_or_default()
            .contains("registry-chain-marker"));
    }

    #[test]
    fn agent_schema_does_not_advertise_unwired_team_routing() {
        let definition = SubagentLauncher.definition();
        let rendered = serde_json::to_string(&definition).expect("agent definition json");
        let value: serde_json::Value =
            serde_json::from_str(&rendered).expect("agent definition value");
        let properties = value["input_schema"]["properties"]
            .as_object()
            .expect("agent schema properties");

        assert!(!rendered.contains("SendMessage"), "{rendered}");
        assert!(!properties.contains_key("team_name"), "{rendered}");
        assert!(!properties.contains_key("mode"), "{rendered}");
        assert!(!properties.contains_key("name"), "{rendered}");
    }
}
