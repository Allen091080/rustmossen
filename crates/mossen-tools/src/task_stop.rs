//! # task_stop — HaltSignal 工具
//!
//! 对应 TS `TaskStopTool`（132 行）。停止运行中的后台任务。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 停止信号 — 中止后台任务。
pub struct HaltSignal;

#[derive(Debug, Clone, Deserialize)]
pub struct HaltSignalInput {
    #[serde(default)]
    pub task_id: Option<String>,
    /// 兼容旧版 KillShell。
    #[serde(default)]
    pub shell_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HaltSignalOutput {
    pub message: String,
    pub task_id: String,
    pub task_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "task_id".to_string(),
        serde_json::json!({
            "type": "string", "description": "The ID of the background task to stop"
        }),
    );
    properties.insert(
        "shell_id".to_string(),
        serde_json::json!({
            "type": "string", "description": "Deprecated: use task_id instead"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec![]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for HaltSignal {
    fn name(&self) -> &str {
        "TaskStop"
    }
    fn description(&self) -> &str {
        "Stop a running background task by ID"
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
        let inp: HaltSignalInput = serde_json::from_value(input)?;
        let id = inp
            .task_id
            .or(inp.shell_id)
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: task_id"))?;

        match crate::task_store::stop_background_task(&id) {
            Some(record) => {
                let task_type = record
                    .metadata
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("workitem")
                    .to_string();
                let command = record
                    .metadata
                    .get("command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let output = HaltSignalOutput {
                    message: format!("Successfully stopped task: {id}"),
                    task_id: id,
                    task_type,
                    command,
                };
                Ok(ToolResult {
                    output: serde_json::to_string(&output)?,
                    is_error: false,
                    duration_ms: 0,
                    metadata: HashMap::new(),
                })
            }
            None => {
                let output = HaltSignalOutput {
                    message: format!("Task not found: {id}"),
                    task_id: id,
                    task_type: "unknown".to_string(),
                    command: None,
                };
                Ok(ToolResult {
                    output: serde_json::to_string(&output)?,
                    is_error: true,
                    duration_ms: 0,
                    metadata: HashMap::new(),
                })
            }
        }
    }
}
