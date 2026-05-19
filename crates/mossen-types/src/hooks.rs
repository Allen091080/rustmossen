//! # hooks — Hook 系统类型
//!
//! 对应 TypeScript `types/hooks.ts`。
//! 定义 `HookResult`、`HookProgress` 等 Hook 系统类型。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::permissions::{PermissionBehavior, PermissionUpdate};

/// Hook 事件类型 — 27 种事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    // Pre hooks（前置拦截）
    PreToolUse,
    PreCompact,

    // Post hooks（后置通知）
    PostToolUse,
    PostToolUseFailure,
    PostCompact,

    // Session lifecycle
    SessionStart,
    SessionEnd,
    Setup,

    // Agent lifecycle
    SubagentStart,
    SubagentStop,

    // User interaction
    UserPromptSubmit,
    PermissionDenied,
    PermissionRequest,
    Notification,

    // Stop hooks
    Stop,
    StopFailure,

    // MCP Elicitation
    Elicitation,
    ElicitationResult,

    // File/Directory watchers
    CwdChanged,
    FileChanged,
    InstructionsLoaded,
    ConfigChange,

    // Worktree
    WorktreeCreate,
    WorktreeRemove,

    // Team/Task
    TeammateIdle,
    TaskCreated,
    TaskCompleted,
}

impl HookEvent {
    /// 返回事件的显示名称。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PreCompact => "PreCompact",
            Self::PostToolUse => "PostToolUse",
            Self::PostToolUseFailure => "PostToolUseFailure",
            Self::PostCompact => "PostCompact",
            Self::SessionStart => "SessionStart",
            Self::SessionEnd => "SessionEnd",
            Self::Setup => "Setup",
            Self::SubagentStart => "SubagentStart",
            Self::SubagentStop => "SubagentStop",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::PermissionDenied => "PermissionDenied",
            Self::PermissionRequest => "PermissionRequest",
            Self::Notification => "Notification",
            Self::Stop => "Stop",
            Self::StopFailure => "StopFailure",
            Self::Elicitation => "Elicitation",
            Self::ElicitationResult => "ElicitationResult",
            Self::CwdChanged => "CwdChanged",
            Self::FileChanged => "FileChanged",
            Self::InstructionsLoaded => "InstructionsLoaded",
            Self::ConfigChange => "ConfigChange",
            Self::WorktreeCreate => "WorktreeCreate",
            Self::WorktreeRemove => "WorktreeRemove",
            Self::TeammateIdle => "TeammateIdle",
            Self::TaskCreated => "TaskCreated",
            Self::TaskCompleted => "TaskCompleted",
        }
    }
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for HookEvent {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PreToolUse" => Ok(Self::PreToolUse),
            "PreCompact" => Ok(Self::PreCompact),
            "PostToolUse" => Ok(Self::PostToolUse),
            "PostToolUseFailure" => Ok(Self::PostToolUseFailure),
            "PostCompact" => Ok(Self::PostCompact),
            "SessionStart" => Ok(Self::SessionStart),
            "SessionEnd" => Ok(Self::SessionEnd),
            "Setup" => Ok(Self::Setup),
            "SubagentStart" => Ok(Self::SubagentStart),
            "SubagentStop" => Ok(Self::SubagentStop),
            "UserPromptSubmit" => Ok(Self::UserPromptSubmit),
            "PermissionDenied" => Ok(Self::PermissionDenied),
            "PermissionRequest" => Ok(Self::PermissionRequest),
            "Notification" => Ok(Self::Notification),
            "Stop" => Ok(Self::Stop),
            "StopFailure" => Ok(Self::StopFailure),
            "Elicitation" => Ok(Self::Elicitation),
            "ElicitationResult" => Ok(Self::ElicitationResult),
            "CwdChanged" => Ok(Self::CwdChanged),
            "FileChanged" => Ok(Self::FileChanged),
            "InstructionsLoaded" => Ok(Self::InstructionsLoaded),
            "ConfigChange" => Ok(Self::ConfigChange),
            "WorktreeCreate" => Ok(Self::WorktreeCreate),
            "WorktreeRemove" => Ok(Self::WorktreeRemove),
            "TeammateIdle" => Ok(Self::TeammateIdle),
            "TaskCreated" => Ok(Self::TaskCreated),
            "TaskCompleted" => Ok(Self::TaskCompleted),
            _ => Err(format!("Unknown hook event: {s}")),
        }
    }
}

/// 全部 27 种 Hook 事件列表。
pub const HOOK_EVENTS: &[HookEvent] = &[
    HookEvent::PreToolUse,
    HookEvent::PreCompact,
    HookEvent::PostToolUse,
    HookEvent::PostToolUseFailure,
    HookEvent::PostCompact,
    HookEvent::SessionStart,
    HookEvent::SessionEnd,
    HookEvent::Setup,
    HookEvent::SubagentStart,
    HookEvent::SubagentStop,
    HookEvent::UserPromptSubmit,
    HookEvent::PermissionDenied,
    HookEvent::PermissionRequest,
    HookEvent::Notification,
    HookEvent::Stop,
    HookEvent::StopFailure,
    HookEvent::Elicitation,
    HookEvent::ElicitationResult,
    HookEvent::CwdChanged,
    HookEvent::FileChanged,
    HookEvent::InstructionsLoaded,
    HookEvent::ConfigChange,
    HookEvent::WorktreeCreate,
    HookEvent::WorktreeRemove,
    HookEvent::TeammateIdle,
    HookEvent::TaskCreated,
    HookEvent::TaskCompleted,
];

/// 始终发送的 Hook 事件（低噪声生命周期事件）。
pub const ALWAYS_EMITTED_HOOK_EVENTS: &[HookEvent] = &[HookEvent::SessionStart, HookEvent::Setup];

/// 检查值是否为有效的 Hook 事件。
pub fn is_hook_event(value: &str) -> bool {
    value.parse::<HookEvent>().is_ok()
}

/// Prompt 请求选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequestOption {
    /// 选项键。
    pub key: String,
    /// 选项标签。
    pub label: String,
    /// 选项描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Prompt 请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequest {
    /// 请求 ID。
    pub prompt: String,
    /// 消息。
    pub message: String,
    /// 选项。
    pub options: Vec<PromptRequestOption>,
}

/// Prompt 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptResponse {
    /// 请求 ID。
    pub prompt_response: String,
    /// 选择的选项。
    pub selected: String,
}

/// Hook 决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookDecision {
    /// 批准。
    Approve,
    /// 阻止。
    Block,
}

/// Hook 特定输出（各事件的特殊输出）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "hookEventName")]
pub enum HookSpecificOutput {
    PreToolUse {
        #[serde(skip_serializing_if = "Option::is_none")]
        permission_decision: Option<PermissionBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permission_decision_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_input: Option<HashMap<String, serde_json::Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    UserPromptSubmit {
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    SessionStart {
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        initial_user_message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        watch_paths: Option<Vec<String>>,
    },
    Setup {
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    SubagentStart {
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    PostToolUse {
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_mcp_tool_output: Option<serde_json::Value>,
    },
    PostToolUseFailure {
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    PermissionDenied {
        #[serde(skip_serializing_if = "Option::is_none")]
        retry: Option<bool>,
    },
    Notification {
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    PermissionRequest {
        decision: PermissionRequestDecision,
    },
    Elicitation {
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<ElicitationAction>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<HashMap<String, serde_json::Value>>,
    },
    ElicitationResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<ElicitationAction>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<HashMap<String, serde_json::Value>>,
    },
    CwdChanged {
        #[serde(skip_serializing_if = "Option::is_none")]
        watch_paths: Option<Vec<String>>,
    },
    FileChanged {
        #[serde(skip_serializing_if = "Option::is_none")]
        watch_paths: Option<Vec<String>>,
    },
    WorktreeCreate {
        worktree_path: String,
    },
}

/// 选举动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElicitationAction {
    Accept,
    Decline,
    Cancel,
}

/// 权限请求决策。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior")]
pub enum PermissionRequestDecision {
    #[serde(rename = "allow")]
    Allow {
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_input: Option<HashMap<String, serde_json::Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_permissions: Option<Vec<PermissionUpdate>>,
    },
    #[serde(rename = "deny")]
    Deny {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        interrupt: Option<bool>,
    },
}

/// 同步 Hook 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHookResponse {
    /// 是否继续。
    #[serde(rename = "continue", skip_serializing_if = "Option::is_none")]
    pub should_continue: Option<bool>,
    /// 是否抑制输出。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress_output: Option<bool>,
    /// 停止原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// 决策。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<HookDecision>,
    /// 原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 系统消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,
    /// Hook 特定输出。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<HookSpecificOutput>,
}

/// 异步 Hook 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncHookResponse {
    /// 标识为异步（始终为 true）。
    pub r#async: bool,
    /// 异步超时（秒）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub async_timeout: Option<f64>,
}

/// Hook JSON 输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookJsonOutput {
    /// 异步响应。
    Async(AsyncHookResponse),
    /// 同步响应。
    Sync(SyncHookResponse),
}

impl HookJsonOutput {
    /// 检查是否为同步 Hook 输出。
    pub fn is_sync(&self) -> bool {
        matches!(self, Self::Sync(_))
    }

    /// 检查是否为异步 Hook 输出。
    pub fn is_async(&self) -> bool {
        matches!(self, Self::Async(_))
    }
}

/// 权限请求结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior")]
pub enum PermissionRequestResult {
    #[serde(rename = "allow")]
    Allow {
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_input: Option<HashMap<String, serde_json::Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_permissions: Option<Vec<PermissionUpdate>>,
    },
    #[serde(rename = "deny")]
    Deny {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        interrupt: Option<bool>,
    },
}

/// Hook 进度。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookProgress {
    /// 类型标识。
    #[serde(rename = "type")]
    pub progress_type: String,
    /// Hook 事件。
    pub hook_event: HookEvent,
    /// Hook 名称。
    pub hook_name: String,
    /// 命令。
    pub command: String,
    /// 提示文本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_text: Option<String>,
    /// 状态消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
}

/// Hook 阻塞错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookBlockingError {
    /// 阻塞错误消息。
    pub blocking_error: String,
    /// 命令。
    pub command: String,
}

/// Hook 结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// 消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<crate::message::Message>,
    /// 系统消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message: Option<crate::message::Message>,
    /// 阻塞错误。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_error: Option<HookBlockingError>,
    /// 结果。
    pub outcome: HookOutcome,
    /// 是否阻止继续。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prevent_continuation: Option<bool>,
    /// 停止原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// 权限行为。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_behavior: Option<PermissionBehavior>,
    /// Hook 权限决策原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_permission_decision_reason: Option<String>,
    /// 附加上下文。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
    /// 初始用户消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_user_message: Option<String>,
    /// 更新的输入。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<HashMap<String, serde_json::Value>>,
    /// 更新的 MCP 工具输出。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_mcp_tool_output: Option<serde_json::Value>,
    /// 权限请求结果。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_request_result: Option<PermissionRequestResult>,
    /// 是否重试。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<bool>,
}

/// Hook 结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookOutcome {
    /// 成功。
    Success,
    /// 阻塞。
    Blocking,
    /// 非阻塞错误。
    NonBlockingError,
    /// 已取消。
    Cancelled,
}

/// 聚合 Hook 结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedHookResult {
    /// 消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<crate::message::Message>,
    /// 阻塞错误列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_errors: Option<Vec<HookBlockingError>>,
    /// 是否阻止继续。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prevent_continuation: Option<bool>,
    /// 停止原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// Hook 权限决策原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_permission_decision_reason: Option<String>,
    /// 权限行为。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_behavior: Option<PermissionBehavior>,
    /// 附加上下文列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_contexts: Option<Vec<String>>,
    /// 初始用户消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_user_message: Option<String>,
    /// 更新的输入。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<HashMap<String, serde_json::Value>>,
    /// 更新的 MCP 工具输出。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_mcp_tool_output: Option<serde_json::Value>,
    /// 权限请求结果。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_request_result: Option<PermissionRequestResult>,
    /// 是否重试。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<bool>,
}
