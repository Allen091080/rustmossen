//! # message_predicates — 消息谓词判断
//!
//! 对应 TypeScript `utils/messagePredicates.ts`。
//! 提供消息类型判断函数。

/// 简化的消息结构。
#[derive(Debug, Clone)]
pub struct Message {
    #[allow(dead_code)]
    pub type_: String,
    #[allow(dead_code)]
    pub is_meta: bool,
    #[allow(dead_code)]
    pub tool_use_result: Option<()>,
}

/// 判断消息是否是人类消息（而非工具结果消息）。
///
/// tool_result 消息与人类消息共享 type:'user'，需要通过
/// toolUseResult 字段来区分。
pub fn is_human_turn(m: &Message) -> bool {
    m.type_ == "user" && !m.is_meta && m.tool_use_result.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_human_turn() {
        let human_msg = Message {
            type_: "user".to_string(),
            is_meta: false,
            tool_use_result: None,
        };
        assert!(is_human_turn(&human_msg));
    }

    #[test]
    fn test_is_not_human_turn_with_tool_result() {
        let tool_result_msg = Message {
            type_: "user".to_string(),
            is_meta: false,
            tool_use_result: Some(()),
        };
        assert!(!is_human_turn(&tool_result_msg));
    }
}
