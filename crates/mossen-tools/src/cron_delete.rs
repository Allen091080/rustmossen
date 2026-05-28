//! # cron_delete — ScheduleRevoke 工具
//!
//! 对应 TS `CronDeleteTool`（96 行）。取消调度任务。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 调度撤销 — 取消 cron 任务。
pub struct ScheduleRevoke;

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleRevokeInput {
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleRevokeOutput {
    pub id: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "id".to_string(),
        serde_json::json!({
            "type": "string", "description": "Job ID returned by CronCreate"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["id".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for ScheduleRevoke {
    fn name(&self) -> &str {
        "CronDelete"
    }
    fn description(&self) -> &str {
        "Cancel a scheduled cron job"
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
        let inp: ScheduleRevokeInput = serde_json::from_value(input)?;
        let output = ScheduleRevokeOutput { id: inp.id.clone() };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
