//! # grep — ContentScanner 工具
//!
//! 对应 TS `GrepTool`（578 行）。通过快速搜索后端搜索文件内容。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;
use tracing::info;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 内容扫描器 — 通过快速搜索后端搜索文件内容。
pub struct ContentScanner;

/// 默认结果行数限制。
const DEFAULT_HEAD_LIMIT: usize = 250;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct ContentScannerInput {
    /// 正则搜索模式。
    pub pattern: String,
    /// 搜索路径（可选）。
    #[serde(default)]
    pub path: Option<String>,
    /// glob 过滤。
    #[serde(default)]
    pub glob: Option<String>,
    /// 输出模式。
    #[serde(default)]
    pub output_mode: Option<String>,
    /// 上下文前行数。
    #[serde(rename = "-B", default)]
    pub context_before: Option<usize>,
    /// 上下文后行数。
    #[serde(rename = "-A", default)]
    pub context_after: Option<usize>,
    /// 上下文行数。
    #[serde(rename = "-C", default)]
    pub context_c: Option<usize>,
    /// 显示行号。
    #[serde(rename = "-n", default)]
    pub show_line_numbers: Option<bool>,
    /// 大小写不敏感。
    #[serde(rename = "-i", default)]
    pub case_insensitive: Option<bool>,
    /// 文件类型过滤。
    #[serde(rename = "type", default)]
    pub file_type: Option<String>,
    /// 结果行数限制。
    #[serde(default)]
    pub head_limit: Option<usize>,
    /// 偏移量。
    #[serde(default)]
    pub offset: Option<usize>,
    /// 多行模式。
    #[serde(default)]
    pub multiline: Option<bool>,
}

fn tool_error(message: impl Into<String>, duration_ms: u64) -> ToolResult {
    ToolResult {
        output: message.into(),
        is_error: true,
        duration_ms,
        metadata: HashMap::new(),
    }
}

fn parse_input(input: Value) -> Result<ContentScannerInput, String> {
    match input {
        Value::Null => Err("Grep requires a JSON object with a `pattern` string; received null."
            .to_string()),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("Grep received invalid input: {error}. Expected object: {{\"pattern\":\"...\",\"path\":\"optional file or directory\"}}.")
        }),
        other => Err(format!(
            "Grep requires a JSON object with a `pattern` string; received {}.",
            other
        )),
    }
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert("pattern".to_string(), serde_json::json!({"type": "string", "description": "The regular expression pattern to search for"}));
    properties.insert("path".to_string(), serde_json::json!({"type": "string", "description": "File or directory to search in. Defaults to cwd."}));
    properties.insert("glob".to_string(), serde_json::json!({"type": "string", "description": "Glob pattern to filter files (e.g. \"*.js\")"}));
    properties.insert("output_mode".to_string(), serde_json::json!({"type": "string", "enum": ["content", "files_with_matches", "count"], "description": "Output mode. Defaults to files_with_matches."}));
    properties.insert(
        "-B".to_string(),
        serde_json::json!({"type": "number", "description": "Lines before match (content mode)"}),
    );
    properties.insert(
        "-A".to_string(),
        serde_json::json!({"type": "number", "description": "Lines after match (content mode)"}),
    );
    properties.insert(
        "-C".to_string(),
        serde_json::json!({"type": "number", "description": "Context lines around match"}),
    );
    properties.insert("-n".to_string(), serde_json::json!({"type": "boolean", "description": "Show line numbers (content mode). Default true."}));
    properties.insert(
        "-i".to_string(),
        serde_json::json!({"type": "boolean", "description": "Case insensitive search"}),
    );
    properties.insert("type".to_string(), serde_json::json!({"type": "string", "description": "File type to search (e.g. js, py, rust)"}));
    properties.insert("head_limit".to_string(), serde_json::json!({"type": "number", "description": "Limit output to first N entries. Default 250. Pass 0 for unlimited."}));
    properties.insert(
        "offset".to_string(),
        serde_json::json!({"type": "number", "description": "Skip first N entries. Default 0."}),
    );
    properties.insert("multiline".to_string(), serde_json::json!({"type": "boolean", "description": "Enable multiline matching. Default false."}));

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["pattern".to_string()]),
        extra: HashMap::new(),
    }
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.is_file()
        && std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn resolve_rg_executable() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MOSSEN_RG").map(PathBuf::from) {
        if is_executable_file(&path) {
            return Some(path);
        }
    }

    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("rg");
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }

    for candidate in ["/opt/homebrew/bin/rg", "/usr/local/bin/rg", "/usr/bin/rg"] {
        let candidate = PathBuf::from(candidate);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn resolve_search_target(raw_path: &str, cwd: &str) -> PathBuf {
    let expanded = shellexpand::tilde(raw_path).to_string();
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        PathBuf::from(cwd).join(path)
    }
}

fn grep_working_dir_and_target(path: &Path) -> anyhow::Result<(PathBuf, String)> {
    if path.is_dir() {
        return Ok((path.to_path_buf(), ".".to_string()));
    }
    if path.is_file() {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8: {}", path.display()))?;
        return Ok((parent.to_path_buf(), filename.to_string()));
    }
    anyhow::bail!("Path not found: {}", path.display())
}

#[async_trait]
impl Tool for ContentScanner {
    fn name(&self) -> &str {
        "Grep"
    }
    fn description(&self) -> &str {
        "Search file contents with regular expressions"
    }
    fn tool_type(&self) -> ToolType {
        ToolType::Builtin
    }
    fn is_read_only(&self) -> bool {
        true
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: build_input_schema(),
            cache_control: None,
        }
    }

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let start = std::time::Instant::now();
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => return Ok(tool_error(message, start.elapsed().as_millis() as u64)),
        };
        if inp.pattern.trim().is_empty() {
            return Ok(tool_error(
                "Grep requires a non-empty `pattern` string.",
                start.elapsed().as_millis() as u64,
            ));
        }

        let raw_search_path = inp.path.as_deref().unwrap_or(context.cwd.as_str());
        let search_path = resolve_search_target(raw_search_path, &context.cwd);
        let (working_dir, target_arg) = match grep_working_dir_and_target(&search_path) {
            Ok(target) => target,
            Err(error) => {
                return Ok(tool_error(
                    format!("Grep path is not searchable: {error}"),
                    start.elapsed().as_millis() as u64,
                ))
            }
        };
        let mode = inp.output_mode.as_deref().unwrap_or("files_with_matches");
        let Some(rg) = resolve_rg_executable() else {
            return Ok(ToolResult {
                output: "Grep search backend is unavailable. Ensure the search executable is installed or available on PATH.".to_string(),
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        };

        let mut args: Vec<String> = vec!["--hidden".into(), "--max-columns".into(), "500".into()];
        for dir in &[".git", ".svn", ".hg", ".bzr", ".jj", ".sl"] {
            args.push("--glob".into());
            args.push(format!("!{dir}"));
        }
        if inp.multiline.unwrap_or(false) {
            args.push("-U".into());
            args.push("--multiline-dotall".into());
        }
        if inp.case_insensitive.unwrap_or(false) {
            args.push("-i".into());
        }
        match mode {
            "files_with_matches" => args.push("-l".into()),
            "count" => args.push("-c".into()),
            _ => {}
        }
        if inp.show_line_numbers.unwrap_or(true) && mode == "content" {
            args.push("-n".into());
        }
        if mode == "content" {
            args.push("--with-filename".into());
        }
        if mode == "content" {
            if let Some(c) = inp.context_c {
                args.push("-C".into());
                args.push(c.to_string());
            } else {
                if let Some(b) = inp.context_before {
                    args.push("-B".into());
                    args.push(b.to_string());
                }
                if let Some(a) = inp.context_after {
                    args.push("-A".into());
                    args.push(a.to_string());
                }
            }
        }
        if inp.pattern.starts_with('-') {
            args.push("-e".into());
        }
        args.push(inp.pattern.clone());
        if let Some(t) = &inp.file_type {
            args.push("--type".into());
            args.push(t.clone());
        }
        if let Some(g) = &inp.glob {
            args.push("--glob".into());
            args.push(g.clone());
        }
        args.push(target_arg);

        info!(pattern = %inp.pattern, path = %search_path.display(), mode = %mode, "ContentScanner: searching");

        let output = match Command::new(rg)
            .args(&args)
            .current_dir(&working_dir)
            .output()
            .await
        {
            Ok(output) => output,
            Err(error) => {
                return Ok(tool_error(
                    format!("Failed to start Grep search backend: {error}"),
                    start.elapsed().as_millis() as u64,
                ))
            }
        };
        let is_no_matches = output.status.code() == Some(1);
        if !output.status.success() && !is_no_matches {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Ok(ToolResult {
                output: if stderr.is_empty() {
                    format!("Grep search backend failed with status {}", output.status)
                } else {
                    stderr
                },
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }

        let lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(String::from)
            .collect();

        let offset = inp.offset.unwrap_or(0);
        let limit = inp.head_limit.unwrap_or(DEFAULT_HEAD_LIMIT);
        let limited: Vec<&String> = if limit == 0 {
            lines.iter().skip(offset).collect()
        } else {
            lines.iter().skip(offset).take(limit).collect()
        };

        let content = if limited.is_empty() {
            "No matches found".to_string()
        } else {
            limited
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };

        Ok(ToolResult {
            output: content,
            is_error: false,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ContentScanner;
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
    async fn grep_searches_absolute_directory_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("marker.txt"), "alpha\nneedle-one\n")
            .expect("write fixture");

        let result = ContentScanner
            .execute(
                serde_json::json!({
                    "pattern": "needle-one",
                    "path": temp.path().to_string_lossy(),
                    "output_mode": "content"
                }),
                &context(temp.path()),
            )
            .await
            .expect("grep result");

        assert!(!result.is_error, "{}", result.output);
        assert!(result.output.contains("marker.txt"), "{}", result.output);
        assert!(result.output.contains("needle-one"), "{}", result.output);
    }

    #[tokio::test]
    async fn grep_searches_file_path_without_current_dir_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("single.rs");
        std::fs::write(&file, "fn main() { println!(\"needle-two\"); }\n").expect("write fixture");

        let result = ContentScanner
            .execute(
                serde_json::json!({
                    "pattern": "needle-two",
                    "path": file.to_string_lossy(),
                    "output_mode": "content"
                }),
                &context(temp.path()),
            )
            .await
            .expect("grep result");

        assert!(!result.is_error, "{}", result.output);
        assert!(result.output.contains("single.rs"), "{}", result.output);
        assert!(result.output.contains("needle-two"), "{}", result.output);
    }

    #[tokio::test]
    async fn grep_null_input_returns_structured_tool_error() {
        let temp = tempfile::tempdir().expect("tempdir");

        let result = ContentScanner
            .execute(serde_json::Value::Null, &context(temp.path()))
            .await
            .expect("grep result");

        assert!(result.is_error);
        assert!(result.output.contains("pattern"), "{}", result.output);
        assert!(result.output.contains("null"), "{}", result.output);
    }

    #[tokio::test]
    async fn grep_missing_path_returns_structured_tool_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let missing = temp.path().join("missing");

        let result = ContentScanner
            .execute(
                serde_json::json!({
                    "pattern": "needle",
                    "path": missing.to_string_lossy()
                }),
                &context(temp.path()),
            )
            .await
            .expect("grep result");

        assert!(result.is_error);
        assert!(
            result.output.contains("Path not found"),
            "{}",
            result.output
        );
    }
}
