//! # cron_create — ScheduleForge 工具
//!
//! 对应 TS `CronCreateTool`（158 行）。创建定时/一次性调度任务。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 调度铸造 — 创建 cron 定时任务。
pub struct ScheduleForge;

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleForgeInput {
    /// 标准 5 字段 cron 表达式。
    pub cron: String,
    /// 触发时执行的 prompt。
    pub prompt: String,
    /// true=周期性，false=一次性。
    #[serde(default = "default_recurring")]
    pub recurring: bool,
    /// true=持久化到磁盘，false=仅当前会话。
    #[serde(default)]
    pub durable: bool,
}

fn default_recurring() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleForgeOutput {
    pub id: String,
    #[serde(rename = "humanSchedule")]
    pub human_schedule: String,
    pub recurring: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durable: Option<bool>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "cron".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Standard 5-field cron expression: M H DoM Mon DoW"
        }),
    );
    properties.insert(
        "prompt".to_string(),
        serde_json::json!({
            "type": "string", "description": "The prompt to enqueue at each fire time"
        }),
    );
    properties.insert(
        "recurring".to_string(),
        serde_json::json!({
            "type": "boolean", "description": "true = recurring, false = one-shot", "default": true
        }),
    );
    properties.insert("durable".to_string(), serde_json::json!({
        "type": "boolean", "description": "true = persist to disk, false = session-only", "default": false
    }));
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["cron".to_string(), "prompt".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for ScheduleForge {
    fn name(&self) -> &str {
        "CronCreate"
    }
    fn description(&self) -> &str {
        "Schedule a recurring or one-shot prompt"
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
        let inp: ScheduleForgeInput = serde_json::from_value(input)?;
        let id = uuid::Uuid::new_v4().to_string();
        let output = ScheduleForgeOutput {
            id,
            human_schedule: inp.cron.clone(),
            recurring: inp.recurring,
            durable: Some(inp.durable),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
