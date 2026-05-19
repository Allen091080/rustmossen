//! # cost_query — MeterQuery 工具
//!
//! 对应 TS `commands/cost`。查询当前会话的成本和用量。

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 计量查询 — 查询当前会话的消耗信息。
pub struct MeterQuery;

fn build_input_schema() -> ToolInputSchema {
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(HashMap::new()),
        required: None,
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for MeterQuery {
    fn name(&self) -> &str {
        "CostQuery"
    }

    fn description(&self) -> &str {
        "Show the total cost and duration of the current session"
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
        // 成本信息由 agent 层的 CostTracker 维护；
        // 此工具充当查询接口，实际数据注入由上层编排器完成。
        let output = serde_json::json!({
            "message": "Cost information is managed by the session orchestrator.",
            "total_cost_usd": 0.0,
            "total_input_tokens": 0,
            "total_output_tokens": 0,
        });

        Ok(ToolResult {
            output: output.to_string(),
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
