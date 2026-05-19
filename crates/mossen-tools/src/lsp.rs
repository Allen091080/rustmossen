//! # lsp — LanguageOracle 工具
//!
//! 对应 TS `LSPTool`（861 行）。通过 LSP 协议查询代码智能信息。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 语言预言机 — 通过 LSP 执行代码导航操作。
pub struct LanguageOracle;

#[derive(Debug, Clone, Deserialize)]
pub struct LanguageOracleInput {
    /// LSP 操作类型。
    pub operation: String,
    /// 目标文件路径（绝对或相对）。
    #[serde(rename = "filePath")]
    pub file_path: String,
    /// 行号（1-based）。
    pub line: u32,
    /// 列偏移（1-based）。
    pub character: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageOracleOutput {
    pub operation: String,
    pub result: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "resultCount")]
    pub result_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "fileCount")]
    pub file_count: Option<u64>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "operation".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": [
                "goToDefinition", "findReferences", "hover",
                "documentSymbol", "workspaceSymbol", "goToImplementation",
                "prepareCallHierarchy", "incomingCalls", "outgoingCalls"
            ],
            "description": "The LSP operation to perform"
        }),
    );
    properties.insert(
        "filePath".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The absolute or relative path to the file"
        }),
    );
    properties.insert(
        "line".to_string(),
        serde_json::json!({
            "type": "integer",
            "description": "The line number (1-based, as shown in editors)"
        }),
    );
    properties.insert(
        "character".to_string(),
        serde_json::json!({
            "type": "integer",
            "description": "The character offset (1-based, as shown in editors)"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec![
            "operation".to_string(),
            "filePath".to_string(),
            "line".to_string(),
            "character".to_string(),
        ]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for LanguageOracle {
    fn name(&self) -> &str {
        "LSP"
    }
    fn description(&self) -> &str {
        "Perform LSP operations such as go-to-definition, find-references, hover, etc."
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

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: LanguageOracleInput = serde_json::from_value(input)?;
        let output = LanguageOracleOutput {
            operation: inp.operation,
            result: "LSP is a stub in this build.".to_string(),
            file_path: inp.file_path,
            result_count: Some(0),
            file_count: Some(0),
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: false,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
