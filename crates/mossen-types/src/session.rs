//! # session — 会话相关类型
//!
//! 定义会话状态、会话管理等类型。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 会话状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// 活跃。
    Active,
    /// 已暂停。
    Paused,
    /// 已完成。
    Completed,
    /// 已取消。
    Cancelled,
}

/// 会话信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    /// 会话 ID。
    pub session_id: String,
    /// 创建时间。
    pub created: String,
    /// 修改时间。
    pub modified: String,
    /// 当前工作目录。
    pub cwd: String,
    /// 状态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SessionStatus>,
    /// 模型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 首个提示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_prompt: Option<String>,
    /// 消息数量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<usize>,
    /// 是否为侧链。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_sidechain: Option<bool>,
    /// 额外属性。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
