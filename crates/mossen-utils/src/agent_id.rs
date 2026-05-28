//! # agent_id — 确定性代理 ID 系统
//!
//! 对应 TypeScript `utils/agentId.ts`。
//! 提供代理 ID 的格式化和解析功能。
//!
//! ## ID 格式
//!
//! **代理 ID**: `agentName@teamName`
//! - 示例: `team-lead@my-project`, `researcher@my-project`
//! - `@` 符号作为代理名和团队名之间的分隔符
//!
//! **请求 ID**: `{requestType}-{timestamp}@{agentId}`
//! - 示例: `shutdown-1702500000000@researcher@my-project`
//! - 用于关闭请求、计划审批等
//!
//! ## 为什么使用确定性 ID？
//!
//! 1. **可重现性**: 相同名称和团队的代理总是得到相同的 ID
//! 2. **人类可读**: ID 有意义且可调试
//! 3. **可预测性**: 团队领导可以直接计算 teammate 的 ID

use std::time::{SystemTime, UNIX_EPOCH};

/// 解析后的代理 ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAgentId {
    pub agent_name: String,
    pub team_name: String,
}

/// 解析后的请求 ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRequestId {
    pub request_type: String,
    pub timestamp: u64,
    pub agent_id: String,
}

/// 格式化代理 ID，格式为 `agentName@teamName`。
pub fn format_agent_id(agent_name: &str, team_name: &str) -> String {
    format!("{}@{}", agent_name, team_name)
}

/// 解析代理 ID 为其组成部分。
///
/// 如果 ID 不包含 `@` 分隔符则返回 None。
pub fn parse_agent_id(agent_id: &str) -> Option<ParsedAgentId> {
    let at_index = agent_id.find('@')?;
    Some(ParsedAgentId {
        agent_name: agent_id[..at_index].to_string(),
        team_name: agent_id[at_index + 1..].to_string(),
    })
}

/// 生成请求 ID，格式为 `{requestType}-{timestamp}@{agentId}`。
pub fn generate_request_id(request_type: &str, agent_id: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{}-{}@{}", request_type, timestamp, agent_id)
}

/// 解析请求 ID 为其组成部分。
///
/// 如果请求 ID 不匹配预期格式则返回 None。
pub fn parse_request_id(request_id: &str) -> Option<ParsedRequestId> {
    let at_index = request_id.find('@')?;

    let prefix = &request_id[..at_index];
    let agent_id = &request_id[at_index + 1..];

    let last_dash_index = prefix.rfind('-')?;

    let request_type = &prefix[..last_dash_index];
    let timestamp_str = &prefix[last_dash_index + 1..];
    let timestamp: u64 = timestamp_str.parse().ok()?;

    Some(ParsedRequestId {
        request_type: request_type.to_string(),
        timestamp,
        agent_id: agent_id.to_string(),
    })
}
