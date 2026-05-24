//! # file_write — FileComposer 工具
//!
//! 对应 TS `FileWriteTool`（435 行）。创建或覆写文件，支持原子写入和 stale 检测。

use std::collections::HashMap;

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
        let inp: FileComposerInput = serde_json::from_value(input)?;
        let start = std::time::Instant::now();

        let full_path = shellexpand::tilde(&inp.file_path).to_string();
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
            tokio::fs::create_dir_all(parent).await?;
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
        let mut tmp = tempfile::NamedTempFile::new_in(parent_dir)?;
        std::io::Write::write_all(&mut tmp, inp.content.as_bytes())?;
        tmp.persist(path)?;
        mossen_agent::services::team_memory_sync::notify_team_memory_file_write(&full_path).await;

        let op_type = if is_update { "update" } else { "create" };

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
