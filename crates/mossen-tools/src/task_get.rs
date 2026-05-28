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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

fn parse_input(input: Value) -> Result<WorkItemQueryInput, String> {
    match input {
        Value::Null => {
            Err("TaskGet requires a JSON object with a `taskId` string; received null.".to_string())
        }
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!(
                "TaskGet received invalid input: {error}. Expected object: {{\"taskId\":\"...\"}}."
            )
        }),
        other => Err(format!(
            "TaskGet requires a JSON object with a `taskId` string; received {}.",
            other
        )),
    }
}

fn error_output(message: impl Into<String>) -> WorkItemQueryOutput {
    WorkItemQueryOutput {
        task: None,
        error: Some(message.into()),
    }
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
        let start = std::time::Instant::now();
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => {
                return Ok(ToolResult {
                    output: serde_json::to_string(&error_output(message))?,
                    is_error: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                    metadata: HashMap::new(),
                })
            }
        };
        if inp.task_id.trim().is_empty() {
            return Ok(ToolResult {
                output: serde_json::to_string(&error_output(
                    "TaskGet requires a non-empty `taskId` string.",
                ))?,
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }
        let output = WorkItemQueryOutput {
            task: crate::task_store::get_task(&inp.task_id).map(|r| TaskDetail {
                id: r.id,
                subject: r.subject,
                description: r.description,
                status: r.status,
                blocks: r.blocks,
                blocked_by: r.blocked_by,
            }),
            error: None,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WorkItemQuery;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use serde_json::Value;

    fn context() -> ToolUseContext {
        ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: Default::default(),
        }
    }

    #[tokio::test]
    async fn task_get_null_input_returns_structured_tool_error() {
        let result = WorkItemQuery
            .execute(Value::Null, &context())
            .await
            .expect("TaskGet result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("taskId"));
    }

    #[tokio::test]
    async fn task_get_empty_id_returns_structured_tool_error() {
        let result = WorkItemQuery
            .execute(serde_json::json!({"taskId": ""}), &context())
            .await
            .expect("TaskGet result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("non-empty"));
    }
}
