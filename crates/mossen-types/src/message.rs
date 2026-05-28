//! # message — 消息与内容块类型
//!
//! 对应 TypeScript `types/message.ts` 中引用的核心消息类型。
//! 定义 `Message`、`ContentBlock`、`Role` 等核心对话数据结构。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// 用户消息。
    User,
    /// 助手消息。
    Assistant,
    /// 系统消息。
    System,
}

/// 文本内容块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    /// 文本内容。
    pub text: String,
}

/// 工具调用块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseBlock {
    /// 工具调用 ID。
    pub id: String,
    /// 工具名称。
    pub name: String,
    /// 工具输入参数。
    pub input: serde_json::Value,
}

/// 工具结果块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultBlock {
    /// 工具调用 ID（关联的 `ToolUseBlock.id`）。
    pub tool_use_id: String,
    /// 结果内容（可以是字符串或内容块列表）。
    pub content: ToolResultContent,
    /// 是否为错误结果。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// 工具结果的内容表示。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// 简单文本结果。
    Text(String),
    /// 内容块列表。
    Blocks(Vec<ContentBlock>),
}

/// 思考/推理块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlock {
    /// 思考内容。
    pub thinking: String,
    /// 签名（用于验证）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// 图像内容块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBlock {
    /// 图像来源。
    pub source: ImageSource,
}

/// 图像来源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    /// 来源类型。
    #[serde(rename = "type")]
    pub source_type: String,
    /// 媒体类型。
    pub media_type: String,
    /// Base64 编码的数据。
    pub data: String,
}

/// 内容块联合类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// 文本。
    #[serde(rename = "text")]
    Text(TextBlock),
    /// 工具调用。
    #[serde(rename = "tool_use")]
    ToolUse(ToolUseBlock),
    /// 工具结果。
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultBlock),
    /// 思考/推理。
    #[serde(rename = "thinking")]
    Thinking(ThinkingBlock),
    /// 图像。
    #[serde(rename = "image")]
    Image(ImageBlock),
}

/// 消息来源标识。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageOrigin {
    /// 人类用户。
    Human,
    /// 队友消息。
    Teammate,
    /// 定时任务。
    Cron,
    /// 主动触发。
    Proactive,
    /// 频道消息。
    Channel,
    /// 跨会话消息。
    CrossSession,
    /// 通知。
    Notification,
}

/// 核心消息类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息角色。
    pub role: Role,
    /// 内容块列表。
    pub content: Vec<ContentBlock>,
    /// 消息 UUID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// 是否为元消息（对模型可见但对用户隐藏）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_meta: Option<bool>,
    /// 消息来源。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<MessageOrigin>,
    /// 消息时间戳（ISO 8601 字符串）。
    ///
    /// 对应 TS `Message.timestamp`。微压缩、转录、会话日志等流程依赖
    /// 此字段判断时间间隔。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// 额外元数据。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 助手消息（包含特定助手字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    /// 角色（始终为 `assistant`）。
    pub role: Role,
    /// 内容块列表。
    pub content: Vec<ContentBlock>,
    /// 消息 UUID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// 模型名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 停止原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// 额外元数据。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 用户消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    /// 角色（始终为 `user`）。
    pub role: Role,
    /// 内容块列表。
    pub content: Vec<ContentBlock>,
    /// 消息 UUID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// 是否为元消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_meta: Option<bool>,
    /// 消息来源。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<MessageOrigin>,
    /// 额外元数据。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 系统 API 错误消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemApiErrorMessage {
    /// 角色（始终为 `assistant`）。
    pub role: Role,
    /// 内容块列表。
    pub content: Vec<ContentBlock>,
    /// 是否为 API 错误。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_api_error_message: Option<bool>,
}

/// 墓碑消息（撤回）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneMessage {
    /// 被撤回的消息 UUID。
    pub uuid: String,
    /// 撤回原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 工具使用摘要消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseSummaryMessage {
    /// 工具名称。
    pub tool_name: String,
    /// 摘要文本。
    pub summary: String,
}
