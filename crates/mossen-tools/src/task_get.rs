//! # task_get — WorkItemQuery 工具
//!
//! 对应 TS `TaskGetTool`（129 行）。按 ID 查询任务详情。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 工作项查询 — 按 ID 获取任务。
pub struct WorkItemQuery;

#[derive(Debug, Clone, Deserialize)]
pub struct WorkItemQueryInput {
    #[serde(rename = "taskId")]
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkItemQueryOutput {
    pub task: Option<TaskDetail>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDetail {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: String,
    pub blocks: Vec<String>,
    #[serde(rename = "blockedBy")]
    pub blocked_by: Vec<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "taskId".to_string(),
        serde_json::json!({
            "type": "string", "description": "The ID of the task to retrieve"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["taskId".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for WorkItemQuery {
    fn name(&self) -> &str {
        "TaskGet"
    }
    fn description(&self) -> &str {
        "Retrieve a task by its ID"
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
        let inp: WorkItemQueryInput = serde_json::from_value(input)?;
        let output = WorkItemQueryOutput {
            task: crate::task_store::get_task(&inp.task_id).map(|r| TaskDetail {
                id: r.id,
                subject: r.subject,
                description: r.description,
                status: r.status,
                blocks: r.blocks,
                blocked_by: r.blocked_by,
            }),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
