//! # bash — ShellExecutor 工具
//!
//! 对应 TS `BashTool`（1144 行）。通过 `tokio::process::Command` 执行 shell 命令，
//! 支持超时、后台任务、信号取消。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;
use tracing::{info, warn};

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

/// Shell 执行器 — 执行 shell 命令。
pub struct ShellExecutor;

/// 默认超时（毫秒）。
const DEFAULT_TIMEOUT_MS: u64 = 120_000;
/// 最大超时（毫秒）。
const MAX_TIMEOUT_MS: u64 = 600_000;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct ShellExecutorInput {
    /// 要执行的 shell 命令。
    pub command: String,
    /// 命令描述。
    #[serde(default)]
    pub description: Option<String>,
    /// 超时（毫秒）。
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// 是否后台运行。
    #[serde(default)]
    pub run_in_background: bool,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_MS
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct ShellExecutorOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default)]
    pub interrupted: bool,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "command".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The shell command to execute."
        }),
    );
    properties.insert(
        "description".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "A brief description of what the command does."
        }),
    );
    properties.insert(
        "timeout".to_string(),
        serde_json::json!({
            "type": "number",
            "description": "Timeout in milliseconds (max 600000).",
            "default": 120000
        }),
    );
    properties.insert(
        "run_in_background".to_string(),
        serde_json::json!({
            "type": "boolean",
            "description": "Whether to run the command in the background.",
            "default": false
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["command".to_string()]),
        extra: HashMap::new(),
    }
}

/// 检测命令是否为只读操作。
fn is_read_only_command(command: &str) -> bool {
    let trimmed = command.trim();
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    matches!(
        first_word,
        "cat"
            | "head"
            | "tail"
            | "less"
            | "more"
            | "wc"
            | "stat"
            | "file"
            | "ls"
            | "tree"
            | "du"
            | "find"
            | "grep"
            | "rg"
            | "ag"
            | "ack"
            | "locate"
            | "which"
            | "whereis"
            | "echo"
            | "printf"
            | "pwd"
            | "env"
            | "whoami"
            | "date"
            | "uname"
            | "hostname"
    )
}

#[async_trait]
impl Tool for ShellExecutor {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command"
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
        let inp: ShellExecutorInput = serde_json::from_value(input)?;
        let start = std::time::Instant::now();

        let timeout_ms = inp.timeout.min(MAX_TIMEOUT_MS);
        let duration = std::time::Duration::from_millis(timeout_ms);

        info!(
            command = %inp.command,
            timeout_ms = timeout_ms,
            background = inp.run_in_background,
            "ShellExecutor: running command"
        );

        // 后台模式：生成后台任务并立即返回。
        if inp.run_in_background {
            let cmd = inp.command.clone();
            let cwd = context.cwd.clone();
            tokio::spawn(async move {
                let _ = Command::new("bash")
                    .arg("-c")
                    .arg(&cmd)
                    .current_dir(&cwd)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn();
            });

            let output = ShellExecutorOutput {
                stdout: Some("Command started in background.".to_string()),
                stderr: None,
                exit_code: None,
                timed_out: false,
                interrupted: false,
            };
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error: false,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }

        // 前台模式：执行命令并等待结果，带超时。
        let child = Command::new("bash")
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
                        let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                        ShellExecutorOutput {
                            stdout: if stdout.is_empty() { None } else { Some(stdout) },
                            stderr: if stderr.is_empty() { None } else { Some(stderr) },
                            exit_code: o.status.code(),
                            timed_out: false,
                            interrupted: false,
                        }
                    }
                    Err(e) => {
                        ShellExecutorOutput {
                            stdout: None,
                            stderr: Some(format!("Failed to execute command: {e}")),
                            exit_code: None,
                            timed_out: false,
                            interrupted: false,
                        }
                    }
                }
            }
            _ = tokio::time::sleep(duration) => {
                warn!(command = %inp.command, "ShellExecutor: command timed out");
                // child is dropped here (kill_on_drop=true), so process is killed.
                ShellExecutorOutput {
                    stdout: None,
                    stderr: Some("Command timed out".to_string()),
                    exit_code: None,
                    timed_out: true,
                    interrupted: false,
                }
            }
        };

        let elapsed = start.elapsed().as_millis() as u64;
        let is_error = result.exit_code.map_or(true, |c| c != 0) || result.timed_out;

        Ok(ToolResult {
            output: serde_json::to_string(&result)?,
            is_error,
            duration_ms: elapsed,
            metadata: HashMap::new(),
        })
    }
}
