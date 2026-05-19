//! # teammate_prompt_addendum — 队友提示补充
//!
//! 对应 TypeScript `utils/swarm/teammatePromptAddendum.ts`。
//! 追加到队友完整主代理系统提示的队友特定补充。

/// 队友系统提示补充。
/// 解释可见性约束和通信要求。
pub const TEAMMATE_SYSTEM_PROMPT_ADDENDUM: &str = r#"
# Agent Teammate Communication

IMPORTANT: You are running as an agent in a team. To communicate with anyone on your team:
- Use the SendMessage tool with `to: "<name>"` to send messages to specific teammates
- Use the SendMessage tool with `to: "*"` sparingly for team-wide broadcasts

Just writing a response in text is not visible to others on your team - you MUST use the SendMessage tool.

The user interacts primarily with the team lead. Your work is coordinated through the task system and teammate messaging.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_teammate_prompt_addendum() {
        assert!(TEAMMATE_SYSTEM_PROMPT_ADDENDUM.contains("SendMessage tool"));
        assert!(TEAMMATE_SYSTEM_PROMPT_ADDENDUM.contains("IMPORTANT"));
    }
}