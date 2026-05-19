//! # brief — SummaryCard 工具
//!
//! 对应 TS `BriefTool`。向用户发送消息，支持附件。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 摘要卡片 — 向用户发送可见消息。
pub struct SummaryCard;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct SummaryCardInput {
    /// 发送给用户的消息（支持 Markdown）。
    pub message: String,
    /// 可选附件文件路径列表。
    #[serde(default)]
    pub attachments: Option<Vec<String>>,
    /// 消息状态：normal | proactive。
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "normal".to_string()
}

/// 附件元信息。
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentMeta {
    pub path: String,
    pub size: u64,
    pub is_image: bool,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct SummaryCardOutput {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<AttachmentMeta>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "message".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The message for the user. Supports markdown formatting."
        }),
    );
    properties.insert(
        "attachments".to_string(),
        serde_json::json!({
            "type": "array",
            "items": { "type": "string" },
            "description": "Optional file paths to attach."
        }),
    );
    properties.insert(
        "status".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": ["normal", "proactive"],
            "description": "Use 'proactive' for unsolicited updates, 'normal' for replies."
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["message".to_string(), "status".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for SummaryCard {
    fn name(&self) -> &str {
        "SendUserMessage"
    }

    fn description(&self) -> &str {
        "Send a visible message to the user"
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
        let inp: SummaryCardInput = serde_json::from_value(input)?;

        let sent_at = chrono::Utc::now().to_rfc3339();

        // 解析附件元信息（如果有）。
        let attachments = if let Some(paths) = &inp.attachments {
            let mut metas = Vec::new();
            for path in paths {
                let expanded = if path.starts_with('~') {
                    if let Ok(home) = std::env::var("HOME") {
                        path.replacen('~', &home, 1)
                    } else {
                        path.clone()
                    }
                } else {
                    path.clone()
                };
                let meta = tokio::fs::metadata(&expanded).await;
                let (size, is_image) = match meta {
                    Ok(m) => {
                        let ext = std::path::Path::new(&expanded)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        let img = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp");
                        (m.len(), img)
                    }
                    Err(_) => (0, false),
                };
                metas.push(AttachmentMeta {
                    path: path.clone(),
                    size,
                    is_image,
                });
            }
            Some(metas)
        } else {
            None
        };

        let output = SummaryCardOutput {
            message: inp.message,
            attachments,
            sent_at: Some(sent_at),
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
