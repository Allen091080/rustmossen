//! # task_create — WorkItemForge 工具
//!
//! 对应 TS `TaskCreateTool`（139 行）。创建新的工作任务。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 工作项铸造器 — 创建新任务。
pub struct WorkItemForge;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct WorkItemForgeInput {
    /// 任务简要标题。
    pub subject: String,
    /// 任务详细描述。
    pub description: String,
    /// 进行中时 spinner 显示的现在分词形式。
    #[serde(default)]
    pub active_form: Option<String>,
    /// 附加到任务的元数据。
    #[serde(default)]
    pub metadata: Option<HashMap<String, Value>>,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct WorkItemForgeOutput {
    pub task: TaskRef,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskRef {
    pub id: String,
    pub subject: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "subject".to_string(),
        serde_json::json!({
            "type": "string", "description": "A brief title for the task"
        }),
    );
    properties.insert(
        "description".to_string(),
        serde_json::json!({
            "type": "string", "description": "What needs to be done"
        }),
    );
    properties.insert("activeForm".to_string(), serde_json::json!({
        "type": "string",
        "description": "Present continuous form shown in spinner when in_progress (e.g., 'Running tests')"
    }));
    properties.insert(
        "metadata".to_string(),
        serde_json::json!({
            "type": "object", "description": "Arbitrary metadata to attach to the task",
            "additionalProperties": true
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["subject".to_string(), "description".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for WorkItemForge {
    fn name(&self) -> &str {
        "TaskCreate"
    }
    fn description(&self) -> &str {
        "Create a new task in the task list"
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
        let inp: WorkItemForgeInput = serde_json::from_value(input)?;
        let mut record = crate::task_store::create_task(inp.subject.clone(), inp.description);
        if let Some(af) = inp.active_form {
            crate::task_store::update_task(&record.id, |r| r.active_form = Some(af.clone()));
            record.active_form = Some(af);
        }
        if let Some(meta) = inp.metadata {
            crate::task_store::update_task(&record.id, |r| r.metadata = meta.clone());
            record.metadata = meta;
        }

        let output = WorkItemForgeOutput {
            task: TaskRef {
                id: record.id,
                subject: record.subject,
            },
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
