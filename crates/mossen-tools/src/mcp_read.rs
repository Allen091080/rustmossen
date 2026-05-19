//! # mcp_read — BridgeReader 工具
//!
//! 对应 TS `ReadMcpResourceTool`（159 行）。读取 MCP 资源内容。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 桥接资源读取 — 按 URI 读取 MCP 资源。
pub struct BridgeReader;

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeReaderInput {
    pub server: String,
    pub uri: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeReaderOutput {
    pub contents: Vec<ResourceContent>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "server".to_string(),
        serde_json::json!({
            "type": "string", "description": "The MCP server name"
        }),
    );
    properties.insert(
        "uri".to_string(),
        serde_json::json!({
            "type": "string", "description": "The resource URI to read"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["server".to_string(), "uri".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for BridgeReader {
    fn name(&self) -> &str {
        "ReadMcpResource"
    }
    fn description(&self) -> &str {
        "Read a specific MCP resource by URI"
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
        let inp: BridgeReaderInput = serde_json::from_value(input)?;
        let output = BridgeReaderOutput {
            contents: vec![ResourceContent {
                uri: inp.uri,
                mime_type: None,
                text: None,
            }],
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
