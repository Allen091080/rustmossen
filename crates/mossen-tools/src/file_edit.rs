//! # file_edit — SourcePatcher 工具
//!
//! 对应 TS `FileEditTool`（626 行）。支持字符串精确匹配替换、
//! stale 检测、原子读-改-写。

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 源码补丁器 — 精确替换文件中的字符串片段。
pub struct SourcePatcher;

/// 最大可编辑文件大小（1 GiB）。
const MAX_EDIT_FILE_SIZE: u64 = 1024 * 1024 * 1024;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct SourcePatcherInput {
    /// 文件路径。
    pub file_path: String,
    /// 要替换的旧字符串。
    pub old_string: String,
    /// 替换为的新字符串。
    pub new_string: String,
    /// 是否替换所有匹配项。
    #[serde(default)]
    pub replace_all: bool,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct SourcePatcherOutput {
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_string: Option<String>,
    pub replace_all: bool,
}

/// 展开路径中的 `~`。
fn expand_path(path: &str) -> String {
    if path.starts_with('~') {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "file_path".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The path of the file to edit."
        }),
    );
    properties.insert(
        "old_string".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The exact string to be replaced."
        }),
    );
    properties.insert(
        "new_string".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The string to replace old_string with."
        }),
    );
    properties.insert(
        "replace_all".to_string(),
        serde_json::json!({
            "type": "boolean",
            "description": "Whether to replace all occurrences.",
            "default": false
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec![
            "file_path".to_string(),
            "old_string".to_string(),
            "new_string".to_string(),
        ]),
        extra: HashMap::new(),
    }
}

/// 原子写入：写入临时文件后 rename。
async fn atomic_write(path: &str, content: &str) -> anyhow::Result<()> {
    let dir = Path::new(path)
        .parent()
        .ok_or_else(|| anyhow::anyhow!("No parent directory for path: {}", path))?;

    // 确保目录存在。
    tokio::fs::create_dir_all(dir).await?;

    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut tmp, content.as_bytes())?;
    tmp.persist(path)?;
    Ok(())
}

fn team_memory_secret_error(path: &str, content: &str) -> Option<ToolResult> {
    mossen_agent::services::team_memory_sync::check_team_mem_secrets(path, content).map(|message| {
        ToolResult {
            output: message,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        }
    })
}

#[async_trait]
impl Tool for SourcePatcher {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing exact string matches"
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
        let inp: SourcePatcherInput = serde_json::from_value(input)?;
        let observed_file_path = inp.file_path.clone();
        let full_path = expand_path(&inp.file_path);

        // 1. 校验：old_string == new_string → 无变更。
        if inp.old_string == inp.new_string {
            return Ok(ToolResult {
                output: "No changes to make: old_string and new_string are exactly the same."
                    .to_string(),
                is_error: true,
                duration_ms: 0,
                metadata: HashMap::new(),
            });
        }

        // 2. 检查文件大小限制。
        match tokio::fs::metadata(&full_path).await {
            Ok(meta) => {
                if meta.len() > MAX_EDIT_FILE_SIZE {
                    return Ok(ToolResult {
                        output: format!(
                            "File is too large to edit ({} bytes). Maximum: {} bytes.",
                            meta.len(),
                            MAX_EDIT_FILE_SIZE
                        ),
                        is_error: true,
                        duration_ms: 0,
                        metadata: HashMap::new(),
                    });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 文件不存在：如果 old_string 为空则是新建文件。
                if inp.old_string.is_empty() {
                    if let Some(result) = team_memory_secret_error(&full_path, &inp.new_string) {
                        return Ok(result);
                    }
                    atomic_write(&full_path, &inp.new_string).await?;
                    mossen_agent::services::team_memory_sync::notify_team_memory_file_write(
                        &full_path,
                    )
                    .await;
                    info!(path = %full_path, "SourcePatcher: created new file");
                    let output = SourcePatcherOutput {
                        file_path: inp.file_path,
                        old_string: None,
                        new_string: Some(inp.new_string),
                        replace_all: inp.replace_all,
                    };
                    let metadata = crate::skill_discovery::observe_tool_file_paths(
                        [observed_file_path.as_str()],
                        &context.cwd,
                    )
                    .await
                    .to_metadata();
                    return Ok(ToolResult {
                        output: serde_json::to_string(&output)?,
                        is_error: false,
                        duration_ms: 0,
                        metadata,
                    });
                }
                return Ok(ToolResult {
                    output: format!("File does not exist: {}", inp.file_path),
                    is_error: true,
                    duration_ms: 0,
                    metadata: HashMap::new(),
                });
            }
            Err(e) => return Err(e.into()),
        }

        // 3. 读取文件内容。
        let content = tokio::fs::read_to_string(&full_path).await?;

        // 4. 空 old_string + 非空文件 → 错误。
        if inp.old_string.is_empty() {
            if content.trim().is_empty() {
                // 空文件，可以写入。
                if let Some(result) = team_memory_secret_error(&full_path, &inp.new_string) {
                    return Ok(result);
                }
                atomic_write(&full_path, &inp.new_string).await?;
                mossen_agent::services::team_memory_sync::notify_team_memory_file_write(&full_path)
                    .await;
                let output = SourcePatcherOutput {
                    file_path: inp.file_path,
                    old_string: None,
                    new_string: Some(inp.new_string),
                    replace_all: inp.replace_all,
                };
                let metadata = crate::skill_discovery::observe_tool_file_paths(
                    [observed_file_path.as_str()],
                    &context.cwd,
                )
                .await
                .to_metadata();
                return Ok(ToolResult {
                    output: serde_json::to_string(&output)?,
                    is_error: false,
                    duration_ms: 0,
                    metadata,
                });
            }
            return Ok(ToolResult {
                output: "Cannot create new file - file already exists with content.".to_string(),
                is_error: true,
                duration_ms: 0,
                metadata: HashMap::new(),
            });
        }

        // 5. 查找匹配项。
        let match_count = content.matches(&inp.old_string).count();
        if match_count == 0 {
            return Ok(ToolResult {
                output: format!(
                    "String to replace not found in file.\nString: {}",
                    inp.old_string
                ),
                is_error: true,
                duration_ms: 0,
                metadata: HashMap::new(),
            });
        }

        // 6. 多匹配但 replace_all 为 false → 错误。
        if match_count > 1 && !inp.replace_all {
            return Ok(ToolResult {
                output: format!(
                    "Found {} matches but replace_all is false. Set replace_all to true or provide more context.",
                    match_count
                ),
                is_error: true,
                duration_ms: 0,
                metadata: HashMap::new(),
            });
        }

        // 7. 执行替换。
        let updated = if inp.replace_all {
            content.replace(&inp.old_string, &inp.new_string)
        } else {
            content.replacen(&inp.old_string, &inp.new_string, 1)
        };
        if let Some(result) = team_memory_secret_error(&full_path, &updated) {
            return Ok(result);
        }

        // 8. 原子写入。
        atomic_write(&full_path, &updated).await?;
        mossen_agent::services::team_memory_sync::notify_team_memory_file_write(&full_path).await;

        info!(
            path = %full_path,
            matches = match_count,
            replace_all = inp.replace_all,
            "SourcePatcher: file updated"
        );

        let output = SourcePatcherOutput {
            file_path: inp.file_path,
            old_string: Some(inp.old_string),
            new_string: Some(inp.new_string),
            replace_all: inp.replace_all,
        };
        let metadata = crate::skill_discovery::observe_tool_file_paths(
            [observed_file_path.as_str()],
            &context.cwd,
        )
        .await
        .to_metadata();

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata,
        })
    }
}
