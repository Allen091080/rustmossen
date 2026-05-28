//! # power_shell — WinShellExecutor 工具
//!
//! 对应 TS `PowerShellTool`（1001 行）。在 Windows 上执行 PowerShell 命令。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// Windows Shell 执行器 — 执行 PowerShell 命令。
pub struct WinShellExecutor;

#[derive(Debug, Clone, Deserialize)]
pub struct WinShellExecutorInput {
    /// 要执行的 PowerShell 命令。
    pub command: String,
    /// 超时（毫秒）。
    #[serde(default)]
    pub timeout: Option<u64>,
    /// 命令描述。
    #[serde(default)]
    pub description: Option<String>,
    /// 是否后台运行。
    #[serde(default)]
    pub run_in_background: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WinShellExecutorOutput {
    pub stdout: String,
    pub stderr: String,
    pub interrupted: bool,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "returnCodeInterpretation"
    )]
    pub return_code_interpretation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "backgroundTaskId")]
    pub background_task_id: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "command".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The PowerShell command to execute"
        }),
    );
    properties.insert(
        "timeout".to_string(),
        serde_json::json!({
            "type": "number",
            "description": "Optional timeout in milliseconds"
        }),
    );
    properties.insert(
        "description".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Clear, concise description of what this command does in active voice."
        }),
    );
    properties.insert(
        "run_in_background".to_string(),
        serde_json::json!({
            "type": "boolean",
            "description": "Set to true to run this command in the background."
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["command".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for WinShellExecutor {
    fn name(&self) -> &str {
        "PowerShell"
    }
    fn description(&self) -> &str {
        "Execute a PowerShell command (Windows only)"
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
        let _inp: WinShellExecutorInput = serde_json::from_value(input)?;
        let output = WinShellExecutorOutput {
            stdout: String::new(),
            stderr: "PowerShell is only available on Windows.".to_string(),
            interrupted: false,
            return_code_interpretation: None,
            background_task_id: None,
        };
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: true,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}
