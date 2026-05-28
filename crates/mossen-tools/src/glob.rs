//! # glob — PathDiscoverer 工具
//!
//! 对应 TS `GlobTool`（199 行）。基于 glob 模式搜索文件。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tracing::info;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 路径发现器 — 基于 glob 模式搜索文件。
pub struct PathDiscoverer;

/// 默认结果上限。
const DEFAULT_RESULT_LIMIT: usize = 100;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct PathDiscovererInput {
    /// Glob 模式。
    pub pattern: String,
    /// 搜索目录（可选，默认 cwd）。
    #[serde(default)]
    pub path: Option<String>,
}

fn tool_error(message: impl Into<String>, duration_ms: u64) -> ToolResult {
    ToolResult {
        output: message.into(),
        is_error: true,
        duration_ms,
        metadata: HashMap::new(),
    }
}

fn parse_input(input: Value) -> Result<PathDiscovererInput, String> {
    match input {
        Value::Null => Err("Glob requires a JSON object with a `pattern` string; received null."
            .to_string()),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("Glob received invalid input: {error}. Expected object: {{\"pattern\":\"...\",\"path\":\"optional directory\"}}.")
        }),
        other => Err(format!(
            "Glob requires a JSON object with a `pattern` string; received {}.",
            other
        )),
    }
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "pattern".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The glob pattern to match files against"
        }),
    );
    properties.insert(
        "path".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The directory to search in. Defaults to current working directory."
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["pattern".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for PathDiscoverer {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Find files by name pattern using glob matching"
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
            Err(message) => return Ok(tool_error(message, start.elapsed().as_millis() as u64)),
        };

        let base = inp
            .path
            .map(|p| shellexpand::tilde(&p).to_string())
            .unwrap_or_else(|| context.cwd.clone());

        info!(pattern = %inp.pattern, base = %base, "PathDiscoverer: searching");

        let matcher = match globset::Glob::new(&inp.pattern) {
            Ok(glob) => glob.compile_matcher(),
            Err(error) => {
                return Ok(tool_error(
                    format!("Glob pattern is invalid: {error}"),
                    start.elapsed().as_millis() as u64,
                ))
            }
        };
        let mut files = Vec::new();
        let mut truncated = false;

        for entry in ignore::WalkBuilder::new(&base).hidden(false).build() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if matcher.is_match(entry.path()) || matcher.is_match(entry.file_name()) {
                let rel = entry
                    .path()
                    .strip_prefix(&base)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .to_string();
                files.push(rel);
                if files.len() >= DEFAULT_RESULT_LIMIT {
                    truncated = true;
                    break;
                }
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;
        let num_files = files.len();

        let content = if files.is_empty() {
            "No files found".to_string()
        } else {
            let mut parts = files.clone();
            if truncated {
                parts.push(
                    "(Results are truncated. Consider using a more specific path or pattern.)"
                        .to_string(),
                );
            }
            parts.join("\n")
        };

        Ok(ToolResult {
            output: content,
            is_error: false,
            duration_ms: elapsed,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PathDiscoverer;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use std::collections::HashMap;

    fn context(cwd: &std::path::Path) -> ToolUseContext {
        ToolUseContext {
            cwd: cwd.to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn glob_null_input_returns_structured_tool_error() {
        let temp = tempfile::tempdir().expect("tempdir");

        let result = PathDiscoverer
            .execute(serde_json::Value::Null, &context(temp.path()))
            .await
            .expect("glob result");

        assert!(result.is_error);
        assert!(result.output.contains("pattern"), "{}", result.output);
        assert!(result.output.contains("null"), "{}", result.output);
    }

    #[tokio::test]
    async fn glob_missing_pattern_returns_structured_tool_error() {
        let temp = tempfile::tempdir().expect("tempdir");

        let result = PathDiscoverer
            .execute(serde_json::json!({}), &context(temp.path()))
            .await
            .expect("glob result");

        assert!(result.is_error);
        assert!(result.output.contains("invalid input"), "{}", result.output);
        assert!(result.output.contains("pattern"), "{}", result.output);
    }
}
