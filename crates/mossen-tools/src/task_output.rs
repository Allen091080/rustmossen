//! # task_output — ResultEmitter 工具
//!
//! 对应 TS `TaskOutputTool`（515 行）。获取后台任务的输出。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 结果发射器 — 获取后台任务输出。
pub struct ResultEmitter;

#[derive(Debug, Clone, Deserialize)]
pub struct ResultEmitterInput {
    /// 任务 ID。
    pub task_id: String,
    /// 是否阻塞等待完成。
    #[serde(default = "default_block")]
    pub block: bool,
    /// 最大等待时间（毫秒）。
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_block() -> bool {
    true
}
fn default_timeout() -> u64 {
    30_000
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultEmitterOutput {
    pub retrieval_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskOutput>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskOutput {
    pub task_id: String,
    pub task_type: String,
    pub status: String,
    pub description: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "task_id".to_string(),
        serde_json::json!({
            "type": "string", "description": "The task ID to get output from"
        }),
    );
    properties.insert(
        "block".to_string(),
        serde_json::json!({
            "type": "boolean", "description": "Whether to wait for completion", "default": true
        }),
    );
    properties.insert(
        "timeout".to_string(),
        serde_json::json!({
            "type": "number", "description": "Max wait time in ms", "default": 30000,
            "minimum": 0, "maximum": 600000
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["task_id".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for ResultEmitter {
    fn name(&self) -> &str {
        "TaskOutput"
    }
    fn description(&self) -> &str {
        "Get the output of a background task"
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
        let inp: ResultEmitterInput = serde_json::from_value(input)?;
        let output = match crate::task_store::get_task(&inp.task_id) {
            None => ResultEmitterOutput {
                retrieval_status: "not_found".to_string(),
                task: None,
            },
            Some(r) => {
                // For background-agent tasks the store carries `output`
                // and `exit_code` once the agent finishes; for plain
                // workitems they stay empty. Map status → retrieval to
                // match the TS contract: terminal statuses become "ready".
                let retrieval = match r.status.as_str() {
                    "completed" | "deleted" => "ready",
                    _ => "not_ready",
                }
                .to_string();
                ResultEmitterOutput {
                    retrieval_status: retrieval,
                    task: Some(TaskOutput {
                        task_id: r.id,
                        task_type: r
                            .metadata
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("workitem")
                            .to_string(),
                        status: r.status,
                        description: r.description,
                        output: r.output,
                        exit_code: r.exit_code,
                    }),
                }
            }
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
