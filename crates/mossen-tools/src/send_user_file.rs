//! # send_user_file — FileDelivery 工具
//!
//! 对应 TS `SendUserFileTool`（109 行）。向用户投递文件附件。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 文件投递器 — 向用户发送文件。
pub struct FileDelivery;

#[derive(Debug, Clone, Deserialize)]
pub struct FileDeliveryInput {
    /// 文件路径列表（绝对或相对于 cwd）。
    pub attachments: Vec<String>,
    /// 附带的简短说明。
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDeliveryAttachment {
    pub path: String,
    pub size: u64,
    #[serde(rename = "isImage")]
    pub is_image: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDeliveryOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub attachments: Vec<FileDeliveryAttachment>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sentAt")]
    pub sent_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn parse_input(input: Value) -> Result<FileDeliveryInput, String> {
    match input {
        Value::Null => Err(
            "SendUserFile requires a JSON object with an `attachments` array; received null."
                .to_string(),
        ),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("SendUserFile received invalid input: {error}. Expected object: {{\"attachments\":[\"...\"]}}.")
        }),
        other => Err(format!(
            "SendUserFile requires a JSON object with an `attachments` array; received {}.",
            other
        )),
    }
}

fn error_output(message: impl Into<String>) -> FileDeliveryOutput {
    FileDeliveryOutput {
        message: None,
        attachments: Vec::new(),
        sent_at: None,
        error: Some(message.into()),
    }
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "attachments".to_string(),
        serde_json::json!({
            "type": "array",
            "items": { "type": "string" },
            "minItems": 1,
            "description": "File paths (absolute or relative to cwd) to deliver to the user."
        }),
    );
    properties.insert(
        "message".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Optional short note to show with the delivered files."
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["attachments".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for FileDelivery {
    fn name(&self) -> &str {
        "SendUserFile"
    }
    fn description(&self) -> &str {
        "Deliver files or screenshots directly to the user"
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
        true
    }

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let start = std::time::Instant::now();
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => {
                return Ok(ToolResult {
                    output: serde_json::to_string(&error_output(message))?,
                    is_error: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                    metadata: HashMap::new(),
                })
            }
        };
        if inp.attachments.is_empty() {
            return Ok(ToolResult {
                output: serde_json::to_string(&error_output(
                    "SendUserFile requires at least one attachment.",
                ))?,
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }

        let mut attachments = Vec::with_capacity(inp.attachments.len());
        for requested in inp.attachments {
            if requested.trim().is_empty() {
                return Ok(ToolResult {
                    output: serde_json::to_string(&error_output(
                        "SendUserFile attachment paths must be non-empty strings.",
                    ))?,
                    is_error: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                    metadata: HashMap::new(),
                });
            }
            let path = resolve_attachment_path(&context.cwd, &requested);
            let metadata = match std::fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    return Ok(ToolResult {
                        output: serde_json::to_string(&error_output(format!(
                            "Attachment not found or unreadable: {} ({})",
                            requested, error
                        )))?,
                        is_error: true,
                        duration_ms: start.elapsed().as_millis() as u64,
                        metadata: HashMap::new(),
                    });
                }
            };
            if !metadata.is_file() {
                return Ok(ToolResult {
                    output: serde_json::to_string(&error_output(format!(
                        "Attachment is not a file: {}",
                        requested
                    )))?,
                    is_error: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                    metadata: HashMap::new(),
                });
            }

            let display_path = path
                .canonicalize()
                .unwrap_or_else(|_| path.clone())
                .to_string_lossy()
                .to_string();
            attachments.push(FileDeliveryAttachment {
                path: display_path,
                size: metadata.len(),
                is_image: is_image_path(&path),
                file_uuid: Some(uuid::Uuid::new_v4().to_string()),
            });
        }

        let output = FileDeliveryOutput {
            message: inp.message,
            attachments,
            sent_at: Some(chrono::Utc::now().to_rfc3339()),
            error: None,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}

fn resolve_attachment_path(cwd: &str, requested: &str) -> PathBuf {
    let expanded = shellexpand::tilde(requested).to_string();
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        Path::new(cwd).join(path)
    }
}

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "svg"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_user_file_returns_attachment_metadata_for_existing_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("note.txt");
        std::fs::write(&file, "hello").expect("write file");
        let context = ToolUseContext {
            cwd: temp.path().to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = FileDelivery
            .execute(
                serde_json::json!({
                    "attachments": ["note.txt"],
                    "message": "see attached"
                }),
                &context,
            )
            .await
            .expect("send user file");
        assert!(!result.is_error);
        let output: serde_json::Value = serde_json::from_str(&result.output).expect("output json");
        assert_eq!(output["message"], "see attached");
        assert_eq!(output["attachments"][0]["size"], 5);
        assert_eq!(output["attachments"][0]["isImage"], false);
        assert!(output["attachments"][0]["file_uuid"].is_string());
    }

    #[tokio::test]
    async fn send_user_file_errors_for_missing_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let context = ToolUseContext {
            cwd: temp.path().to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = FileDelivery
            .execute(
                serde_json::json!({
                    "attachments": ["missing.txt"]
                }),
                &context,
            )
            .await
            .expect("send user file");
        assert!(result.is_error);
        assert!(result.output.contains("Attachment not found"));
    }

    #[tokio::test]
    async fn send_user_file_null_input_returns_structured_tool_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let context = ToolUseContext {
            cwd: temp.path().to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = FileDelivery
            .execute(Value::Null, &context)
            .await
            .expect("send user file");
        let output: serde_json::Value = serde_json::from_str(&result.output).expect("output json");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("attachments"));
    }

    #[tokio::test]
    async fn send_user_file_empty_attachment_returns_structured_tool_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let context = ToolUseContext {
            cwd: temp.path().to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = FileDelivery
            .execute(serde_json::json!({"attachments": [""]}), &context)
            .await
            .expect("send user file");
        let output: serde_json::Value = serde_json::from_str(&result.output).expect("output json");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("non-empty"));
    }
}
