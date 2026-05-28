//! # exit — SessionExit 工具
//!
//! 对应 TS `commands/exit`。退出当前 REPL 会话。

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 会话退出 — 终止当前会话。
pub struct SessionExit;

fn build_input_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(HashMap::new()),
        required: None,
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for SessionExit {
    fn name(&self) -> &str {
        "Exit"
    }

    fn description(&self) -> &str {
        "Exit the current REPL session"
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

    async fn execute(
        &self,
        _input: Value,
        _context: &ToolUseContext,
    ) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            output: serde_json::json!({ "status": "exiting" }).to_string(),
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
