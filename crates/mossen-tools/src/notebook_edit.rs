//! # notebook_edit — NotebookPatcher 工具
//!
//! 对应 TS `NotebookEditTool`（491 行）。编辑 Jupyter Notebook 单元格。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: NotebookPatcherInput = serde_json::from_value(input)?;
        let mode = inp.edit_mode.unwrap_or_else(|| "replace".to_string());
        let cell_type = inp.cell_type.unwrap_or_else(|| "code".to_string());
        let output = NotebookPatcherOutput {
            new_source: inp.new_source,
            cell_id: inp.cell_id,
            cell_type,
            language: "python".to_string(),
            edit_mode: mode,
            error: Some("NotebookEdit is a stub in this build.".to_string()),
            notebook_path: inp.notebook_path,
            original_file: String::new(),
            updated_file: String::new(),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
