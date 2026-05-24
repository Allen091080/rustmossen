//! Todo types module — translated from utils/todo/types.ts

use serde::{Deserialize, Serialize};

/// Status of a todo item
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// A single todo item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// Content of the todo item (must not be empty)
    pub content: String,
    /// Current status
    pub status: TodoStatus,
    /// Active form text (must not be empty)
    pub active_form: String,
}

impl TodoItem {
    /// Validate that the todo item's fields are non-empty
    pub fn validate(&self) -> Result<(), String> {
        if self.content.is_empty() {
            return Err("Content cannot be empty".to_string());
        }
        if self.active_form.is_empty() {
            return Err("Active form cannot be empty".to_string());
        }
        Ok(())
    }
}

/// A list of todo items
pub type TodoList = Vec<TodoItem>;

/// Validate a todo list (all items must be valid)
pub fn validate_todo_list(list: &TodoList) -> Result<(), String> {
    for (i, item) in list.iter().enumerate() {
        item.validate().map_err(|e| format!("Item {}: {}", i, e))?;
    }
    Ok(())
}

/// Alias for the todo item validator (mirrors TS `TodoItemSchema`).
pub type TodoItemSchema = TodoItem;
/// Alias for the todo list validator (mirrors TS `TodoListSchema`).
pub type TodoListSchema = TodoList;
