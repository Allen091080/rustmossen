//! # status_notice_helpers — 状态通知辅助工具
//!
//! 对应 TypeScript `utils/statusNoticeHelpers.ts`。
//! 代理描述的令牌计数估算。

use crate::status_notice_definitions::AgentDefinitionsResult;

/// 代理描述令牌阈值。
pub const AGENT_DESCRIPTIONS_THRESHOLD: usize = 15_000;

/// 粗略 token 估算（与 TS `roughTokenCountEstimation` 等价：~chars/4）。
fn rough_token_count_estimation(text: &str) -> usize {
    (text.len() + 3) / 4
}

/// 计算代理描述的累计令牌估算。
///
/// 对应 TS `getAgentDescriptionsTotalTokens` —— 排除 `built-in` 源代理，
/// 累加 `${agentType}: ${whenToUse}` 描述的 token 估算。
///
/// Rust 端目前 [`AgentDefinitionsResult`] 只保留累计 token 数（`total_description_tokens`）
/// 而不保留每个 agent 的描述串：上游 agent 加载层已经把代价摊到那里，本函数
/// 透传该字段即可。如果未来 [`AgentDefinitionsResult`] 扩展出 `active_agents`
/// 字段，可以重新接入字符级估算。
pub fn get_agent_descriptions_total_tokens(
    agent_definitions: Option<&AgentDefinitionsResult>,
) -> usize {
    agent_definitions
        .map(|d| d.total_description_tokens)
        .unwrap_or(0)
}

/// 直接对单个描述字符串做估算（保留给上游 agent 加载层使用）。
pub fn estimate_agent_description_tokens(agent_type: &str, when_to_use: &str) -> usize {
    let description = format!("{}: {}", agent_type, when_to_use);
    rough_token_count_estimation(&description)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threshold() {
        assert_eq!(AGENT_DESCRIPTIONS_THRESHOLD, 15_000);
    }

    #[test]
    fn test_empty_definitions() {
        assert_eq!(get_agent_descriptions_total_tokens(None), 0);
    }

    #[test]
    fn test_rough_token_estimation() {
        assert_eq!(rough_token_count_estimation(""), 0);
        assert_eq!(rough_token_count_estimation("abcd"), 1);
        assert_eq!(rough_token_count_estimation("a".repeat(8).as_str()), 2);
    }
}
