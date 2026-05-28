//! # todo — TaskNotePad 工具
//!
//! 对应 TS `TodoWriteTool`。管理会话级 todo 列表。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn todo_store() -> &'static Mutex<Vec<TodoItem>> {
    static STORE: OnceLock<Mutex<Vec<TodoItem>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
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

fn parse_input(input: Value) -> Result<TaskNotePadInput, String> {
    match input {
        Value::Null => {
            Err("TodoWrite requires a JSON object with a `todos` array; received null.".to_string())
        }
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!(
                "TodoWrite received invalid input: {error}. Expected object: {{\"todos\":[...]}}."
            )
        }),
        other => Err(format!(
            "TodoWrite requires a JSON object with a `todos` array; received {}.",
            other
        )),
    }
}

fn error_result(message: impl Into<String>) -> anyhow::Result<ToolResult> {
    let output = TaskNotePadOutput {
        old_todos: todo_store().lock().unwrap().clone(),
        new_todos: Vec::new(),
        error: Some(message.into()),
    };
    Ok(ToolResult {
        output: serde_json::to_string(&output)?,
        is_error: true,
        duration_ms: 0,
        metadata: HashMap::new(),
    })
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
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => return error_result(message),
        };
        for todo in &inp.todos {
            if todo.id.trim().is_empty() {
                return error_result("TodoWrite requires each todo to have a non-empty `id`.");
            }
            if todo.content.trim().is_empty() {
                return error_result("TodoWrite requires each todo to have non-empty `content`.");
            }
            if !matches!(
                todo.status.as_str(),
                "pending" | "in_progress" | "completed"
            ) {
                return error_result(format!(
                    "Invalid todo status for {}: {}",
                    todo.id, todo.status
                ));
            }
        }

        let mut store = todo_store().lock().unwrap();
        let old_todos = store.clone();
        *store = inp.todos;
        let output = TaskNotePadOutput {
            old_todos,
            new_todos: store.clone(),
            error: None,
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
pub(crate) fn clear_todos_for_test() {
    todo_store().lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn todo_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("todo test lock poisoned")
    }

    #[tokio::test]
    async fn todo_write_preserves_previous_todos_between_calls() {
        let _lock = todo_test_lock();
        clear_todos_for_test();
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let first = TaskNotePad
            .execute(
                serde_json::json!({
                    "todos": [
                        {"id": "1", "content": "inspect", "status": "pending"}
                    ]
                }),
                &context,
            )
            .await
            .expect("first todo write");
        assert!(!first.is_error);
        let first_output: serde_json::Value =
            serde_json::from_str(&first.output).expect("first json");
        assert_eq!(first_output["old_todos"].as_array().unwrap().len(), 0);

        let second = TaskNotePad
            .execute(
                serde_json::json!({
                    "todos": [
                        {"id": "1", "content": "inspect", "status": "completed"}
                    ]
                }),
                &context,
            )
            .await
            .expect("second todo write");
        assert!(!second.is_error);
        let second_output: serde_json::Value =
            serde_json::from_str(&second.output).expect("second json");
        assert_eq!(second_output["old_todos"][0]["status"], "pending");
        assert_eq!(second_output["new_todos"][0]["status"], "completed");

        clear_todos_for_test();
    }

    #[tokio::test]
    async fn todo_write_null_input_returns_structured_tool_error() {
        let _lock = todo_test_lock();
        clear_todos_for_test();
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = TaskNotePad
            .execute(serde_json::Value::Null, &context)
            .await
            .expect("todo write");
        let output: serde_json::Value = serde_json::from_str(&result.output).expect("json");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("todos"));
        assert_eq!(output["new_todos"].as_array().unwrap().len(), 0);
        clear_todos_for_test();
    }

    #[tokio::test]
    async fn todo_write_empty_content_returns_structured_tool_error() {
        let _lock = todo_test_lock();
        clear_todos_for_test();
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = TaskNotePad
            .execute(
                serde_json::json!({
                    "todos": [
                        {"id": "1", "content": "", "status": "pending"}
                    ]
                }),
                &context,
            )
            .await
            .expect("todo write");
        let output: serde_json::Value = serde_json::from_str(&result.output).expect("json");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("content"));
        clear_todos_for_test();
    }
}
