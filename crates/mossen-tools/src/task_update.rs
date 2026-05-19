//! # task_update — WorkItemMutator 工具
//!
//! 对应 TS `TaskUpdateTool`（407 行）。更新任务的状态、描述等字段。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 工作项变更器 — 更新任务属性。
pub struct WorkItemMutator;

#[derive(Debug, Clone, Deserialize)]
pub struct WorkItemMutatorInput {
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "activeForm")]
    pub active_form: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default, rename = "addBlocks")]
    pub add_blocks: Option<Vec<String>>,
    #[serde(default, rename = "addBlockedBy")]
    pub add_blocked_by: Option<Vec<String>>,
    #[serde(default)]
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkItemMutatorOutput {
    pub success: bool,
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(rename = "updatedFields")]
    pub updated_fields: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "taskId".to_string(),
        serde_json::json!({
            "type": "string", "description": "The ID of the task to update"
        }),
    );
    properties.insert(
        "subject".to_string(),
        serde_json::json!({
            "type": "string", "description": "New subject for the task"
        }),
    );
    properties.insert(
        "description".to_string(),
        serde_json::json!({
            "type": "string", "description": "New description for the task"
        }),
    );
    properties.insert(
        "status".to_string(),
        serde_json::json!({
            "type": "string", "description": "New status for the task",
            "enum": ["pending", "in_progress", "completed", "deleted"]
        }),
    );
    properties.insert("activeForm".to_string(), serde_json::json!({
        "type": "string", "description": "Present continuous form shown in spinner when in_progress"
    }));
    properties.insert(
        "owner".to_string(),
        serde_json::json!({
            "type": "string", "description": "New owner for the task"
        }),
    );
    properties.insert("addBlocks".to_string(), serde_json::json!({
        "type": "array", "items": { "type": "string" }, "description": "Task IDs that this task blocks"
    }));
    properties.insert("addBlockedBy".to_string(), serde_json::json!({
        "type": "array", "items": { "type": "string" }, "description": "Task IDs that block this task"
    }));
    properties.insert(
        "metadata".to_string(),
        serde_json::json!({
            "type": "object", "description": "Metadata keys to merge into the task",
            "additionalProperties": true
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
impl Tool for WorkItemMutator {
    fn name(&self) -> &str {
        "TaskUpdate"
    }
    fn description(&self) -> &str {
        "Update a task's status, description, or other fields"
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
        let inp: WorkItemMutatorInput = serde_json::from_value(input)?;
        let task_id = inp.task_id.clone();
        let mut updated_fields = Vec::new();
        let applied = crate::task_store::update_task(&inp.task_id, |r| {
            if let Some(v) = inp.subject {
                r.subject = v;
                updated_fields.push("subject".to_string());
            }
            if let Some(v) = inp.description {
                r.description = v;
                updated_fields.push("description".to_string());
            }
            if let Some(v) = inp.status {
                r.status = v;
                updated_fields.push("status".to_string());
            }
            if let Some(v) = inp.active_form {
                r.active_form = Some(v);
                updated_fields.push("activeForm".to_string());
            }
            if let Some(v) = inp.owner {
                r.owner = Some(v);
                updated_fields.push("owner".to_string());
            }
            if let Some(v) = inp.add_blocks {
                r.blocks.extend(v);
                updated_fields.push("addBlocks".to_string());
            }
            if let Some(v) = inp.add_blocked_by {
                r.blocked_by.extend(v);
                updated_fields.push("addBlockedBy".to_string());
            }
            if let Some(v) = inp.metadata {
                for (k, val) in v {
                    if val.is_null() {
                        r.metadata.remove(&k);
                    } else {
                        r.metadata.insert(k, val);
                    }
                }
                updated_fields.push("metadata".to_string());
            }
        });

        let output = if applied.is_some() {
            WorkItemMutatorOutput {
                success: true,
                task_id,
                updated_fields,
                error: None,
            }
        } else {
            WorkItemMutatorOutput {
                success: false,
                task_id,
                updated_fields: Vec::new(),
                error: Some("task not found".to_string()),
            }
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: !output.success,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
