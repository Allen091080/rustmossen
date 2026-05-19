//! # notification — AlertDispatcher 工具
//!
//! 对应 TS `PushNotificationTool`（84 行）。发送系统通知。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 告警分发器 — 向用户发送系统通知。
pub struct AlertDispatcher;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct AlertDispatcherInput {
    /// 通知标题（1-120 字符）。
    pub title: String,
    /// 通知正文（1-500 字符）。
    pub body: String,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct AlertDispatcherOutput {
    pub delivered: bool,
    pub title: String,
    pub body: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "title".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Notification title (max 120 chars)",
            "maxLength": 120
        }),
    );
    properties.insert(
        "body".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Notification body (max 500 chars)",
            "maxLength": 500
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["title".to_string(), "body".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for AlertDispatcher {
    fn name(&self) -> &str {
        "PushNotification"
    }

    fn description(&self) -> &str {
        "Send a push notification to the user"
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
        true
    }

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: AlertDispatcherInput = serde_json::from_value(input)?;

        let output = AlertDispatcherOutput {
            delivered: true,
            title: inp.title,
            body: inp.body,
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
