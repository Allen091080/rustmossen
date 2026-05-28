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
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskRef {
    pub id: String,
    pub subject: String,
}

fn parse_input(input: Value) -> Result<WorkItemForgeInput, String> {
    match input {
        Value::Null => Err(
            "TaskCreate requires a JSON object with `subject` and `description`; received null."
                .to_string(),
        ),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("TaskCreate received invalid input: {error}. Expected object: {{\"subject\":\"...\",\"description\":\"...\"}}.")
        }),
        other => Err(format!(
            "TaskCreate requires a JSON object with `subject` and `description`; received {}.",
            other
        )),
    }
}

fn error_result(message: impl Into<String>, duration_ms: u64) -> anyhow::Result<ToolResult> {
    let output = serde_json::json!({
        "success": false,
        "task": null,
        "error": message.into(),
    });
    Ok(ToolResult {
        output: serde_json::to_string(&output)?,
        is_error: true,
        duration_ms,
        metadata: HashMap::new(),
    })
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

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let start = std::time::Instant::now();
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => return error_result(message, start.elapsed().as_millis() as u64),
        };
        if inp.subject.trim().is_empty() {
            return error_result(
                "TaskCreate requires a non-empty `subject` string.",
                start.elapsed().as_millis() as u64,
            );
        }
        if inp.description.trim().is_empty() {
            return error_result(
                "TaskCreate requires a non-empty `description` string.",
                start.elapsed().as_millis() as u64,
            );
        }
        let task_id = uuid::Uuid::new_v4().to_string();
        let created_hook = crate::task_hooks::task_created(
            context,
            &task_id,
            &inp.subject,
            Some(inp.description.as_str()),
        )
        .await;
        if let Some(message) = created_hook.block_message {
            return error_result(message, start.elapsed().as_millis() as u64);
        }
        let mut record =
            crate::task_store::create_task_with_id(task_id, inp.subject.clone(), inp.description);
        if let Some(af) = inp.active_form {
            crate::task_store::update_task(&record.id, |r| r.active_form = Some(af.clone()));
            record.active_form = Some(af);
        }
        if let Some(meta) = inp.metadata {
            crate::task_store::update_task(&record.id, |r| r.metadata = meta.clone());
            record.metadata = meta;
        }

        let output = WorkItemForgeOutput {
            success: true,
            task: Some(TaskRef {
                id: record.id,
                subject: record.subject,
            }),
            error: None,
        };

        let mut output = serde_json::to_string(&output)?;
        crate::task_hooks::append_additional_contexts(
            &mut output,
            "TaskCreated additional context",
            &created_hook.additional_contexts,
        );

        Ok(ToolResult {
            output,
            is_error: false,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WorkItemForge;
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
            session_id: "task-create-test".to_string(),
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
    async fn task_create_null_input_returns_structured_tool_error() {
        let result = WorkItemForge
            .execute(Value::Null, &context())
            .await
            .expect("TaskCreate result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert_eq!(output["success"], false);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("subject"));
    }

    #[tokio::test]
    async fn task_create_empty_subject_returns_structured_tool_error() {
        let result = WorkItemForge
            .execute(
                serde_json::json!({"subject": "", "description": "desc"}),
                &context(),
            )
            .await
            .expect("TaskCreate result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert_eq!(output["success"], false);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("non-empty"));
    }

    #[tokio::test]
    async fn task_create_executes_task_created_hook() {
        let _guard = crate::task_store::test_store_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let marker_path = temp.path().join("task_created_marker");
        let marker_arg = marker_path.to_string_lossy().replace('\'', "'\\''");
        let (context, _hook_registration) = hooked_context(
            temp.path(),
            "TaskCreated",
            format!("printf task-created > '{marker_arg}'"),
        );

        let result = WorkItemForge
            .execute(
                serde_json::json!({"subject": "write docs", "description": "draft section"}),
                &context,
            )
            .await
            .expect("TaskCreate result");

        assert!(!result.is_error);
        assert_eq!(
            tokio::fs::read_to_string(marker_path)
                .await
                .expect("TaskCreated marker"),
            "task-created"
        );
        assert_eq!(crate::task_store::list_tasks().len(), 1);
    }

    #[tokio::test]
    async fn task_create_blocking_hook_prevents_insert() {
        let _guard = crate::task_store::test_store_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let (context, _hook_registration) = hooked_context(
            temp.path(),
            "TaskCreated",
            "printf 'blocked creation' >&2; exit 2".to_string(),
        );

        let result = WorkItemForge
            .execute(
                serde_json::json!({"subject": "blocked", "description": "do not insert"}),
                &context,
            )
            .await
            .expect("TaskCreate result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("TaskCreated hook feedback"));
        assert!(crate::task_store::list_tasks().is_empty());
    }
}
