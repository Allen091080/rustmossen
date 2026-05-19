//! # tool — 工具定义类型
//!
//! 定义 `Tool`、`ToolDefinition`、`ToolUseContext` 等工具系统类型。
//! 对应 TypeScript 中 `Tool.ts` 及相关模块的核心工具抽象。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 工具输入模式（JSON Schema）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputSchema {
    /// Schema 类型（始终为 "object"）。
    #[serde(rename = "type")]
    pub schema_type: String,
    /// 属性定义。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, serde_json::Value>>,
    /// 必需属性列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    /// 额外属性。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 工具定义（用于 API 请求）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 工具名称。
    pub name: String,
    /// 工具描述。
    pub description: String,
    /// 输入模式。
    pub input_schema: ToolInputSchema,
    /// 缓存控制。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// 缓存控制。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    /// 类型。
    #[serde(rename = "type")]
    pub control_type: String,
}

/// 工具类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolType {
    /// 内置工具。
    Builtin,
    /// MCP 工具。
    Mcp,
    /// 合成输出工具。
    SyntheticOutput,
}

/// 工具结果大小配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultSizeConfig {
    /// 最大结果字符数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_result_size_chars: Option<usize>,
}

/// 工具使用上下文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseContext {
    /// 当前工作目录。
    pub cwd: String,
    /// 附加工作目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_working_directories: Option<Vec<String>>,
    /// 额外上下文。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 工具名称常量。
pub mod tool_names {
    pub const FILE_READ: &str = "Read";
    pub const FILE_WRITE: &str = "Write";
    pub const FILE_EDIT: &str = "Edit";
    pub const BASH: &str = "Bash";
    pub const GLOB: &str = "Glob";
    pub const GREP: &str = "Grep";
    pub const WEB_SEARCH: &str = "WebSearch";
    pub const WEB_FETCH: &str = "WebFetch";
    pub const TODO_WRITE: &str = "TodoWrite";
    pub const AGENT: &str = "Agent";
    pub const SKILL: &str = "Skill";
    pub const NOTEBOOK_EDIT: &str = "NotebookEdit";
    pub const SEND_MESSAGE: &str = "SendMessage";
    pub const TASK_CREATE: &str = "TaskCreate";
    pub const TASK_GET: &str = "TaskGet";
    pub const TASK_LIST: &str = "TaskList";
    pub const TASK_UPDATE: &str = "TaskUpdate";
    pub const TASK_OUTPUT: &str = "TaskOutput";
    pub const TASK_STOP: &str = "TaskStop";
    pub const ASK_USER_QUESTION: &str = "AskUserQuestion";
    pub const TOOL_SEARCH: &str = "ToolSearch";
    pub const ENTER_WORKTREE: &str = "EnterWorktree";
    pub const EXIT_WORKTREE: &str = "ExitWorktree";
    pub const ENTER_PLAN_MODE: &str = "EnterPlanMode";
    pub const EXIT_PLAN_MODE: &str = "ExitPlanMode";
    pub const SYNTHETIC_OUTPUT: &str = "SyntheticOutput";
    pub const WORKFLOW: &str = "Workflow";
    pub const SLEEP: &str = "Sleep";
}
