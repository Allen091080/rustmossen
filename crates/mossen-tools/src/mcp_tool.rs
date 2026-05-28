//! # mcp_tool — BridgeExecutor 工具
//!
//! 对应 TS `MCPTool`（78 行）。通用 MCP 工具执行器，schema 动态注入。

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 桥接执行器 — MCP 通用工具代理。
pub struct BridgeExecutor;

fn build_input_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(HashMap::new()),
        required: Some(vec![]),
        extra: {
            let mut extra = HashMap::new();
            extra.insert("additionalProperties".to_string(), serde_json::json!(true));
            extra
        },
    }
}

#[async_trait]
impl Tool for BridgeExecutor {
    fn name(&self) -> &str {
        "mcp"
    }
    fn description(&self) -> &str {
        "Execute an MCP tool"
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
        // Stub: real MCP tool execution is handled by the MCP client layer.
        Ok(ToolResult {
            output: String::new(),
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
