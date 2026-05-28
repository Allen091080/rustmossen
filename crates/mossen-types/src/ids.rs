//! # ids — 品牌类型
//!
//! 对应 TypeScript `types/ids.ts`。
//! 使用 newtype 模式防止 `SessionId` 与 `AgentId` 混用。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 会话唯一标识，newtype 防止与 `AgentId` 混用。
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionId(pub String);

/// 子 Agent 唯一标识。
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl SessionId {
    /// 从原始字符串构造 `SessionId`。
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 获取内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AgentId {
    /// 从原始字符串构造 `AgentId`。
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 获取内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 验证 `AgentId` 格式：`a` + 可选 `<label>-` + 16 位十六进制。
    /// 匹配 `createAgentId()` 产生的格式。
    /// 返回 `None` 表示格式不匹配（例如 teammate 名称、team-addressing）。
    pub fn parse(s: &str) -> Option<Self> {
        use once_cell::sync::Lazy;
        static AGENT_ID_RE: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"^a(?:.+-)?[0-9a-f]{16}$").unwrap());
        if AGENT_ID_RE.is_match(s) {
            Some(Self(s.to_string()))
        } else {
            None
        }
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
