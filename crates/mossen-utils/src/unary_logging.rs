//! # unary_logging — 一元事件日志
//!
//! 对应 TypeScript `utils/unaryLogging.ts`。

/// 补全类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionType {
    StrReplaceSingle,
    StrReplaceMulti,
    WriteFileSingle,
    ToolUseSingle,
}

impl CompletionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StrReplaceSingle => "str_replace_single",
            Self::StrReplaceMulti => "str_replace_multi",
            Self::WriteFileSingle => "write_file_single",
            Self::ToolUseSingle => "tool_use_single",
        }
    }
}

/// 一元事件类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryEventType {
    Accept,
    Reject,
    Response,
}

/// 日志事件元数据。
#[derive(Debug, Clone)]
pub struct UnaryEventMetadata {
    pub language_name: String,
    pub message_id: String,
    pub platform: String,
    pub has_feedback: Option<bool>,
}

/// 一元日志事件。
#[derive(Debug, Clone)]
pub struct UnaryLogEvent {
    pub completion_type: CompletionType,
    pub event: UnaryEventType,
    pub metadata: UnaryEventMetadata,
}

/// 记录一元事件。
pub async fn log_unary_event(event: UnaryLogEvent) {
    tracing::info!(
        target: "tengu_unary_event",
        event_type = ?event.event,
        completion_type = event.completion_type.as_str(),
        language_name = %event.metadata.language_name,
        message_id = %event.metadata.message_id,
        platform = %event.metadata.platform,
    );
}
