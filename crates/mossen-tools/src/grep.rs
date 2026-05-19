//! # grep — ContentScanner 工具
//!
//! 对应 TS `GrepTool`（578 行）。通过 ripgrep 搜索文件内容。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;
use tracing::info;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 内容扫描器 — 通过 ripgrep 搜索文件内容。
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
    properties.insert("multiline".to_string(), serde_json::json!({"type": "boolean", "description": "Enable multiline mode (rg -U --multiline-dotall). Default false."}));

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["pattern".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for ContentScanner {
    fn name(&self) -> &str {
        "Grep"
    }
    fn description(&self) -> &str {
        "Search file contents with regex using ripgrep"
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
        let inp: ContentScannerInput = serde_json::from_value(input)?;
        let start = std::time::Instant::now();

        let search_path = inp
            .path
            .map(|p| shellexpand::tilde(&p).to_string())
            .unwrap_or_else(|| context.cwd.clone());
        let mode = inp.output_mode.as_deref().unwrap_or("files_with_matches");

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

        info!(pattern = %inp.pattern, path = %search_path, mode = %mode, "ContentScanner: searching");

        let output = Command::new("rg")
            .args(&args)
            .current_dir(&search_path)
            .output()
            .await?;

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
