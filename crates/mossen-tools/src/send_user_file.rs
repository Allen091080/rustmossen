//! # send_user_file — FileDelivery 工具
//!
//! 对应 TS `SendUserFileTool`（109 行）。向用户投递文件附件。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 文件投递器 — 向用户发送文件。
pub struct FileDelivery;

#[derive(Debug, Clone, Deserialize)]
pub struct FileDeliveryInput {
    /// 文件路径列表（绝对或相对于 cwd）。
    pub attachments: Vec<String>,
    /// 附带的简短说明。
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDeliveryAttachment {
    pub path: String,
    pub size: u64,
    #[serde(rename = "isImage")]
    pub is_image: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDeliveryOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub attachments: Vec<FileDeliveryAttachment>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sentAt")]
    pub sent_at: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "attachments".to_string(),
        serde_json::json!({
            "type": "array",
            "items": { "type": "string" },
            "minItems": 1,
            "description": "File paths (absolute or relative to cwd) to deliver to the user."
        }),
    );
    properties.insert(
        "message".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Optional short note to show with the delivered files."
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["attachments".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for FileDelivery {
    fn name(&self) -> &str {
        "SendUserFile"
    }
    fn description(&self) -> &str {
        "Deliver files or screenshots directly to the user"
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
        let inp: FileDeliveryInput = serde_json::from_value(input)?;
        let output = FileDeliveryOutput {
            message: inp.message,
            attachments: vec![],
            sent_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
