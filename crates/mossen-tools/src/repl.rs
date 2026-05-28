//! # repl — SandboxedRunner 工具
//!
//! 对应 TS `REPLTool`。在沙箱环境中批量执行工具操作。
//! REPL 模式下，FileRead/FileEdit/Bash 等工具被隐藏，
//! 强制通过 REPL 进行批量操作。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// 沙箱运行器 — 在受控环境中执行批量操作。
pub struct SandboxedRunner;

/// REPL 模式下被隐藏的工具名称。
pub const REPL_ONLY_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Glob",
    "Grep",
    "Bash",
    "NotebookEdit",
    "Agent",
];

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct SandboxedRunnerInput {
    /// 要在 REPL 中执行的命令或脚本。
    pub command: String,
    /// 可选的执行语言/模式。
    #[serde(default)]
    pub language: Option<String>,
    /// 超时（毫秒）。
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    120_000
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct SandboxedRunnerOutput {
    pub stdout: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    pub exit_code: i32,
    pub timed_out: bool,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "command".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The command or script to execute in the REPL sandbox."
        }),
    );
    properties.insert(
        "language".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The language/mode for execution (e.g., 'bash', 'python')."
        }),
    );
    properties.insert(
        "timeout".to_string(),
        serde_json::json!({
            "type": "number",
            "description": "Timeout in milliseconds.",
            "default": 120000
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["command".to_string()]),
        extra: HashMap::new(),
    }
}

/// 检查 REPL 模式是否启用。
pub fn is_repl_mode_enabled() -> bool {
    // MOSSEN_CODE_REPL=0 → 禁用。
    if let Ok(val) = std::env::var("MOSSEN_CODE_REPL") {
        if val == "0" || val.to_lowercase() == "false" {
            return false;
        }
    }
    // MOSSEN_REPL_MODE=1 → 强制启用。
    if let Ok(val) = std::env::var("MOSSEN_REPL_MODE") {
        if val == "1" || val.to_lowercase() == "true" {
            return true;
        }
    }
    false
}

#[async_trait]
impl Tool for SandboxedRunner {
    fn name(&self) -> &str {
        "REPL"
    }

    fn description(&self) -> &str {
        "Execute commands in a sandboxed REPL environment"
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
        let inp: SandboxedRunnerInput = serde_json::from_value(input)?;
        let start = std::time::Instant::now();

        let timeout_ms = inp.timeout.min(600_000);
        let duration = std::time::Duration::from_millis(timeout_ms);

        info!(
            command_len = inp.command.len(),
            language = ?inp.language,
            timeout_ms = timeout_ms,
            "SandboxedRunner: executing command"
        );

        // REPL 内部通过 shell 执行命令。
        // 完整的沙箱隔离由上层编排器管理。
        let child = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&inp.command)
            .current_dir(&context.cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let result = tokio::select! {
            res = child.wait_with_output() => {
                match res {
                    Ok(o) => {
                        SandboxedRunnerOutput {
                            stdout: String::from_utf8_lossy(&o.stdout).to_string(),
                            stderr: {
                                let s = String::from_utf8_lossy(&o.stderr).to_string();
                                if s.is_empty() { None } else { Some(s) }
                            },
                            exit_code: o.status.code().unwrap_or(-1),
                            timed_out: false,
                        }
                    }
                    Err(e) => {
                        SandboxedRunnerOutput {
                            stdout: String::new(),
                            stderr: Some(format!("Execution failed: {e}")),
                            exit_code: -1,
                            timed_out: false,
                        }
                    }
                }
            }
            _ = tokio::time::sleep(duration) => {
                SandboxedRunnerOutput {
                    stdout: String::new(),
                    stderr: Some("Command timed out".to_string()),
                    exit_code: -1,
                    timed_out: true,
                }
            }
        };

        let elapsed = start.elapsed().as_millis() as u64;
        let is_error = result.exit_code != 0 || result.timed_out;

        Ok(ToolResult {
            output: serde_json::to_string(&result)?,
            is_error,
            duration_ms: elapsed,
            metadata: HashMap::new(),
        })
    }
}
