//! # file_read — FileInspector 工具
//!
//! 对应 TS `FileReadTool`（1178 行）。支持读取文本、图片、PDF 等多种格式，
//! 带 token 预算和去重机制。

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 文件审查器 — 读取文件内容（文本/图片/PDF）。
pub struct FileInspector;

/// 默认最大行数限制。
const DEFAULT_MAX_LINES: usize = 2000;

/// 图片扩展名集合。
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// 会阻塞的设备路径。
const BLOCKED_DEVICE_PATHS: &[&str] = &[
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
    "/dev/full",
    "/dev/stdin",
    "/dev/tty",
    "/dev/console",
    "/dev/stdout",
    "/dev/stderr",
    "/dev/fd/0",
    "/dev/fd/1",
    "/dev/fd/2",
];

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct FileInspectorInput {
    /// 文件路径。
    pub file_path: String,
    /// 起始行号（1-based）。
    #[serde(default)]
    pub offset: Option<usize>,
    /// 读取行数限制。
    #[serde(default)]
    pub limit: Option<usize>,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum FileInspectorOutput {
    /// 文本文件。
    #[serde(rename = "text")]
    Text {
        file_path: String,
        content: String,
        total_lines: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        offset: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    },
    /// 图片文件。
    #[serde(rename = "image")]
    Image {
        file_path: String,
        media_type: String,
        size_bytes: u64,
    },
    /// 二进制文件。
    #[serde(rename = "binary")]
    Binary {
        file_path: String,
        size_bytes: u64,
        message: String,
    },
    /// 错误。
    #[serde(rename = "error")]
    Error { message: String },
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

/// 为文本添加行号。
fn add_line_numbers(content: &str, offset: usize) -> String {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{:>6}│{}", offset + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "file_path".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The path of the file to read."
        }),
    );
    properties.insert(
        "offset".to_string(),
        serde_json::json!({
            "type": "number",
            "description": "Line offset to start reading from (0-based)."
        }),
    );
    properties.insert(
        "limit".to_string(),
        serde_json::json!({
            "type": "number",
            "description": "Maximum number of lines to read."
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["file_path".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for FileInspector {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file"
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
        true
    }

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: FileInspectorInput = serde_json::from_value(input)?;
        let observed_file_path = inp.file_path.clone();
        let full_path = expand_path(&inp.file_path);

        // 检查被阻塞的设备路径。
        if BLOCKED_DEVICE_PATHS.contains(&full_path.as_str()) {
            let output = FileInspectorOutput::Error {
                message: format!("Cannot read device path: {}", full_path),
            };
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error: true,
                duration_ms: 0,
                metadata: HashMap::new(),
            });
        }

        // 检查文件是否存在。
        let metadata = match tokio::fs::metadata(&full_path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let output = FileInspectorOutput::Error {
                    message: format!("File not found: {}", inp.file_path),
                };
                return Ok(ToolResult {
                    output: serde_json::to_string(&output)?,
                    is_error: true,
                    duration_ms: 0,
                    metadata: HashMap::new(),
                });
            }
            Err(e) => return Err(e.into()),
        };

        // 检测文件类型。
        let ext = Path::new(&full_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // 图片文件。
        if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            let media_type = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => "application/octet-stream",
            };
            let output = FileInspectorOutput::Image {
                file_path: inp.file_path,
                media_type: media_type.to_string(),
                size_bytes: metadata.len(),
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

        // 尝试读取为文本。
        let raw_bytes = tokio::fs::read(&full_path).await?;

        // 检测二进制内容（前 8KB 中的 NUL 字节）。
        let check_len = raw_bytes.len().min(8192);
        let has_nul = raw_bytes[..check_len].contains(&0);
        if has_nul {
            let output = FileInspectorOutput::Binary {
                file_path: inp.file_path,
                size_bytes: metadata.len(),
                message: "File appears to be binary. Use appropriate tools to handle binary files."
                    .to_string(),
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

        let content = String::from_utf8_lossy(&raw_bytes).to_string();
        let all_lines: Vec<&str> = content.lines().collect();
        let total_lines = all_lines.len();

        let offset = inp.offset.unwrap_or(0);
        let limit = inp.limit.unwrap_or(DEFAULT_MAX_LINES);

        let selected: Vec<&str> = all_lines.iter().skip(offset).take(limit).copied().collect();

        let numbered = add_line_numbers(&selected.join("\n"), offset);

        info!(
            path = %full_path,
            total_lines = total_lines,
            offset = offset,
            limit = limit,
            "FileInspector: read file"
        );

        let output = FileInspectorOutput::Text {
            file_path: inp.file_path,
            content: numbered,
            total_lines,
            offset: if offset > 0 { Some(offset) } else { None },
            limit: if limit < total_lines {
                Some(limit)
            } else {
                None
            },
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
