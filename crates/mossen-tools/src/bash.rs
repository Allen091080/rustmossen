//! # bash — ShellExecutor 工具
//!
//! 对应 TS `BashTool`（1144 行）。通过 `tokio::process::Command` 执行 shell 命令，
//! 支持超时、后台任务、信号取消。

use std::collections::HashMap;

use async_trait::async_trait;
#[cfg(unix)]
use nix::sys::signal::{killpg, Signal};
#[cfg(unix)]
use nix::unistd::Pid;
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

#[cfg(unix)]
struct ForegroundProcessGroupGuard {
    pgid: Pid,
    active: bool,
}

#[cfg(unix)]
impl ForegroundProcessGroupGuard {
    fn from_child(child: &tokio::process::Child) -> Option<Self> {
        child.id().map(|pid| Self {
            pgid: Pid::from_raw(pid as i32),
            active: true,
        })
    }

    fn disarm(&mut self) {
        self.active = false;
    }

    fn terminate_now(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        terminate_process_group(self.pgid);
    }
}

#[cfg(unix)]
impl Drop for ForegroundProcessGroupGuard {
    fn drop(&mut self) {
        if self.active {
            terminate_process_group(self.pgid);
        }
    }
}

#[cfg(unix)]
fn configure_foreground_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_foreground_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_group(pgid: Pid) {
    let _ = killpg(pgid, Signal::SIGTERM);
    let _ = killpg(pgid, Signal::SIGKILL);
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
    #[serde(rename = "backgroundTaskId", skip_serializing_if = "Option::is_none")]
    pub background_task_id: Option<String>,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default)]
    pub interrupted: bool,
}

fn format_background_shell_output(stdout: &str, stderr: &str, timed_out: bool) -> String {
    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(stdout);
    }
    if !stderr.is_empty() {
        if !combined.ends_with('\n') && !combined.is_empty() {
            combined.push('\n');
        }
        if !combined.is_empty() {
            combined.push_str("[stderr]\n");
        }
        combined.push_str(stderr);
    }
    if timed_out {
        if !combined.ends_with('\n') && !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str("Command timed out");
    }
    crate::bash_tool::utils::format_output(&combined).truncated_content
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
            let task = crate::task_store::create_background_shell_task(
                inp.command.clone(),
                context.cwd.clone(),
                inp.description.clone(),
                timeout_ms,
            );
            let task_id = task.id.clone();

            let mut command = Command::new("bash");
            command
                .arg("-c")
                .arg(&inp.command)
                .current_dir(&context.cwd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            configure_foreground_process_group(&mut command);
            let child = match command.spawn() {
                Ok(child) => child,
                Err(e) => {
                    crate::task_store::finish_background_shell_task(
                        &task_id,
                        "failed",
                        format!("Failed to execute command: {e}"),
                        None,
                        false,
                    );
                    let output = ShellExecutorOutput {
                        stdout: None,
                        stderr: Some(format!("Failed to execute command: {e}")),
                        exit_code: None,
                        background_task_id: Some(task_id.clone()),
                        timed_out: false,
                        interrupted: false,
                    };
                    return Ok(ToolResult {
                        output: serde_json::to_string(&output)?,
                        is_error: true,
                        duration_ms: start.elapsed().as_millis() as u64,
                        metadata: HashMap::from([("task_id".to_string(), Value::String(task_id))]),
                    });
                }
            };

            if let Some(pid) = child.id() {
                crate::task_store::register_background_shell_process(&task_id, pid);
            }
            #[cfg(unix)]
            let mut process_group_guard = ForegroundProcessGroupGuard::from_child(&child);

            let command_text = inp.command.clone();
            let task_id_for_task = task_id.clone();
            tokio::spawn(async move {
                let result = tokio::select! {
                    res = child.wait_with_output() => {
                        #[cfg(unix)]
                        if let Some(guard) = process_group_guard.as_mut() {
                            guard.disarm();
                        }
                        match res {
                            Ok(o) => {
                                let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                                let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                                let status = if o.status.success() { "completed" } else { "failed" };
                                let output = format_background_shell_output(&stdout, &stderr, false);
                                (status.to_string(), output, o.status.code(), false)
                            }
                            Err(e) => {
                                (
                                    "failed".to_string(),
                                    format!("Failed to execute command: {e}"),
                                    None,
                                    false,
                                )
                            }
                        }
                    }
                    _ = tokio::time::sleep(duration) => {
                        warn!(command = %command_text, "ShellExecutor: background command timed out");
                        #[cfg(unix)]
                        if let Some(guard) = process_group_guard.as_mut() {
                            guard.terminate_now();
                        }
                        (
                            "failed".to_string(),
                            format_background_shell_output("", "", true),
                            None,
                            true,
                        )
                    }
                };
                crate::task_store::finish_background_shell_task(
                    &task_id_for_task,
                    &result.0,
                    result.1,
                    result.2,
                    result.3,
                );
            });

            let output = ShellExecutorOutput {
                stdout: Some(format!("Command started in background task: {task_id}")),
                stderr: None,
                exit_code: None,
                background_task_id: Some(task_id.clone()),
                timed_out: false,
                interrupted: false,
            };
            let mut metadata = HashMap::new();
            metadata.insert("task_id".to_string(), Value::String(task_id));
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error: false,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata,
            });
        }

        // 前台模式：执行命令并等待结果，带超时。
        let mut command = Command::new("bash");
        command
            .arg("-c")
            .arg(&inp.command)
            .current_dir(&context.cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        configure_foreground_process_group(&mut command);
        let child = command.spawn()?;
        #[cfg(unix)]
        let mut process_group_guard = ForegroundProcessGroupGuard::from_child(&child);

        let result = tokio::select! {
            res = child.wait_with_output() => {
                #[cfg(unix)]
                if let Some(guard) = process_group_guard.as_mut() {
                    guard.disarm();
                }
                match res {
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                        ShellExecutorOutput {
                            stdout: if stdout.is_empty() { None } else { Some(stdout) },
                            stderr: if stderr.is_empty() { None } else { Some(stderr) },
                            exit_code: o.status.code(),
                            background_task_id: None,
                            timed_out: false,
                            interrupted: false,
                        }
                    }
                    Err(e) => {
                        ShellExecutorOutput {
                            stdout: None,
                            stderr: Some(format!("Failed to execute command: {e}")),
                            exit_code: None,
                            background_task_id: None,
                            timed_out: false,
                            interrupted: false,
                        }
                    }
                }
            }
            _ = tokio::time::sleep(duration) => {
                warn!(command = %inp.command, "ShellExecutor: command timed out");
                #[cfg(unix)]
                if let Some(guard) = process_group_guard.as_mut() {
                    guard.terminate_now();
                }
                ShellExecutorOutput {
                    stdout: None,
                    stderr: Some("Command timed out".to_string()),
                    exit_code: None,
                    background_task_id: None,
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

#[cfg(test)]
mod tests {
    use super::ShellExecutor;
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use serde_json::Value;
    use std::collections::HashMap;

    async fn wait_for_task(task_id: &str, status: &str) -> crate::task_store::TaskRecord {
        for _ in 0..40 {
            if let Some(record) = crate::task_store::get_task(task_id) {
                if record.status == status {
                    return record;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("task {task_id} did not reach status {status}");
    }

    #[cfg(unix)]
    fn pid_is_alive(pid: u32) -> bool {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(unix)]
    fn kill_pid(pid: u32) {
        let _ = std::process::Command::new("kill")
            .arg("-KILL")
            .arg(pid.to_string())
            .stderr(std::process::Stdio::null())
            .status();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bash_timeout_kills_foreground_process_group_children() {
        let temp = tempfile::tempdir().expect("tempdir");
        let context = ToolUseContext {
            cwd: temp.path().to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };
        let input = serde_json::json!({
            "command": "sleep 30 & echo $! > child.pid; wait",
            "timeout": 100,
        });

        let result = ShellExecutor
            .execute(input, &context)
            .await
            .expect("bash result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");
        assert_eq!(output["timed_out"], true);

        let pid_text = std::fs::read_to_string(temp.path().join("child.pid"))
            .expect("child pid file was written");
        let child_pid: u32 = pid_text.trim().parse().expect("child pid");

        for _ in 0..40 {
            if !pid_is_alive(child_pid) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        kill_pid(child_pid);
        panic!("foreground process-group child survived Bash timeout");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bash_background_task_returns_id_and_records_output() {
        let temp = tempfile::tempdir().expect("tempdir");
        let context = ToolUseContext {
            cwd: temp.path().to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };
        let input = serde_json::json!({
            "command": "sleep 0.2; printf background-ok",
            "run_in_background": true,
            "timeout": 2_000,
        });

        let result = ShellExecutor
            .execute(input, &context)
            .await
            .expect("bash result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");
        let task_id = output["backgroundTaskId"]
            .as_str()
            .expect("background task id");
        assert_eq!(result.metadata["task_id"], task_id);
        let running = crate::task_store::get_task(task_id).expect("background task");
        assert_eq!(running.status, "in_progress");
        assert_eq!(running.metadata["type"].as_str(), Some("background_shell"));

        let completed = wait_for_task(task_id, "completed").await;
        assert_eq!(completed.exit_code, Some(0));
        assert!(completed.output.contains("background-ok"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bash_background_task_stop_kills_process_group_children() {
        let temp = tempfile::tempdir().expect("tempdir");
        let context = ToolUseContext {
            cwd: temp.path().to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };
        let input = serde_json::json!({
            "command": "sleep 30 & echo $! > child.pid; wait",
            "run_in_background": true,
            "timeout": 60_000,
        });

        let result = ShellExecutor
            .execute(input, &context)
            .await
            .expect("bash result");
        let output: Value = serde_json::from_str(&result.output).expect("json output");
        let task_id = output["backgroundTaskId"]
            .as_str()
            .expect("background task id");

        let pid_file = temp.path().join("child.pid");
        for _ in 0..40 {
            if pid_file.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let pid_text = std::fs::read_to_string(&pid_file).expect("child pid file was written");
        let child_pid: u32 = pid_text.trim().parse().expect("child pid");

        let stopped = crate::task_store::stop_background_task(task_id).expect("stopped task");
        assert_eq!(stopped.status, "cancelled");

        for _ in 0..40 {
            if !pid_is_alive(child_pid) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        kill_pid(child_pid);
        panic!("background process-group child survived TaskStop");
    }
}
