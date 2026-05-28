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

fn parse_input(input: Value) -> Result<WorkItemMutatorInput, String> {
    match input {
        Value::Null => Err(
            "TaskUpdate requires a JSON object with a `taskId` string; received null.".to_string(),
        ),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("TaskUpdate received invalid input: {error}. Expected object: {{\"taskId\":\"...\"}}.")
        }),
        other => Err(format!(
            "TaskUpdate requires a JSON object with a `taskId` string; received {}.",
            other
        )),
    }
}

fn error_output(task_id: impl Into<String>, message: impl Into<String>) -> WorkItemMutatorOutput {
    WorkItemMutatorOutput {
        success: false,
        task_id: task_id.into(),
        updated_fields: Vec::new(),
        error: Some(message.into()),
    }
}

fn valid_status(status: &str) -> bool {
    matches!(status, "pending" | "in_progress" | "completed" | "deleted")
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

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let start = std::time::Instant::now();
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => {
                return Ok(ToolResult {
                    output: serde_json::to_string(&error_output("", message))?,
                    is_error: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                    metadata: HashMap::new(),
                })
            }
        };
        if inp.task_id.trim().is_empty() {
            return Ok(ToolResult {
                output: serde_json::to_string(&error_output(
                    "",
                    "TaskUpdate requires a non-empty `taskId` string.",
                ))?,
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }
        if let Some(status) = inp.status.as_deref() {
            if !valid_status(status) {
                return Ok(ToolResult {
                    output: serde_json::to_string(&error_output(
                        &inp.task_id,
                        format!(
                            "Unsupported task status: {status}. Expected pending, in_progress, completed, or deleted."
                        ),
                    ))?,
                    is_error: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                    metadata: HashMap::new(),
                });
            }
        }
        let completed_hook = if inp.status.as_deref() == Some("completed") {
            if let Some(task) = crate::task_store::get_task(&inp.task_id) {
                if task.status != "completed" {
                    crate::task_hooks::task_completed(
                        context,
                        &task.id,
                        &task.subject,
                        Some(task.description.as_str()),
                    )
                    .await
                } else {
                    crate::task_hooks::TaskHookOutcome::default()
                }
            } else {
                crate::task_hooks::TaskHookOutcome::default()
            }
        } else {
            crate::task_hooks::TaskHookOutcome::default()
        };
        if let Some(message) = completed_hook.block_message {
            let output = error_output(&inp.task_id, message);
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }
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
        let mut output_text = serde_json::to_string(&output)?;
        crate::task_hooks::append_additional_contexts(
            &mut output_text,
            "TaskCompleted additional context",
            &completed_hook.additional_contexts,
        );
        Ok(ToolResult {
            output: output_text,
            is_error: !output.success,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WorkItemMutator;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use mossen_utils::hooks_utils::{
        register_runtime_hooks_context, unregister_runtime_hooks_context, HookMatcher, HooksContext,
    };
    use serde_json::Value;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    fn context() -> ToolUseContext {
        ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: Default::default(),
        }
    }

    struct HookRegistration {
        id: String,
    }

    impl Drop for HookRegistration {
        fn drop(&mut self) {
            unregister_runtime_hooks_context(&self.id);
        }
    }

    fn hooked_context(
        cwd: &std::path::Path,
        event: &str,
        command: String,
    ) -> (ToolUseContext, HookRegistration) {
        let mut registered_hooks = HashMap::new();
        registered_hooks.insert(
            event.to_string(),
            vec![HookMatcher {
                matcher: None,
                hooks: vec![serde_json::json!({
                    "type": "command",
                    "command": command,
                    "timeout": 1
                })],
                plugin_root: None,
                plugin_id: None,
                plugin_name: None,
                skill_root: None,
                skill_name: None,
            }],
        );
        let hooks_context = Arc::new(HooksContext {
            session_id: "task-update-test".to_string(),
            original_cwd: cwd.to_string_lossy().to_string(),
            project_root: cwd.to_string_lossy().to_string(),
            is_non_interactive: true,
            trust_accepted: true,
            hooks_config_snapshot: None,
            registered_hooks: Some(registered_hooks),
            disable_all_hooks: false,
            managed_hooks_only: false,
            main_thread_agent_type: Some("main".to_string()),
            custom_backend_enabled: false,
            simple_mode: false,
            get_transcript_path: Arc::new(|session_id| format!("/tmp/{session_id}.jsonl")),
            get_agent_transcript_path: Arc::new(|agent_id| format!("/tmp/agent-{agent_id}.jsonl")),
            log_debug: Arc::new(|_| {}),
            log_error: Arc::new(|_| {}),
            log_event: Arc::new(|_, _| {}),
            get_settings: Arc::new(|| None),
            get_settings_for_source: Arc::new(|_| None),
            invalidate_session_env_cache: Arc::new(|| {}),
            dynamic_hook_executor: None,
            subprocess_env: std::env::vars().collect(),
            allowed_official_marketplace_names: HashSet::new(),
        });
        let id = register_runtime_hooks_context(hooks_context);
        let mut context = ToolUseContext {
            cwd: cwd.to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };
        context.extra.insert(
            crate::task_hooks::HOOK_CONTEXT_ID_EXTRA_KEY.to_string(),
            serde_json::json!(id.clone()),
        );
        (context, HookRegistration { id })
    }

    #[tokio::test]
    async fn task_update_null_input_returns_structured_tool_error() {
        let result = WorkItemMutator
            .execute(Value::Null, &context())
            .await
            .expect("TaskUpdate result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert_eq!(output["success"], false);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("taskId"));
    }

    #[tokio::test]
    async fn task_update_rejects_invalid_status_as_structured_tool_error() {
        let result = WorkItemMutator
            .execute(
                serde_json::json!({"taskId": "task-1", "status": "done"}),
                &context(),
            )
            .await
            .expect("TaskUpdate result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert_eq!(output["success"], false);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("Unsupported task status"));
    }

    #[tokio::test]
    async fn task_update_executes_task_completed_hook() {
        let _guard = crate::task_store::test_store_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let marker_path = temp.path().join("task_completed_marker");
        let marker_arg = marker_path.to_string_lossy().replace('\'', "'\\''");
        let (context, _hook_registration) = hooked_context(
            temp.path(),
            "TaskCompleted",
            format!("printf task-completed > '{marker_arg}'"),
        );
        let task = crate::task_store::create_task("ship".into(), "finish the thing".into());
        let task_id = task.id.clone();

        let result = WorkItemMutator
            .execute(
                serde_json::json!({"taskId": task_id, "status": "completed"}),
                &context,
            )
            .await
            .expect("TaskUpdate result");

        assert!(!result.is_error);
        assert_eq!(
            tokio::fs::read_to_string(marker_path)
                .await
                .expect("TaskCompleted marker"),
            "task-completed"
        );
    }

    #[tokio::test]
    async fn task_update_blocking_hook_prevents_completion() {
        let _guard = crate::task_store::test_store_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let (context, _hook_registration) = hooked_context(
            temp.path(),
            "TaskCompleted",
            "printf 'blocked completion' >&2; exit 2".to_string(),
        );
        let task = crate::task_store::create_task("ship".into(), "finish the thing".into());
        let task_id = task.id.clone();

        let result = WorkItemMutator
            .execute(
                serde_json::json!({"taskId": task_id, "status": "completed"}),
                &context,
            )
            .await
            .expect("TaskUpdate result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");
        let stored = crate::task_store::get_task(&task.id).expect("stored task");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("TaskCompleted hook feedback"));
        assert_eq!(stored.status, "pending");
    }
}
