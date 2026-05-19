//! # mcp_auth — BridgeAuthenticator 工具
//!
//! 对应 TS `McpAuthTool`（216 行）。MCP 服务器 OAuth 认证流程。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 桥接认证器 — MCP 服务器 OAuth 认证。
pub struct BridgeAuthenticator;

#[derive(Debug, Clone, Serialize)]
pub struct BridgeAuthOutput {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "authUrl")]
    pub auth_url: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(HashMap::new()),
        required: Some(vec![]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for BridgeAuthenticator {
    fn name(&self) -> &str {
        "McpAuth"
    }
    fn description(&self) -> &str {
        "Authenticate with an MCP server via OAuth"
    }
    fn tool_type(&self) -> ToolType {
        ToolType::Mcp
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

    async fn execute(
        &self,
        _input: Value,
        _context: &ToolUseContext,
    ) -> anyhow::Result<ToolResult> {
        let output = BridgeAuthOutput {
            status: "unsupported".to_string(),
            message: "MCP OAuth authentication is not yet implemented in this build.".to_string(),
            auth_url: None,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
