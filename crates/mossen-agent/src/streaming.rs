//! # streaming — SSE 流式响应解析
//!
//! 对应 TS 中 Provider SDK 的 stream 消费逻辑。
//! 解析 Server-Sent Events 流并转化为类型化的 StreamEvent。

use std::time::Duration;

use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};

use crate::types::{ApiUsage, ContentDelta};

// ---------------------------------------------------------------------------
// SSE 事件类型
// ---------------------------------------------------------------------------

/// 原始 SSE 事件。
#[derive(Debug, Clone)]
pub struct RawSseEvent {
    /// 事件类型。
    pub event: String,
    /// 数据载荷。
    pub data: String,
}

/// 解析后的流式事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    /// 消息开始。
    #[serde(rename = "message_start")]
    MessageStart { message: MessageStartPayload },

    /// 内容块开始。
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: ContentBlockInfo,
    },

    /// 内容块增量。
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: ContentDelta },

    /// 内容块停止。
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },

    /// 消息增量（usage / stop_reason 更新）。
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDeltaPayload,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ApiUsage>,
    },

    /// 消息停止。
    #[serde(rename = "message_stop")]
    MessageStop,

    /// Ping（心跳）。
    #[serde(rename = "ping")]
    Ping,

    /// 错误。
    #[serde(rename = "error")]
    Error { error: StreamErrorPayload },
}

/// 消息开始载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageStartPayload {
    pub id: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub role: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ApiUsage>,
}

/// 内容块信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlockInfo {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
}

/// 消息增量载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDeltaPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
}

/// 流式错误载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamErrorPayload {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// SSE 解析
// ---------------------------------------------------------------------------

/// 从原始 SSE 事件字符串解析出 StreamEvent。
pub fn parse_sse_event(raw: &RawSseEvent) -> Result<StreamEvent, StreamParseError> {
    match raw.event.as_str() {
        "message_start" => {
            let payload: MessageStartPayload = serde_json::from_str(&raw.data)?;
            Ok(StreamEvent::MessageStart { message: payload })
        }
        "content_block_start" => {
            #[derive(Deserialize)]
            struct Wrapper {
                index: usize,
                content_block: ContentBlockInfo,
            }
            let w: Wrapper = serde_json::from_str(&raw.data)?;
            Ok(StreamEvent::ContentBlockStart {
                index: w.index,
                content_block: w.content_block,
            })
        }
        "content_block_delta" => {
            #[derive(Deserialize)]
            struct Wrapper {
                index: usize,
                delta: ContentDelta,
            }
            let w: Wrapper = serde_json::from_str(&raw.data)?;
            Ok(StreamEvent::ContentBlockDelta {
                index: w.index,
                delta: w.delta,
            })
        }
        "content_block_stop" => {
            #[derive(Deserialize)]
            struct Wrapper {
                index: usize,
            }
            let w: Wrapper = serde_json::from_str(&raw.data)?;
            Ok(StreamEvent::ContentBlockStop { index: w.index })
        }
        "message_delta" => {
            #[derive(Deserialize)]
            struct Wrapper {
                delta: MessageDeltaPayload,
                usage: Option<ApiUsage>,
            }
            let w: Wrapper = serde_json::from_str(&raw.data)?;
            Ok(StreamEvent::MessageDelta {
                delta: w.delta,
                usage: w.usage,
            })
        }
        "message_stop" => Ok(StreamEvent::MessageStop),
        "ping" => Ok(StreamEvent::Ping),
        "error" => {
            let payload: StreamErrorPayload = serde_json::from_str(&raw.data)?;
            Ok(StreamEvent::Error { error: payload })
        }
        other => Err(StreamParseError::UnknownEvent(other.to_string())),
    }
}

/// 从字节流中提取 SSE 行。
///
/// SSE 协议：每个事件以 `\n\n` 分隔，
/// 字段格式为 `field: value\n`。
pub fn parse_sse_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(':') {
        return None;
    }
    if let Some((field, value)) = line.split_once(':') {
        let value = value.strip_prefix(' ').unwrap_or(value);
        Some((field.to_string(), value.to_string()))
    } else {
        Some((line.to_string(), String::new()))
    }
}

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

/// 流式解析错误。
#[derive(Debug, thiserror::Error)]
pub enum StreamParseError {
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unknown SSE event type: {0}")]
    UnknownEvent(String),

    #[error("Stream timeout after {0:?}")]
    Timeout(Duration),

    #[error("Stream cancelled")]
    Cancelled,

    #[error("Connection error: {0}")]
    Connection(String),
}

// ---------------------------------------------------------------------------
// 流式消息累加器
// ---------------------------------------------------------------------------

/// 累加流式事件以构建完整的 AssistantMessage。
#[derive(Debug, Default)]
pub struct StreamAccumulator {
    /// 消息 ID。
    pub message_id: Option<String>,
    /// 模型。
    pub model: Option<String>,
    /// 已完成的内容块。
    pub content_blocks: Vec<AccumulatedBlock>,
    /// 当前正在累积的块。
    current_block: Option<AccumulatingBlock>,
    /// 停止原因。
    pub stop_reason: Option<String>,
    /// 最终用量。
    pub usage: Option<ApiUsage>,
}

/// 已完成累积的内容块。
#[derive(Debug, Clone)]
pub enum AccumulatedBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input_json: String,
    },
    Thinking {
        thinking: String,
    },
}

/// 正在累积中的块。
#[derive(Debug)]
enum AccumulatingBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input_json: String,
    },
    Thinking(String),
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// 处理一个流式事件。
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::MessageStart { message } => {
                self.message_id = Some(message.id.clone());
                self.model = Some(message.model.clone());
                if let Some(u) = &message.usage {
                    self.usage = Some(u.clone());
                }
            }
            StreamEvent::ContentBlockStart { content_block, .. } => {
                // 完成前一个块
                self.finish_current_block();
                self.current_block = Some(match content_block {
                    ContentBlockInfo::Text { text } => AccumulatingBlock::Text(text.clone()),
                    ContentBlockInfo::ToolUse { id, name } => AccumulatingBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input_json: String::new(),
                    },
                    ContentBlockInfo::Thinking { thinking } => {
                        AccumulatingBlock::Thinking(thinking.clone())
                    }
                });
            }
            StreamEvent::ContentBlockDelta { delta, .. } => {
                if let Some(ref mut block) = self.current_block {
                    match (block, delta) {
                        (
                            AccumulatingBlock::Text(ref mut text),
                            ContentDelta::TextDelta { text: t },
                        ) => {
                            text.push_str(t);
                        }
                        (
                            AccumulatingBlock::ToolUse { input_json, .. },
                            ContentDelta::InputJsonDelta { partial_json },
                        ) => {
                            input_json.push_str(partial_json);
                        }
                        (
                            AccumulatingBlock::Thinking(ref mut s),
                            ContentDelta::ThinkingDelta { thinking },
                        ) => {
                            s.push_str(thinking);
                        }
                        _ => {}
                    }
                }
            }
            StreamEvent::ContentBlockStop { .. } => {
                self.finish_current_block();
            }
            StreamEvent::MessageDelta { delta, usage } => {
                if let Some(reason) = &delta.stop_reason {
                    self.stop_reason = Some(reason.clone());
                }
                if let Some(u) = usage {
                    self.usage = Some(u.clone());
                }
            }
            StreamEvent::MessageStop => {
                self.finish_current_block();
            }
            _ => {}
        }
    }

    fn finish_current_block(&mut self) {
        if let Some(block) = self.current_block.take() {
            let accumulated = match block {
                AccumulatingBlock::Text(text) => AccumulatedBlock::Text(text),
                AccumulatingBlock::ToolUse {
                    id,
                    name,
                    input_json,
                } => AccumulatedBlock::ToolUse {
                    id,
                    name,
                    input_json,
                },
                AccumulatingBlock::Thinking(thinking) => AccumulatedBlock::Thinking { thinking },
            };
            self.content_blocks.push(accumulated);
        }
    }

    /// 是否有工具调用。
    pub fn has_tool_use(&self) -> bool {
        self.content_blocks
            .iter()
            .any(|b| matches!(b, AccumulatedBlock::ToolUse { .. }))
    }

    /// 提取所有工具调用。
    pub fn tool_uses(&self) -> Vec<(String, String, String)> {
        self.content_blocks
            .iter()
            .filter_map(|b| {
                if let AccumulatedBlock::ToolUse {
                    id,
                    name,
                    input_json,
                } = b
                {
                    Some((id.clone(), name.clone(), input_json.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// 提取可见文本。
    pub fn visible_text(&self) -> String {
        self.content_blocks
            .iter()
            .filter_map(|b| {
                if let AccumulatedBlock::Text(text) = b {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("")
    }
}
