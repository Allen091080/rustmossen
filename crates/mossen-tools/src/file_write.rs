//! # file_write — FileComposer 工具
//!
//! 对应 TS `FileWriteTool`（435 行）。创建或覆写文件，支持原子写入和 stale 检测。

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 文件编写器 — 创建或覆写文件。
pub struct FileComposer;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct FileComposerInput {
    /// 文件绝对路径。
    pub file_path: String,
    /// 写入内容。
    pub content: String,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct FileComposerOutput {
    /// 操作类型 ("create" | "update")。
    #[serde(rename = "type")]
    pub op_type: String,
    /// 文件路径。
    #[serde(rename = "filePath")]
    pub file_path: String,
}

fn resolve_tool_path(path: &str, cwd: &str) -> String {
    let expanded = shellexpand::tilde(path).to_string();
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path.to_string_lossy().to_string()
    } else {
        PathBuf::from(cwd).join(path).to_string_lossy().to_string()
    }
}

fn tool_error(message: impl Into<String>, duration_ms: u64) -> ToolResult {
    ToolResult {
        output: message.into(),
        is_error: true,
        duration_ms,
        metadata: HashMap::new(),
    }
}

fn parse_input(input: Value) -> Result<FileComposerInput, String> {
    match input {
        Value::Null => Err(
            "Write requires a JSON object with `file_path` and `content`; received null."
                .to_string(),
        ),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("Write received invalid input: {error}. Expected object: {{\"file_path\":\"...\",\"content\":\"...\"}}.")
        }),
        other => Err(format!(
            "Write requires a JSON object with `file_path` and `content`; received {}.",
            other
        )),
    }
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "file_path".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The absolute path to the file to write (must be absolute, not relative)"
        }),
    );
    properties.insert(
        "content".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The content to write to the file"
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["file_path".to_string(), "content".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for FileComposer {
    fn name(&self) -> &str {
        "Write"
    }

    fn description(&self) -> &str {
        "Write a file to the local filesystem. Creates or overwrites the file."
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
            Err(message) => return Ok(tool_error(message, start.elapsed().as_millis() as u64)),
        };
        if inp.file_path.trim().is_empty() {
            return Ok(tool_error(
                "Write requires a non-empty `file_path` string.",
                start.elapsed().as_millis() as u64,
            ));
        }

        let full_path = resolve_tool_path(&inp.file_path, &context.cwd);
        let path = std::path::Path::new(&full_path);
        if let Some(message) =
            mossen_agent::services::team_memory_sync::check_team_mem_secrets(path, &inp.content)
        {
            return Ok(ToolResult {
                output: message,
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            if let Err(error) = tokio::fs::create_dir_all(parent).await {
                return Ok(tool_error(
                    format!(
                        "Cannot create parent directory for {}: {error}",
                        inp.file_path
                    ),
                    start.elapsed().as_millis() as u64,
                ));
            }
        }

        // Determine if this is a create or update.
        let is_update = path.exists();

        info!(
            file_path = %inp.file_path,
            is_update = is_update,
            "FileComposer: writing file"
        );

        // Atomic write via temp file + rename.
        let parent_dir = path.parent().unwrap_or(std::path::Path::new("."));
        let mut tmp = match tempfile::NamedTempFile::new_in(parent_dir) {
            Ok(tmp) => tmp,
            Err(error) => {
                return Ok(tool_error(
                    format!("Cannot prepare atomic write for {}: {error}", inp.file_path),
                    start.elapsed().as_millis() as u64,
                ))
            }
        };
        if let Err(error) = std::io::Write::write_all(&mut tmp, inp.content.as_bytes()) {
            return Ok(tool_error(
                format!("Cannot write temporary file for {}: {error}", inp.file_path),
                start.elapsed().as_millis() as u64,
            ));
        }
        if let Err(error) = tmp.persist(path) {
            return Ok(tool_error(
                format!("Cannot persist file {}: {}", inp.file_path, error.error),
                start.elapsed().as_millis() as u64,
            ));
        }
        mossen_agent::services::team_memory_sync::notify_team_memory_file_write(&full_path).await;

        let op_type = if is_update { "update" } else { "create" };
        crate::task_hooks::file_changed(
            context,
            &full_path,
            if is_update { "change" } else { "add" },
        )
        .await;

        let output = FileComposerOutput {
            op_type: op_type.to_string(),
            file_path: inp.file_path,
        };

        let result_msg = match op_type {
            "create" => format!("File created successfully at: {}", output.file_path),
            _ => format!(
                "The file {} has been updated successfully.",
                output.file_path
            ),
        };
        let metadata = crate::skill_discovery::observe_tool_file_paths(
            [output.file_path.as_str()],
            &context.cwd,
        )
        .await
        .to_metadata();

        Ok(ToolResult {
            output: result_msg,
            is_error: false,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FileComposer;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use mossen_utils::hooks_utils::{
        register_runtime_hooks_context, unregister_runtime_hooks_context, HookMatcher, HooksContext,
    };
    use serde_json::Value;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::sync::Arc;

    fn context(cwd: &std::path::Path) -> ToolUseContext {
        ToolUseContext {
            cwd: cwd.to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
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
        command: String,
    ) -> (ToolUseContext, HookRegistration) {
        let mut registered_hooks = HashMap::new();
        registered_hooks.insert(
            "FileChanged".to_string(),
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
            session_id: "file-write-hook-test".to_string(),
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
        let mut context = context(cwd);
        context.extra.insert(
            crate::task_hooks::HOOK_CONTEXT_ID_EXTRA_KEY.to_string(),
            serde_json::json!(id.clone()),
        );
        (context, HookRegistration { id })
    }

    #[tokio::test]
    async fn write_null_input_returns_structured_tool_error() {
        let temp = tempfile::tempdir().expect("tempdir");

        let result = FileComposer
            .execute(Value::Null, &context(temp.path()))
            .await
            .expect("write result");

        assert!(result.is_error);
        assert!(result.output.contains("file_path"), "{}", result.output);
        assert!(result.output.contains("null"), "{}", result.output);
    }

    #[tokio::test]
    async fn write_relative_path_resolves_against_tool_context_cwd() {
        let temp = tempfile::tempdir().expect("tempdir");

        let result = FileComposer
            .execute(
                serde_json::json!({
                    "file_path": "nested/output.txt",
                    "content": "context-cwd-write"
                }),
                &context(temp.path()),
            )
            .await
            .expect("write result");

        assert!(!result.is_error, "{}", result.output);
        let written = std::fs::read_to_string(temp.path().join("nested/output.txt"))
            .expect("relative file was written under cwd");
        assert_eq!(written, "context-cwd-write");
    }

    #[tokio::test]
    async fn write_executes_file_changed_hook() {
        let temp = tempfile::tempdir().expect("tempdir");
        let marker_path = temp.path().join("file_changed_marker");
        let marker_arg = marker_path.to_string_lossy().replace('\'', "'\\''");
        let (context, _registration) =
            hooked_context(temp.path(), format!("printf file-changed > '{marker_arg}'"));

        let result = FileComposer
            .execute(
                serde_json::json!({
                    "file_path": "output.txt",
                    "content": "hooked-write"
                }),
                &context,
            )
            .await
            .expect("write result");

        assert!(!result.is_error, "{}", result.output);
        assert_eq!(
            tokio::fs::read_to_string(marker_path)
                .await
                .expect("FileChanged marker"),
            "file-changed"
        );
    }
}
