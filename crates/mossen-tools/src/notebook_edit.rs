//! # notebook_edit — NotebookPatcher 工具
//!
//! 对应 TS `NotebookEditTool`（491 行）。编辑 Jupyter Notebook 单元格。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// Notebook 补丁器 — 编辑 Jupyter notebook 中的单元格。
pub struct NotebookPatcher;

#[derive(Debug, Clone, Deserialize)]
pub struct NotebookPatcherInput {
    /// notebook 文件的绝对路径。
    pub notebook_path: String,
    /// 要编辑的单元格 ID。
    #[serde(default)]
    pub cell_id: Option<String>,
    /// 新的单元格源码。
    pub new_source: String,
    /// 单元格类型（code 或 markdown）。
    #[serde(default)]
    pub cell_type: Option<String>,
    /// 编辑模式（replace / insert / delete）。
    #[serde(default)]
    pub edit_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotebookPatcherOutput {
    pub new_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
    pub cell_type: String,
    pub language: String,
    pub edit_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub notebook_path: String,
    pub original_file: String,
    pub updated_file: String,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert("notebook_path".to_string(), serde_json::json!({
        "type": "string",
        "description": "The absolute path to the Jupyter notebook file to edit (must be absolute, not relative)"
    }));
    properties.insert("cell_id".to_string(), serde_json::json!({
        "type": "string",
        "description": "The ID of the cell to edit. When inserting, the new cell is placed after this one."
    }));
    properties.insert(
        "new_source".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The new source for the cell"
        }),
    );
    properties.insert(
        "cell_type".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": ["code", "markdown"],
            "description": "The type of the cell (code or markdown). Defaults to current cell type."
        }),
    );
    properties.insert(
        "edit_mode".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": ["replace", "insert", "delete"],
            "description": "The type of edit to make. Defaults to replace."
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["notebook_path".to_string(), "new_source".to_string()]),
        extra: HashMap::new(),
    }
}

fn resolve_notebook_path(path: &str, cwd: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(cwd).join(p)
    }
}

fn source_value(source: &str) -> Value {
    let mut lines: Vec<String> = source
        .split_inclusive('\n')
        .map(|line| line.to_string())
        .collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    Value::Array(lines.into_iter().map(Value::String).collect())
}

fn source_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(lines) => lines
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn cell_matches(cell: &Value, target: &str) -> bool {
    cell.get("id").and_then(Value::as_str) == Some(target)
}

fn next_cell_id(cells: &[Value]) -> String {
    let mut idx = cells.len() + 1;
    loop {
        let candidate = format!("mossen-cell-{idx}");
        if !cells.iter().any(|c| cell_matches(c, &candidate)) {
            return candidate;
        }
        idx += 1;
    }
}

fn parse_input(input: Value) -> Result<NotebookPatcherInput, String> {
    match input {
        Value::Null => Err(
            "NotebookEdit requires a JSON object with `notebook_path` and `new_source`; received null."
                .to_string(),
        ),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("NotebookEdit received invalid input: {error}. Expected object: {{\"notebook_path\":\"...\",\"new_source\":\"...\"}}.")
        }),
        other => Err(format!(
            "NotebookEdit requires a JSON object with `notebook_path` and `new_source`; received {}.",
            other
        )),
    }
}

fn error_output(
    notebook_path: impl Into<String>,
    original_file: impl Into<String>,
    message: impl Into<String>,
    start: Instant,
) -> anyhow::Result<ToolResult> {
    let output = NotebookPatcherOutput {
        new_source: String::new(),
        cell_id: None,
        cell_type: "code".to_string(),
        language: "python".to_string(),
        edit_mode: "replace".to_string(),
        error: Some(message.into()),
        notebook_path: notebook_path.into(),
        original_file: original_file.into(),
        updated_file: String::new(),
    };
    Ok(ToolResult {
        output: serde_json::to_string(&output)?,
        is_error: true,
        duration_ms: start.elapsed().as_millis() as u64,
        metadata: HashMap::new(),
    })
}

#[async_trait]
impl Tool for NotebookPatcher {
    fn name(&self) -> &str {
        "NotebookEdit"
    }
    fn description(&self) -> &str {
        "Edit Jupyter notebook cells (replace, insert, or delete)"
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
        let start = Instant::now();
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => return error_output("", "", message, start),
        };
        let mode = inp.edit_mode.unwrap_or_else(|| "replace".to_string());
        let cell_type = inp.cell_type.unwrap_or_else(|| "code".to_string());
        if inp.notebook_path.trim().is_empty() {
            return error_output(
                "",
                "",
                "NotebookEdit requires a non-empty `notebook_path`.",
                start,
            );
        }
        if !matches!(mode.as_str(), "replace" | "insert" | "delete") {
            return error_output(
                &inp.notebook_path,
                "",
                format!("unsupported edit_mode: {mode}"),
                start,
            );
        }
        if !matches!(cell_type.as_str(), "code" | "markdown") {
            return error_output(
                &inp.notebook_path,
                "",
                format!("unsupported cell_type: {cell_type}"),
                start,
            );
        }
        let path = resolve_notebook_path(&inp.notebook_path, &context.cwd);
        let path_display = path.to_string_lossy().to_string();
        let original_file = match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(error) => {
                return error_output(
                    path_display,
                    "",
                    format!("failed to read notebook: {error}"),
                    start,
                );
            }
        };
        let mut notebook: Value = match serde_json::from_str(&original_file) {
            Ok(notebook) => notebook,
            Err(error) => {
                return error_output(
                    path_display,
                    original_file,
                    format!("failed to parse notebook JSON: {error}"),
                    start,
                );
            }
        };
        let cells = notebook
            .get_mut("cells")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow::anyhow!("notebook missing cells array"));
        let cells = match cells {
            Ok(cells) => cells,
            Err(error) => {
                return error_output(path_display, original_file, error.to_string(), start);
            }
        };

        let mut resolved_cell_id = inp.cell_id.clone();
        let error = None;
        match mode.as_str() {
            "replace" => {
                let cell_id = match inp.cell_id.as_deref() {
                    Some(cell_id) if !cell_id.trim().is_empty() => cell_id,
                    _ => {
                        return error_output(
                            path_display,
                            original_file,
                            "cell_id is required for replace",
                            start,
                        );
                    }
                };
                let cell = match cells.iter_mut().find(|cell| cell_matches(cell, cell_id)) {
                    Some(cell) => cell,
                    None => {
                        return error_output(
                            path_display,
                            original_file,
                            format!("cell_id not found: {cell_id}"),
                            start,
                        );
                    }
                };
                cell["source"] = source_value(&inp.new_source);
                cell["cell_type"] = Value::String(cell_type.clone());
            }
            "insert" => {
                let new_id = next_cell_id(cells);
                let new_cell = match cell_type.as_str() {
                    "markdown" => json!({
                        "cell_type": "markdown",
                        "id": new_id,
                        "metadata": {},
                        "source": source_value(&inp.new_source)
                    }),
                    "code" => json!({
                        "cell_type": "code",
                        "execution_count": null,
                        "id": new_id,
                        "metadata": {},
                        "outputs": [],
                        "source": source_value(&inp.new_source)
                    }),
                    _ => unreachable!("cell_type was validated before edit"),
                };
                let insert_at = inp
                    .cell_id
                    .as_deref()
                    .and_then(|id| cells.iter().position(|cell| cell_matches(cell, id)))
                    .map(|idx| idx + 1)
                    .unwrap_or(cells.len());
                cells.insert(insert_at, new_cell);
                resolved_cell_id = cells
                    .get(insert_at)
                    .and_then(|cell| cell.get("id"))
                    .and_then(Value::as_str)
                    .map(|s| s.to_string());
            }
            "delete" => {
                let cell_id = match inp.cell_id.as_deref() {
                    Some(cell_id) if !cell_id.trim().is_empty() => cell_id,
                    _ => {
                        return error_output(
                            path_display,
                            original_file,
                            "cell_id is required for delete",
                            start,
                        );
                    }
                };
                let idx = match cells.iter().position(|cell| cell_matches(cell, cell_id)) {
                    Some(idx) => idx,
                    None => {
                        return error_output(
                            path_display,
                            original_file,
                            format!("cell_id not found: {cell_id}"),
                            start,
                        );
                    }
                };
                cells.remove(idx);
            }
            _ => unreachable!("edit_mode was validated before edit"),
        }

        let updated_file = serde_json::to_string_pretty(&notebook)?;
        if let Err(error) = tokio::fs::write(&path, format!("{updated_file}\n")).await {
            return error_output(
                path_display,
                original_file,
                format!("failed to write notebook: {error}"),
                start,
            );
        }

        let effective_source = if mode == "delete" {
            String::new()
        } else if let Some(cell_id) = resolved_cell_id.as_deref() {
            notebook
                .get("cells")
                .and_then(Value::as_array)
                .and_then(|cells| cells.iter().find(|cell| cell_matches(cell, cell_id)))
                .and_then(|cell| cell.get("source"))
                .map(source_to_string)
                .unwrap_or_else(|| inp.new_source.clone())
        } else {
            inp.new_source.clone()
        };
        let output = NotebookPatcherOutput {
            new_source: effective_source,
            cell_id: resolved_cell_id,
            cell_type,
            language: notebook
                .get("metadata")
                .and_then(|m| m.get("language_info"))
                .and_then(|l| l.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("python")
                .to_string(),
            edit_mode: mode,
            error,
            notebook_path: path_display,
            original_file,
            updated_file,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: output.error.is_some(),
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::NotebookPatcher;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use serde_json::Value;
    use std::collections::HashMap;

    fn context(cwd: &str) -> ToolUseContext {
        ToolUseContext {
            cwd: cwd.to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn notebook_edit_replaces_cell_source_on_disk() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("demo.ipynb");
        tokio::fs::write(
            &path,
            r#"{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "id": "cell-a",
      "metadata": {},
      "outputs": [],
      "source": ["print('old')\n"]
    }
  ],
  "metadata": {"language_info": {"name": "python"}},
  "nbformat": 4,
  "nbformat_minor": 5
}"#,
        )
        .await
        .expect("write notebook");

        let result = NotebookPatcher
            .execute(
                serde_json::json!({
                    "notebook_path": path,
                    "cell_id": "cell-a",
                    "new_source": "print('new')\n",
                    "edit_mode": "replace"
                }),
                &context(temp.path().to_string_lossy().as_ref()),
            )
            .await
            .expect("notebook edit");
        assert!(!result.is_error);
        let output: Value = serde_json::from_str(&result.output).expect("json");
        assert_eq!(output["cell_id"], "cell-a");
        let updated = tokio::fs::read_to_string(&path).await.expect("updated");
        assert!(updated.contains("print('new')"));
    }

    #[tokio::test]
    async fn notebook_edit_null_input_returns_structured_tool_error() {
        let temp = tempfile::tempdir().expect("tempdir");

        let result = NotebookPatcher
            .execute(
                serde_json::Value::Null,
                &context(temp.path().to_string_lossy().as_ref()),
            )
            .await
            .expect("notebook edit");
        let output: Value = serde_json::from_str(&result.output).expect("json");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("notebook_path"));
    }

    #[tokio::test]
    async fn notebook_edit_missing_cell_returns_structured_tool_error_without_writing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("demo.ipynb");
        let original = r#"{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "id": "cell-a",
      "metadata": {},
      "outputs": [],
      "source": ["print('old')\n"]
    }
  ],
  "metadata": {"language_info": {"name": "python"}},
  "nbformat": 4,
  "nbformat_minor": 5
}"#;
        tokio::fs::write(&path, original)
            .await
            .expect("write notebook");

        let result = NotebookPatcher
            .execute(
                serde_json::json!({
                    "notebook_path": path,
                    "cell_id": "missing-cell",
                    "new_source": "print('new')\n",
                    "edit_mode": "replace"
                }),
                &context(temp.path().to_string_lossy().as_ref()),
            )
            .await
            .expect("notebook edit");
        let output: Value = serde_json::from_str(&result.output).expect("json");

        assert!(result.is_error);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("cell_id not found"));
        let after = tokio::fs::read_to_string(&path)
            .await
            .expect("read notebook");
        assert_eq!(after, original);
    }
}
