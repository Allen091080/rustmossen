//! # send_message — PeerDispatch 工具
//!
//! 对应 TS `SendMessageTool`（844 行）。在团队 agent 之间发送消息。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 同级分发器 — 向队友 agent 发送消息。
pub struct PeerDispatch;

#[derive(Debug, Clone, Deserialize)]
pub struct PeerDispatchInput {
    /// 接收者名称，队友名称或 "*" 广播。
    pub to: String,
    /// 5-10 字的摘要预览。
    #[serde(default)]
    pub summary: Option<String>,
    /// 消息内容（纯文本字符串或结构化消息）。
    pub message: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerDispatchOutput {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipients: Option<Vec<String>>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "to".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Recipient: teammate name, or \"*\" for broadcast to all teammates"
        }),
    );
    properties.insert("summary".to_string(), serde_json::json!({
        "type": "string",
        "description": "A 5-10 word summary shown as a preview in the UI (required when message is a string)"
    }));
    properties.insert(
        "message".to_string(),
        serde_json::json!({
            "oneOf": [
                { "type": "string", "description": "Plain text message content" },
                { "type": "object", "description": "Structured message content" }
            ]
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["to".to_string(), "message".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for PeerDispatch {
    fn name(&self) -> &str {
        "SendMessage"
    }
    fn description(&self) -> &str {
        "Send a message to a teammate agent"
    }
    fn tool_type(&self) -> ToolType {
        ToolType::Builtin
    }
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: build_input_schema(),
            cache_control: None,
        }
    }
    fn is_read_only(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: PeerDispatchInput = serde_json::from_value(input)?;
        let output = PeerDispatchOutput {
            success: false,
            message: format!("SendMessage to '{}' is a stub in this build.", inp.to),
            recipients: None,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
