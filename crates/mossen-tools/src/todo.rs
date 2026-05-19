//! # todo — TaskNotePad 工具
//!
//! 对应 TS `TodoWriteTool`。管理会话级 todo 列表。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 任务记事本 — 管理 todo 列表。
pub struct TaskNotePad;

/// 单条 todo 项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// 唯一标识。
    pub id: String,
    /// 内容描述。
    pub content: String,
    /// 状态：pending | in_progress | completed。
    pub status: String,
}

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct TaskNotePadInput {
    /// 更新后的 todo 列表。
    pub todos: Vec<TodoItem>,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct TaskNotePadOutput {
    pub old_todos: Vec<TodoItem>,
    pub new_todos: Vec<TodoItem>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "todos".to_string(),
        serde_json::json!({
            "type": "array",
            "description": "The updated todo list",
            "items": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Unique identifier" },
                    "content": { "type": "string", "description": "Task description" },
                    "status": {
                        "type": "string",
                        "enum": ["pending", "in_progress", "completed"],
                        "description": "Task status"
                    }
                },
                "required": ["id", "content", "status"]
            }
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["todos".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for TaskNotePad {
    fn name(&self) -> &str {
        "TodoWrite"
    }

    fn description(&self) -> &str {
        "Create and manage a todo list for tracking tasks"
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

    fn needs_permission(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: TaskNotePadInput = serde_json::from_value(input)?;

        // 当前实现：将输入 todos 直接返回为新列表。
        // 实际运行时需要与 AppState 交互（由上层编排处理）。
        let output = TaskNotePadOutput {
            old_todos: Vec::new(),
            new_todos: inp.todos,
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
