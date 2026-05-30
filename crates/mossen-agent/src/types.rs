//! # types — Agent 核心内部类型
//!
//! 定义 Agent Loop 状态机、对话规格、流式事件等核心类型。
//! 对应 TS query.ts 中的 State/Continue/Terminal 等概念。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::goal::ThreadGoal;

pub use mossen_types::{
    AssistantMessage, ContentBlock, Message, Role, ToolDefinition, ToolUseContext,
    ToolUseSummaryMessage, UserMessage,
};

// ---------------------------------------------------------------------------
// 权限模式
// ---------------------------------------------------------------------------

/// Session-scoped permission mode. Mirrors the public SDK string values while
/// keeping the engine side strongly typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    BypassPermissions,
    Plan,
    DontAsk,
    Auto,
    Yolo,
}

impl PermissionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::BypassPermissions => "bypassPermissions",
            Self::Plan => "plan",
            Self::DontAsk => "dontAsk",
            Self::Auto => "auto",
            Self::Yolo => "yolo",
        }
    }

    pub fn parse(raw: impl AsRef<str>) -> Self {
        let normalized = raw
            .as_ref()
            .trim()
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | ' ' | '\t'))
            .flat_map(char::to_lowercase)
            .collect::<String>();

        match normalized.as_str() {
            "acceptedits" => Self::AcceptEdits,
            "bypasspermissions" | "bypass" | "fullauto" => Self::BypassPermissions,
            "plan" | "readonly" | "read" => Self::Plan,
            "dontask" | "dontprompt" | "neverask" => Self::DontAsk,
            "auto" => Self::Auto,
            "yolo" => Self::Yolo,
            "default" | "supervised" | "suggest" | "ask" | "" => Self::Default,
            _ => Self::Default,
        }
    }
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::Default
    }
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// 对话规格（QueryParams → DialogueSpec）
// ---------------------------------------------------------------------------

/// 对话规格——描述一次 Agent 对话的不可变配置。
///
/// 对应 TS `QueryParams`。
#[derive(Debug, Clone)]
pub struct DialogueSpec {
    /// 系统提示。
    pub system_prompt: Vec<SystemBlock>,
    /// 初始消息列表。
    pub messages: Vec<Message>,
    /// 可用工具列表。
    pub tools: Vec<ToolDefinition>,
    /// 工具使用上下文。
    pub tool_use_context: ToolUseContext,
    /// 模型 ID。
    pub model: String,
    /// 是否允许思考模式。
    pub thinking_enabled: bool,
    /// 思考预算 token 数。
    pub thinking_budget: Option<u32>,
    /// 最大输出 token 数。
    pub max_output_tokens: Option<u32>,
    /// 最大轮次。
    pub max_turns: Option<u32>,
    /// 来源标签。
    pub origin_tag: OriginTag,
    /// 是否为快速模式。
    pub fast_mode: Option<bool>,
    /// 额外请求体。
    pub extra_body: HashMap<String, serde_json::Value>,
    /// 取消令牌。
    pub cancel: CancellationToken,
    /// 链路追踪上下文。
    pub chain_trace: Option<ChainTrace>,
    /// 是否禁止 stop hooks。
    pub skip_stop_hooks: bool,
    /// effort 级别。
    pub effort: Option<EffortLevel>,
    /// 自动模式。
    pub auto_mode: bool,
    /// 预付权限列表。
    pub pre_approved_permissions: Vec<String>,
    /// Session permission mode applied before the interactive gate. Modes like
    /// `bypassPermissions`, `plan`, and `dontAsk` can decide a tool-use without
    /// opening a UI prompt; `default` falls through to `permission_gate`.
    pub permission_mode: PermissionMode,
    /// Permission gate — consulted before each tool invocation. The engine
    /// calls `check()` with the tool name, id, and input JSON; the gate
    /// returns `Allow`, `AllowAlways`, or `Deny`. `AllowAllGate` is the
    /// default (current open behaviour); an `InteractiveGate` (declared in
    /// this module) drives a UI modal for genuine supervised mode.
    ///
    /// Stored as `Arc<dyn PermissionGate>` so the same gate instance can be
    /// shared across nested dialogue turns and child agent tasks without
    /// re-creating its UI channel.
    pub permission_gate: std::sync::Arc<dyn PermissionGate>,
    /// Runtime hook context loaded from settings/plugin/session state. This is
    /// optional so SDK/test callers can run without a CLI bootstrap context.
    pub hook_context: Option<std::sync::Arc<mossen_utils::hooks_utils::HooksContext>>,
}

/// User decision on a tool-use permission request. `AllowAlways` is
/// session-scoped (the gate may persist it as an `AllowAllow` rule but the
/// engine itself just sees it as `Allow`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    AllowAlways,
    AllowWithUpdatedInput { updated_input: serde_json::Value },
    Deny,
}

impl PermissionDecision {
    /// Convenience predicate: `Allow` or `AllowAlways`.
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            PermissionDecision::Allow
                | PermissionDecision::AllowAlways
                | PermissionDecision::AllowWithUpdatedInput { .. }
        )
    }

    pub fn updated_input(&self) -> Option<&serde_json::Value> {
        match self {
            PermissionDecision::AllowWithUpdatedInput { updated_input } => Some(updated_input),
            _ => None,
        }
    }
}

/// One outstanding permission request — emitted by `InteractiveGate` to
/// whichever UI surface is consuming the channel. The UI fills in the
/// `responder` oneshot once the user clicks Allow / Deny / Allow Always.
pub struct PermissionRequest {
    pub tool_id: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub responder: tokio::sync::oneshot::Sender<PermissionDecision>,
}

/// Permission check interface — pluggable so the engine can run unattended
/// (AllowAllGate), prompt the TUI (InteractiveGate), or in tests pre-script
/// a sequence of decisions (a custom impl).
///
/// `Debug` is a super-trait so `Arc<dyn PermissionGate>` can be embedded
/// in `Debug`-deriving config structs (DialogueSpec, OrchestratorConfig).
#[async_trait::async_trait]
pub trait PermissionGate: Send + Sync + std::fmt::Debug {
    async fn check(
        &self,
        tool_name: &str,
        tool_id: &str,
        input: &serde_json::Value,
    ) -> PermissionDecision;
}

/// Always-allow gate — equivalent to running with `--access-policy
/// unrestricted`. Used when the engine has no UI to prompt, in tests, and as
/// the safe default until a real gate is plumbed in.
#[derive(Debug)]
pub struct AllowAllGate;

#[async_trait::async_trait]
impl PermissionGate for AllowAllGate {
    async fn check(
        &self,
        _tool_name: &str,
        _tool_id: &str,
        _input: &serde_json::Value,
    ) -> PermissionDecision {
        PermissionDecision::Allow
    }
}

/// Interactive gate — forwards each tool-use to a channel the UI drains.
/// Once a `PermissionRequest` is sent, the gate blocks on `responder` until
/// the UI replies. `pre_approved` keeps session-scoped rule keys that bypass
/// the channel after the user clicks "Allow Always".
pub struct InteractiveGate {
    request_tx: tokio::sync::mpsc::Sender<PermissionRequest>,
    pre_approved: tokio::sync::RwLock<std::collections::HashSet<String>>,
}

impl std::fmt::Debug for InteractiveGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InteractiveGate")
            .field("request_tx", &"<mpsc::Sender>")
            .field("pre_approved", &"<RwLock<HashSet>>")
            .finish()
    }
}

impl InteractiveGate {
    pub fn new(request_tx: tokio::sync::mpsc::Sender<PermissionRequest>) -> Self {
        Self {
            request_tx,
            pre_approved: tokio::sync::RwLock::new(std::collections::HashSet::new()),
        }
    }
}

#[async_trait::async_trait]
impl PermissionGate for InteractiveGate {
    async fn check(
        &self,
        tool_name: &str,
        tool_id: &str,
        input: &serde_json::Value,
    ) -> PermissionDecision {
        let session_rule_key = interactive_gate_session_rule_key(tool_name, input);

        // Fast-path: this session rule was previously "Allow Always"-ed.
        if self.pre_approved.read().await.contains(&session_rule_key) {
            return PermissionDecision::Allow;
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        let request = PermissionRequest {
            tool_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
            input: input.clone(),
            responder: tx,
        };

        // If the UI channel is closed (TUI exited mid-turn) we treat it as
        // Deny — safer than guessing.
        if self.request_tx.send(request).await.is_err() {
            return PermissionDecision::Deny;
        }

        match rx.await {
            Ok(decision) => {
                if matches!(&decision, PermissionDecision::AllowAlways) {
                    self.pre_approved.write().await.insert(session_rule_key);
                }
                decision
            }
            Err(_) => PermissionDecision::Deny,
        }
    }
}

fn interactive_gate_session_rule_key(tool_name: &str, input: &serde_json::Value) -> String {
    let clean_tool_name = interactive_gate_rule_text(tool_name);
    if let Some(command) = interactive_gate_shell_command_rule(tool_name, input) {
        return format!("command:{clean_tool_name}:{command}");
    }
    format!("tool:{clean_tool_name}")
}

fn interactive_gate_shell_command_rule(
    tool_name: &str,
    input: &serde_json::Value,
) -> Option<String> {
    if !matches!(tool_name, "Bash" | "PowerShell" | "Execute") {
        return None;
    }
    input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .map(interactive_gate_rule_text)
        .filter(|command| !command.is_empty())
}

fn interactive_gate_rule_text(raw: &str) -> String {
    raw.chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

/// 系统提示块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemBlock {
    /// 文本内容。
    pub text: String,
    /// 缓存控制。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlSpec>,
}

/// 缓存控制规格。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControlSpec {
    /// 类型。
    #[serde(rename = "type")]
    pub control_type: String,
    /// TTL。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
    /// 作用域。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// 来源标签——标识对话来源。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginTag {
    /// 交互式 REPL。
    Repl,
    /// SDK 调用。
    Sdk,
    /// 自定义后端。
    CustomBackend,
    /// Agent 子任务。
    AgentTask,
    /// 后台任务。
    Background,
    /// 流水线。
    Pipeline,
}

/// Effort 级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffortLevel {
    Low,
    Medium,
    High,
    Max,
}

impl EffortLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }
}

/// 链路追踪上下文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainTrace {
    /// 追踪 ID。
    pub trace_id: String,
    /// 父 span ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Agent Loop 状态（State → TurnLedger）
// ---------------------------------------------------------------------------

/// Agent Loop 每次迭代携带的可变状态。
///
/// 对应 TS `State`（query.ts:236-249）。
pub struct TurnLedger {
    /// 当前累积消息。
    pub messages: Vec<Message>,
    /// 工具使用上下文。
    pub tool_use_context: ToolUseContext,
    /// 自动压缩追踪状态。
    pub auto_compact_tracking: Option<AutoCompactTracking>,
    /// max_output_tokens 恢复尝试计数。
    pub max_output_tokens_recovery_count: u32,
    /// 是否已尝试响应式压缩。
    pub has_attempted_reactive_compact: bool,
    /// max_output_tokens 覆盖值。
    pub max_output_tokens_override: Option<u32>,
    /// 待处理的工具使用摘要。
    pub pending_tool_use_summary: Option<JoinHandle<Option<ToolUseSummaryMessage>>>,
    /// stop hook 活跃状态。
    pub stop_hook_active: Option<bool>,
    /// 当前轮次计数。
    pub turn_count: u32,
    /// 上一次迭代的 Continue 原因。
    pub transition: Option<ContinueReason>,
}

impl TurnLedger {
    /// 创建初始状态。
    pub fn new(spec: &DialogueSpec) -> Self {
        Self {
            messages: spec.messages.clone(),
            tool_use_context: spec.tool_use_context.clone(),
            auto_compact_tracking: None,
            max_output_tokens_recovery_count: 0,
            has_attempted_reactive_compact: false,
            max_output_tokens_override: None,
            pending_tool_use_summary: None,
            stop_hook_active: None,
            turn_count: 0,
            transition: None,
        }
    }

    /// 推进到下一轮。
    pub fn advance_turn(&mut self, reason: ContinueReason) {
        self.turn_count += 1;
        self.transition = Some(reason);
    }
}

/// 自动压缩追踪状态。
#[derive(Debug, Clone, Default)]
pub struct AutoCompactTracking {
    /// 连续失败次数。
    pub consecutive_failures: u32,
    /// 上次压缩的 token 计数。
    pub last_compact_token_count: Option<u64>,
    /// 上次压缩时间。
    pub last_compact_time: Option<chrono::DateTime<chrono::Utc>>,
}

// ---------------------------------------------------------------------------
// 状态转移枚举
// ---------------------------------------------------------------------------

/// 8 个 Continue 站点——描述 Agent Loop 继续下一轮的原因。
///
/// 对应 TS `Continue` tagged union。
#[derive(Debug, Clone)]
pub enum ContinueReason {
    /// 站点 1：上下文坍缩排空后重试。
    CollapseDrainRetry { committed: usize },
    /// 站点 2：响应式压缩后重试。
    ReactiveCompactRetry,
    /// 站点 3：max_output_tokens 升档（到 64K）。
    MaxOutputTokensEscalate,
    /// 站点 4：max_output_tokens 多轮恢复（≤3 次）。
    MaxOutputTokensRecovery { attempt: u32 },
    /// 站点 5：action promise 恢复。
    ActionPromiseRecovery,
    /// 工具结果后的模型空响应恢复。
    EmptyResponseRecovery { attempt: u32 },
    /// 站点 6：stop hook 阻塞后重试。
    StopHookBlocking,
    /// 站点 7：token budget 自动续行。
    TokenBudgetContinuation,
    /// Active thread goal asks the model to keep working after an idle stop.
    GoalContinuation,
    /// 站点 8：正常下一轮工具调用。
    NextTurn,
}

/// 终止原因——描述 Agent Loop 结束的原因。
///
/// 对应 TS `Terminal`。
#[derive(Debug)]
pub enum TerminalReason {
    /// 正常完成。
    Completed,
    /// 阻塞限制。
    BlockingLimit,
    /// 图像错误。
    ImageError,
    /// 提示过长。
    PromptTooLong,
    /// 模型错误。
    ModelError { error: anyhow::Error },
    /// 流式中止。
    AbortedStreaming,
    /// 工具中止。
    AbortedTools,
    /// 达到最大轮次。
    MaxTurns { turn_count: u32 },
    /// 钩子停止。
    HookStopped,
    /// Stop hook 阻止。
    StopHookPrevented,
    /// 需要重试（fallback 触发但无 handler，用当前 model 再试）。
    Retry,
}

// ---------------------------------------------------------------------------
// 查询环境快照（buildQueryConfig → snapshot_turn_env）
// ---------------------------------------------------------------------------

/// 每轮开始时采样的不可变环境快照。
#[derive(Debug, Clone)]
pub struct TurnEnvironment {
    /// 模型 ID。
    pub model: String,
    /// 上下文窗口大小。
    pub context_window: u64,
    /// 最大输出 token 数。
    pub max_output_tokens: u32,
    /// 思考配置。
    pub thinking_config: Option<ThinkingConfig>,
    /// 快速模式。
    pub fast_mode: bool,
    /// effort 级别。
    pub effort: Option<EffortLevel>,
    /// 自动模式。
    pub auto_mode: bool,
    /// Beta 特性列表。
    pub betas: Vec<String>,
}

/// 思考模式配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    /// 是否启用。
    pub enabled: bool,
    /// 预算 token 数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

// ---------------------------------------------------------------------------
// SDK 消息事件（SdkMessage）
// ---------------------------------------------------------------------------

/// 会话编排器 yield 的消息类型。
///
/// 对应 TS `SDKMessage` 联合类型。每个变体都包含一个可选的 `task_id`
/// 字段：`None` = 主 agent 消息，`Some(id)` = 子 agent 消息。
/// 该字段使用 serde default 确保前后兼容 —— 旧版序列化的消息不含
/// `task_id` 时自动解析为 `None`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SdkMessage {
    /// 系统初始化信息。
    #[serde(rename = "system_init")]
    SystemInit {
        session_id: String,
        model: String,
        tools: Vec<String>,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// 助手消息。
    #[serde(rename = "assistant")]
    Assistant {
        message: AssistantMessage,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ApiUsage>,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// 用户消息回放。
    #[serde(rename = "user")]
    User {
        message: UserMessage,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// 流式事件。
    #[serde(rename = "stream_event")]
    StreamEvent {
        event: StreamEventData,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// 结果消息。
    #[serde(rename = "result")]
    Result {
        terminal: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ApiUsage>,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// 工具使用摘要。
    #[serde(rename = "tool_use_summary")]
    ToolUseSummary {
        tool_name: String,
        /// Stable id of the `ToolUseBlock` this result summarizes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
        /// Truncated preview (≤ ~600 chars) shown in the collapsed
        /// ToolResult row.
        summary: String,
        /// Full untruncated tool output, surfaced when the user expands
        /// the row via the right-arrow / Enter key. `None` when the
        /// preview already is the full content (no truncation happened).
        #[serde(skip_serializing_if = "Option::is_none")]
        full_content: Option<String>,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// 压缩边界。
    #[serde(rename = "compact_boundary")]
    CompactBoundary {
        before_token_count: u64,
        after_token_count: u64,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// Compact control request status.
    #[serde(rename = "compact_request_status")]
    CompactRequestStatus {
        request_id: String,
        status: CompactRequestStatus,
        dry_run: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_token_count: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_token_count: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_count_before: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_count_after: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compacted_message_count: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// Conversation clear boundary.
    #[serde(rename = "conversation_cleared")]
    ConversationCleared {
        message_count_before: u64,
        message_count_after: u64,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// Clear control request status.
    #[serde(rename = "clear_request_status")]
    ClearRequestStatus {
        request_id: String,
        status: ClearRequestStatus,
        dry_run: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_count_before: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_count_after: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// API 重试通知。
    #[serde(rename = "api_retry")]
    ApiRetry {
        error: String,
        attempt: u32,
        max_retries: u32,
        retry_in_ms: u64,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// Thread goal state changed.
    #[serde(rename = "thread_goal_updated")]
    ThreadGoalUpdated {
        thread_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        goal: ThreadGoal,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    /// Thread goal state was cleared.
    #[serde(rename = "thread_goal_cleared")]
    ThreadGoalCleared {
        thread_id: String,
        /// 消息来源 task id。None = 主 agent。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
}

impl SdkMessage {
    /// Extract the optional task_id from any variant.
    /// None = main agent, Some(id) = sub-agent message.
    pub fn task_id(&self) -> Option<&str> {
        match self {
            SdkMessage::SystemInit { task_id, .. } => task_id.as_deref(),
            SdkMessage::Assistant { task_id, .. } => task_id.as_deref(),
            SdkMessage::User { task_id, .. } => task_id.as_deref(),
            SdkMessage::StreamEvent { task_id, .. } => task_id.as_deref(),
            SdkMessage::Result { task_id, .. } => task_id.as_deref(),
            SdkMessage::ToolUseSummary { task_id, .. } => task_id.as_deref(),
            SdkMessage::CompactBoundary { task_id, .. } => task_id.as_deref(),
            SdkMessage::CompactRequestStatus { task_id, .. } => task_id.as_deref(),
            SdkMessage::ConversationCleared { task_id, .. } => task_id.as_deref(),
            SdkMessage::ClearRequestStatus { task_id, .. } => task_id.as_deref(),
            SdkMessage::ApiRetry { task_id, .. } => task_id.as_deref(),
            SdkMessage::ThreadGoalUpdated { task_id, .. } => task_id.as_deref(),
            SdkMessage::ThreadGoalCleared { task_id, .. } => task_id.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactRequestStatus {
    TimedOut,
    DryRun,
    Completed,
    Skipped,
    Failed,
}

impl CompactRequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CompactRequestStatus::TimedOut => "timed_out",
            CompactRequestStatus::DryRun => "dry_run",
            CompactRequestStatus::Completed => "completed",
            CompactRequestStatus::Skipped => "skipped",
            CompactRequestStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearRequestStatus {
    TimedOut,
    DryRun,
    Completed,
}

impl ClearRequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ClearRequestStatus::TimedOut => "timed_out",
            ClearRequestStatus::DryRun => "dry_run",
            ClearRequestStatus::Completed => "completed",
        }
    }
}

/// 流式事件数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum StreamEventData {
    /// 内容块开始。
    #[serde(rename = "content_block_start")]
    ContentBlockStart { index: usize },
    /// 内容块增量。
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: ContentDelta },
    /// 内容块停止。
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    /// 消息开始。
    #[serde(rename = "message_start")]
    MessageStart,
    /// 消息增量（usage 更新）。
    #[serde(rename = "message_delta")]
    MessageDelta {
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ApiUsage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason: Option<String>,
    },
    /// 消息停止。
    #[serde(rename = "message_stop")]
    MessageStop,
}

/// 内容增量。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentDelta {
    /// 文本增量。
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    /// 思考增量。
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    /// 工具输入 JSON 增量。
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

// ---------------------------------------------------------------------------
// API 用量
// ---------------------------------------------------------------------------

/// API 调用用量统计。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiUsage {
    /// 输入 token 数。
    pub input_tokens: u64,
    /// 输出 token 数。
    pub output_tokens: u64,
    /// 缓存读取 token 数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
    /// 缓存创建 token 数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
}

/// 不可空用量（累加后使用）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NonNullableUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

impl NonNullableUsage {
    pub fn accumulate(&mut self, usage: &ApiUsage) {
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        self.cache_read_input_tokens += usage.cache_read_input_tokens.unwrap_or(0);
        self.cache_creation_input_tokens += usage.cache_creation_input_tokens.unwrap_or(0);
    }
}

// ---------------------------------------------------------------------------
// API 请求参数
// ---------------------------------------------------------------------------

/// 流式 API 请求参数。
#[derive(Debug, Clone, Serialize)]
pub struct StreamRequestParams {
    /// 模型。
    pub model: String,
    /// 最大输出 token 数。
    pub max_tokens: u32,
    /// 消息列表。
    pub messages: Vec<MessageParam>,
    /// 系统提示。
    pub system: Vec<SystemBlock>,
    /// 工具列表。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    /// 思考配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// 工具选择策略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// 是否启用流式。
    pub stream: bool,
    /// 元数据。
    pub metadata: ApiMetadata,
    /// 额外请求体。
    #[serde(flatten)]
    pub extra_body: HashMap<String, serde_json::Value>,
    /// Provider-neutral effort level. The API client maps this to the
    /// selected backend protocol instead of leaking one provider's field
    /// names into every request body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<EffortLevel>,
}

/// 消息参数（API 请求中使用的格式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageParam {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

impl From<&Message> for MessageParam {
    fn from(msg: &Message) -> Self {
        Self {
            role: match msg.role {
                Role::User => "user".to_string(),
                Role::Assistant => "assistant".to_string(),
                Role::System => "system".to_string(),
            },
            content: msg.content.clone(),
        }
    }
}

/// 工具选择策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolChoice {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "any")]
    Any,
    #[serde(rename = "tool")]
    Specific { name: String },
    #[serde(rename = "none")]
    None,
}

/// API 元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMetadata {
    /// 用户 ID（用于追踪）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

// ---------------------------------------------------------------------------
// 编排器配置（QueryEngineConfig → OrchestratorConfig）
// ---------------------------------------------------------------------------

/// 会话编排器配置。
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// 系统提示生成器。
    pub system_prompt: Vec<SystemBlock>,
    /// 可用工具列表。
    pub tools: Vec<ToolDefinition>,
    /// 工具使用上下文。
    pub tool_use_context: ToolUseContext,
    /// 模型 ID。
    pub model: String,
    /// 用户指定的模型覆盖。
    pub user_specified_model: Option<String>,
    /// 最大输出 token 数。
    pub max_output_tokens: Option<u32>,
    /// 来源标签。
    pub origin_tag: OriginTag,
    /// 快速模式。
    pub fast_mode: Option<bool>,
    /// Effort level applied to provider-specific reasoning controls.
    pub effort: Option<EffortLevel>,
    /// API 基础 URL。
    pub api_base_url: Option<String>,
    /// API 密钥。
    pub api_key: Option<String>,
    /// 是否跳过 stop hooks。
    pub skip_stop_hooks: bool,
    /// 自动模式。
    pub auto_mode: bool,
    /// 额外请求体。
    pub extra_body: HashMap<String, serde_json::Value>,
    /// Session permission mode.
    pub permission_mode: PermissionMode,
    /// Optional permission gate. `None` falls through to `AllowAllGate` so
    /// existing callers (oneshot / SDK / tests) keep their previous open
    /// semantics; the interactive REPL sets this to an `InteractiveGate`
    /// that drives a TUI modal for every tool-use.
    pub permission_gate: Option<std::sync::Arc<dyn PermissionGate>>,
    /// Optional executable tool registry. Required for dialogue.rs to
    /// actually dispatch tool_use blocks the model emits; without it the
    /// loop receives "tool not found" for every call and gives up. Built
    /// in mossen-cli (where `mossen_tools::all_tools()` is available) and
    /// injected here to avoid a circular dependency with mossen-tools.
    pub tool_registry: Option<std::sync::Arc<crate::tool_registry::ToolRegistry>>,
    /// Runtime hook context forwarded into dialogue so settings/plugin hooks
    /// can fire from turn, sampling, and compaction lifecycle points.
    pub hook_context: Option<std::sync::Arc<mossen_utils::hooks_utils::HooksContext>>,
}

/// 提交选项。
#[derive(Debug, Clone, Default)]
pub struct SubmitOptions {
    /// 是否为恢复会话。
    pub is_resume: bool,
    /// 附加消息。
    pub additional_messages: Vec<Message>,
    /// 最大轮次。
    pub max_turns: Option<u32>,
    /// Optional caller-owned cancellation token for this turn.
    pub cancel_token: Option<CancellationToken>,
    /// Extra blocks (e.g. images) folded into the user message after
    /// the text block. Lets multimodal turns ship through the same
    /// `dispatch_turn` entry point as text-only turns.
    pub additional_user_blocks: Vec<mossen_types::ContentBlock>,
}

// ---------------------------------------------------------------------------
// ask() 便捷入口参数
// ---------------------------------------------------------------------------

/// submit_prompt() 便捷入口参数。
#[derive(Debug, Clone)]
pub struct PromptParams {
    /// 用户提示内容。
    pub prompt: String,
    /// Conversation messages that should precede the new user prompt.
    /// The TUI keeps this trimmed through `/compact`; non-interactive
    /// callers leave it empty.
    pub history_messages: Vec<Message>,
    /// Extra content blocks (images, etc.) the engine should append to
    /// the user message *after* the text block. Used by the TUI's
    /// Ctrl+V paste handler to ship `ContentBlock::Image` to the
    /// multimodal API along with the text prompt. Empty list = pure
    /// text turn (the historical default).
    pub additional_blocks: Vec<mossen_types::ContentBlock>,
    /// 模型 ID。
    pub model: String,
    /// 系统提示。
    pub system_prompt: Vec<SystemBlock>,
    /// 工具列表。
    pub tools: Vec<ToolDefinition>,
    /// 工具使用上下文。
    pub tool_use_context: ToolUseContext,
    /// 来源标签。
    pub origin_tag: OriginTag,
    /// 最大轮次。
    pub max_turns: Option<u32>,
    /// Optional caller-owned cancellation token for this turn.
    pub cancel_token: Option<CancellationToken>,
    /// API 基础 URL。
    pub api_base_url: Option<String>,
    /// API 密钥。
    pub api_key: Option<String>,
    /// 额外请求体。
    pub extra_body: HashMap<String, serde_json::Value>,
    /// Optional live fast-mode override for this turn/session.
    pub fast_mode: Option<bool>,
    /// Optional live effort override for this turn/session.
    pub effort: Option<EffortLevel>,
    /// Session permission mode.
    pub permission_mode: PermissionMode,
    /// Optional permission gate forwarded to the orchestrator. The TUI uses
    /// this to inject an `InteractiveGate` whose `check()` opens a modal;
    /// non-interactive callers leave it `None` to fall through to
    /// `AllowAllGate`.
    pub permission_gate: Option<std::sync::Arc<dyn PermissionGate>>,
    /// Optional executable tool registry — forwarded to
    /// `OrchestratorConfig::tool_registry`. See that field for the rationale.
    pub tool_registry: Option<std::sync::Arc<crate::tool_registry::ToolRegistry>>,
    /// Runtime hook context forwarded to the orchestrator/dialogue.
    pub hook_context: Option<std::sync::Arc<mossen_utils::hooks_utils::HooksContext>>,
}

// ---------------------------------------------------------------------------
// 权限拒绝记录
// ---------------------------------------------------------------------------

/// SDK 权限拒绝记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkPermissionDenial {
    pub tool_name: String,
    pub reason: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// 文件状态缓存
// ---------------------------------------------------------------------------

/// 文件读取状态缓存。
#[derive(Debug, Clone, Default)]
pub struct FileStateCache {
    /// 已读取的文件路径 → 内容 hash。
    pub read_files: HashMap<String, u64>,
    /// 文件更新计数。
    pub update_count: u64,
}

// ---------------------------------------------------------------------------
// 缓存中断检测
// ---------------------------------------------------------------------------

/// 前次提示状态快照。
#[derive(Debug, Clone)]
pub struct PreviousPromptState {
    pub system_hash: u32,
    pub tools_hash: u32,
    pub cache_control_hash: u32,
    pub tool_names: Vec<String>,
    pub per_tool_hashes: HashMap<String, u32>,
    pub system_char_count: usize,
    pub model: String,
    pub fast_mode: bool,
    pub call_count: u64,
    pub prev_cache_read_tokens: Option<u64>,
}

// ---------------------------------------------------------------------------
// 微压缩策略
// ---------------------------------------------------------------------------

/// 微压缩策略。
#[derive(Debug, Clone)]
pub enum MicrocompactStrategy {
    /// 基于时间的微压缩。
    TimeBased {
        gap_minutes: f64,
        keep_recent: usize,
    },
    /// 缓存编辑微压缩。
    CacheEditing { tool_ids_to_delete: Vec<String> },
    /// 不执行微压缩。
    None,
}

#[cfg(test)]
mod permission_gate_tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn interactive_gate_allow_always_is_scoped_to_exact_shell_command() {
        let (tx, mut rx) = mpsc::channel(4);
        let gate = Arc::new(InteractiveGate::new(tx));
        let first_gate = gate.clone();
        let first = tokio::spawn(async move {
            first_gate
                .check(
                    "Bash",
                    "tool-1",
                    &serde_json::json!({ "command": "cargo test -q" }),
                )
                .await
        });

        let first_request = rx.recv().await.expect("first permission request");
        assert_eq!(first_request.tool_name, "Bash");
        assert_eq!(first_request.input["command"], "cargo test -q");
        first_request
            .responder
            .send(PermissionDecision::AllowAlways)
            .expect("send first decision");
        assert_eq!(
            first.await.expect("first permission task"),
            PermissionDecision::AllowAlways
        );

        let second = gate
            .check(
                "Bash",
                "tool-2",
                &serde_json::json!({ "command": "cargo test -q" }),
            )
            .await;
        assert_eq!(second, PermissionDecision::Allow);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), rx.recv())
                .await
                .is_err(),
            "same shell command should not prompt again after session approval"
        );

        let different_gate = gate.clone();
        let different = tokio::spawn(async move {
            different_gate
                .check(
                    "Bash",
                    "tool-3",
                    &serde_json::json!({ "command": "cargo check -q" }),
                )
                .await
        });
        let different_request = rx.recv().await.expect("different permission request");
        assert_eq!(different_request.input["command"], "cargo check -q");
        different_request
            .responder
            .send(PermissionDecision::Deny)
            .expect("send different decision");
        assert_eq!(
            different.await.expect("different permission task"),
            PermissionDecision::Deny
        );
    }

    #[tokio::test]
    async fn interactive_gate_allow_always_falls_back_to_tool_scope_without_shell_command() {
        let (tx, mut rx) = mpsc::channel(4);
        let gate = Arc::new(InteractiveGate::new(tx));
        let first_gate = gate.clone();
        let first = tokio::spawn(async move {
            first_gate
                .check(
                    "Read",
                    "tool-1",
                    &serde_json::json!({ "file_path": "a.rs" }),
                )
                .await
        });

        let first_request = rx.recv().await.expect("first permission request");
        assert_eq!(first_request.tool_name, "Read");
        first_request
            .responder
            .send(PermissionDecision::AllowAlways)
            .expect("send first decision");
        assert_eq!(
            first.await.expect("first permission task"),
            PermissionDecision::AllowAlways
        );

        let second = gate
            .check(
                "Read",
                "tool-2",
                &serde_json::json!({ "file_path": "b.rs" }),
            )
            .await;
        assert_eq!(second, PermissionDecision::Allow);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), rx.recv())
                .await
                .is_err(),
            "non-shell fallback tool rule should not prompt again in the same session"
        );
    }
}
