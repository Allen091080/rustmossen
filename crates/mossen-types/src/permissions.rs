//! # permissions — 权限系统核心类型
//!
//! 对应 TypeScript `types/permissions.ts`。
//! 包含权限模式、权限规则、权限决策等完整权限系统类型。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Permission Modes
// ============================================================================

/// 外部可见的权限模式（用户可设置）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalPermissionMode {
    /// 接受编辑。
    AcceptEdits,
    /// 绕过权限。
    BypassPermissions,
    /// 默认模式（需确认）。
    Default,
    /// 不询问（自动拒绝）。
    DontAsk,
    /// 计划模式（只读）。
    Plan,
}

/// 内部权限模式（包含内部专用模式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// 接受编辑。
    AcceptEdits,
    /// 绕过权限。
    BypassPermissions,
    /// 默认模式。
    Default,
    /// 不询问。
    DontAsk,
    /// 计划模式。
    Plan,
    /// 自动模式（Swift 仲裁器驱动）。
    Auto,
    /// 冒泡/委派模式（委托给上级）。
    Bubble,
}

/// 外部权限模式常量列表。
pub const EXTERNAL_PERMISSION_MODES: &[ExternalPermissionMode] = &[
    ExternalPermissionMode::AcceptEdits,
    ExternalPermissionMode::BypassPermissions,
    ExternalPermissionMode::Default,
    ExternalPermissionMode::DontAsk,
    ExternalPermissionMode::Plan,
];

impl From<ExternalPermissionMode> for PermissionMode {
    fn from(m: ExternalPermissionMode) -> Self {
        match m {
            ExternalPermissionMode::AcceptEdits => Self::AcceptEdits,
            ExternalPermissionMode::BypassPermissions => Self::BypassPermissions,
            ExternalPermissionMode::Default => Self::Default,
            ExternalPermissionMode::DontAsk => Self::DontAsk,
            ExternalPermissionMode::Plan => Self::Plan,
        }
    }
}

// ============================================================================
// Permission Behaviors
// ============================================================================

/// 权限行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionBehavior {
    /// 允许。
    Allow,
    /// 拒绝。
    Deny,
    /// 询问用户。
    Ask,
}

// ============================================================================
// Permission Rules
// ============================================================================

/// 权限规则来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionRuleSource {
    /// 用户设置。
    UserSettings,
    /// 项目设置。
    ProjectSettings,
    /// 本地设置。
    LocalSettings,
    /// 功能标志设置。
    FlagSettings,
    /// 策略设置。
    PolicySettings,
    /// CLI 参数。
    CliArg,
    /// 命令。
    Command,
    /// 会话。
    Session,
}

/// 权限规则值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRuleValue {
    /// 工具名称。
    pub tool_name: String,
    /// 规则内容（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_content: Option<String>,
}

/// 权限规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    /// 规则来源。
    pub source: PermissionRuleSource,
    /// 规则行为。
    pub rule_behavior: PermissionBehavior,
    /// 规则值。
    pub rule_value: PermissionRuleValue,
}

// ============================================================================
// Permission Updates
// ============================================================================

/// 权限更新目的地。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionUpdateDestination {
    /// 用户设置。
    UserSettings,
    /// 项目设置。
    ProjectSettings,
    /// 本地设置。
    LocalSettings,
    /// 会话。
    Session,
    /// CLI 参数。
    CliArg,
}

/// 权限更新操作。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PermissionUpdate {
    /// 添加规则。
    AddRules {
        destination: PermissionUpdateDestination,
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
    },
    /// 替换规则。
    ReplaceRules {
        destination: PermissionUpdateDestination,
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
    },
    /// 移除规则。
    RemoveRules {
        destination: PermissionUpdateDestination,
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
    },
    /// 设置模式。
    SetMode {
        destination: PermissionUpdateDestination,
        mode: ExternalPermissionMode,
    },
    /// 添加目录。
    AddDirectories {
        destination: PermissionUpdateDestination,
        directories: Vec<String>,
    },
    /// 移除目录。
    RemoveDirectories {
        destination: PermissionUpdateDestination,
        directories: Vec<String>,
    },
}

/// 附加工作目录来源（等同于 `PermissionRuleSource`）。
pub type WorkingDirectorySource = PermissionRuleSource;

/// 附加工作目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdditionalWorkingDirectory {
    /// 目录路径。
    pub path: String,
    /// 来源。
    pub source: WorkingDirectorySource,
}

// ============================================================================
// Permission Decisions & Results
// ============================================================================

/// 权限命令元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCommandMetadata {
    /// 命令名称。
    pub name: String,
    /// 命令描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 额外属性。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 权限元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionMetadata {
    /// 关联的命令元数据。
    pub command: PermissionCommandMetadata,
}

/// 待处理的分类器检查。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingClassifierCheck {
    /// 命令。
    pub command: String,
    /// 当前工作目录。
    pub cwd: String,
    /// 描述列表。
    pub descriptions: Vec<String>,
}

/// 权限决策原因。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PermissionDecisionReason {
    /// 基于规则。
    Rule { rule: PermissionRule },
    /// 基于模式。
    Mode { mode: PermissionMode },
    /// 子命令结果。
    SubcommandResults {
        reasons: HashMap<String, serde_json::Value>,
    },
    /// 权限提示工具。
    PermissionPromptTool {
        permission_prompt_tool_name: String,
        tool_result: serde_json::Value,
    },
    /// Hook。
    Hook {
        hook_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        hook_source: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// 异步 Agent。
    AsyncAgent { reason: String },
    /// 沙箱覆盖。
    SandboxOverride { reason: String },
    /// 分类器。
    Classifier { classifier: String, reason: String },
    /// 工作目录。
    WorkingDir { reason: String },
    /// 安全检查。
    SafetyCheck {
        reason: String,
        classifier_approvable: bool,
    },
    /// 其他。
    Other { reason: String },
}

/// 权限决策。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior")]
pub enum PermissionDecision {
    /// 允许。
    #[serde(rename = "allow")]
    Allow {
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_input: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_modified: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_reason: Option<PermissionDecisionReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        accept_feedback: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_blocks: Option<Vec<serde_json::Value>>,
    },
    /// 询问。
    #[serde(rename = "ask")]
    Ask {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_input: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_reason: Option<PermissionDecisionReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        suggestions: Option<Vec<PermissionUpdate>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<PermissionMetadata>,
        /// 由 bashCommandIsSafe_DEPRECATED 安全检查触发。
        #[serde(skip_serializing_if = "Option::is_none")]
        is_bash_security_check_for_misparsing: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pending_classifier_check: Option<PendingClassifierCheck>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_blocks: Option<Vec<serde_json::Value>>,
    },
    /// 拒绝。
    #[serde(rename = "deny")]
    Deny {
        message: String,
        decision_reason: PermissionDecisionReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
}

/// 权限结果（包含 passthrough 选项）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior")]
pub enum PermissionResult {
    /// 允许。
    #[serde(rename = "allow")]
    Allow {
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_input: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_modified: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_reason: Option<PermissionDecisionReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        accept_feedback: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_blocks: Option<Vec<serde_json::Value>>,
    },
    /// 询问。
    #[serde(rename = "ask")]
    Ask {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_input: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_reason: Option<PermissionDecisionReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        suggestions: Option<Vec<PermissionUpdate>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<PermissionMetadata>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_bash_security_check_for_misparsing: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pending_classifier_check: Option<PendingClassifierCheck>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_blocks: Option<Vec<serde_json::Value>>,
    },
    /// 拒绝。
    #[serde(rename = "deny")]
    Deny {
        message: String,
        decision_reason: PermissionDecisionReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
    /// 透传。
    #[serde(rename = "passthrough")]
    Passthrough {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_reason: Option<PermissionDecisionReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        suggestions: Option<Vec<PermissionUpdate>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pending_classifier_check: Option<PendingClassifierCheck>,
    },
}

// ============================================================================
// Bash Classifier Types
// ============================================================================

/// 分类器结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifierResult {
    /// 是否匹配。
    pub matches: bool,
    /// 匹配的描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_description: Option<String>,
    /// 置信度。
    pub confidence: ClassifierConfidence,
    /// 原因。
    pub reason: String,
}

/// 分类器置信度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassifierConfidence {
    /// 高。
    High,
    /// 中。
    Medium,
    /// 低。
    Low,
}

/// 分类器行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassifierBehavior {
    /// 拒绝。
    Deny,
    /// 询问。
    Ask,
    /// 允许。
    Allow,
}

/// 分类器 token 用量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifierUsage {
    /// 输入 token 数。
    pub input_tokens: u64,
    /// 输出 token 数。
    pub output_tokens: u64,
    /// 缓存读取输入 token 数。
    pub cache_read_input_tokens: u64,
    /// 缓存创建输入 token 数。
    pub cache_creation_input_tokens: u64,
}

/// YOLO/Swift 分类器结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloClassifierResult {
    /// 思考内容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    /// 是否应阻止。
    pub should_block: bool,
    /// 原因。
    pub reason: String,
    /// 是否不可用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<bool>,
    /// 记录是否过长。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_too_long: Option<bool>,
    /// 使用的模型。
    pub model: String,
    /// Token 用量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ClassifierUsage>,
    /// 持续时间（毫秒）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// 提示长度。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_lengths: Option<PromptLengths>,
    /// 错误转储路径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_dump_path: Option<String>,
    /// 阶段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<ClassifierStage>,
    /// 阶段 1 用量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage1_usage: Option<ClassifierUsage>,
    /// 阶段 1 持续时间。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage1_duration_ms: Option<u64>,
    /// 阶段 1 请求 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage1_request_id: Option<String>,
    /// 阶段 1 消息 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage1_msg_id: Option<String>,
    /// 阶段 2 用量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage2_usage: Option<ClassifierUsage>,
    /// 阶段 2 持续时间。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage2_duration_ms: Option<u64>,
    /// 阶段 2 请求 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage2_request_id: Option<String>,
    /// 阶段 2 消息 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage2_msg_id: Option<String>,
}

/// 提示长度信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptLengths {
    /// 系统提示长度。
    pub system_prompt: usize,
    /// 工具调用长度。
    pub tool_calls: usize,
    /// 用户提示长度。
    pub user_prompts: usize,
}

/// 分类器阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassifierStage {
    /// 快速阶段。
    Fast,
    /// 思考阶段。
    Thinking,
}

// ============================================================================
// Permission Explainer Types
// ============================================================================

/// 风险等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// 低。
    LOW,
    /// 中。
    MEDIUM,
    /// 高。
    HIGH,
}

/// 权限解释。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionExplanation {
    /// 风险等级。
    pub risk_level: RiskLevel,
    /// 解释。
    pub explanation: String,
    /// 推理过程。
    pub reasoning: String,
    /// 风险描述。
    pub risk: String,
}

// ============================================================================
// Tool Permission Context
// ============================================================================

/// 按来源分组的工具权限规则。
pub type ToolPermissionRulesBySource = HashMap<String, Vec<String>>;

/// 工具权限上下文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermissionContext {
    /// 当前权限模式。
    pub mode: PermissionMode,
    /// 附加工作目录。
    pub additional_working_directories: HashMap<String, AdditionalWorkingDirectory>,
    /// 始终允许规则。
    pub always_allow_rules: ToolPermissionRulesBySource,
    /// 始终拒绝规则。
    pub always_deny_rules: ToolPermissionRulesBySource,
    /// 始终询问规则。
    pub always_ask_rules: ToolPermissionRulesBySource,
    /// 绕过权限模式是否可用。
    pub is_bypass_permissions_mode_available: bool,
    /// 已剥离的危险规则。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stripped_dangerous_rules: Option<ToolPermissionRulesBySource>,
    /// 是否应避免权限提示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should_avoid_permission_prompts: Option<bool>,
    /// 是否在对话框之前等待自动检查。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub await_automated_checks_before_dialog: Option<bool>,
    /// 计划模式之前的权限模式。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_plan_mode: Option<PermissionMode>,
}
