//! # exec_command — Command Hook 执行器
//!
//! 对应 TS `execCommandHook()` 中 Command 类型 Hook 的执行。
//! 使用 `tokio::process::Command` 执行 shell 命令。

use std::collections::HashMap;
use std::time::Duration;

use mossen_types::hooks::HookOutcome;
use tokio::process::Command;
use tracing::{debug, warn};

use super::settings::HookCommand;

/// Command Hook 执行上下文。
#[derive(Debug, Clone)]
pub struct CommandHookContext {
    /// 环境变量。
    pub env_vars: HashMap<String, String>,
    /// 工作目录。
    pub working_dir: Option<String>,
    /// 超时时间（秒）。
    pub timeout_secs: f64,
}

impl Default for CommandHookContext {
    fn default() -> Self {
        Self {
            env_vars: HashMap::new(),
            working_dir: None,
            timeout_secs: 600.0,
        }
    }
}

/// Command Hook 执行结果。
#[derive(Debug, Clone)]
pub struct CommandHookResult {
    /// 执行结果状态。
    pub outcome: HookOutcome,
    /// 标准输出。
    pub stdout: String,
    /// 标准错误。
    pub stderr: String,
    /// 退出码。
    pub exit_code: i32,
    /// JSON 输出（如果 stdout 包含有效 JSON）。
    pub json_output: Option<serde_json::Value>,
}

/// 执行 Command 类型 Hook。
///
/// 对应 TS 中的 shell 命令执行逻辑。
pub async fn exec_command_hook(
    command: &str,
    shell: Option<&str>,
    json_input: &str,
    ctx: &CommandHookContext,
) -> CommandHookResult {
    let shell_cmd = shell.unwrap_or("bash");
    debug!(
        command = command,
        shell = shell_cmd,
        "Executing command hook"
    );

    let mut cmd = Command::new(shell_cmd);
    cmd.arg("-c").arg(command);

    // 设置 HOOK_INPUT 环境变量
    cmd.env("HOOK_INPUT", json_input);

    // 设置额外环境变量
    for (key, value) in &ctx.env_vars {
        cmd.env(key, value);
    }

    // 设置工作目录
    if let Some(ref cwd) = ctx.working_dir {
        cmd.current_dir(cwd);
    }

    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let timeout_duration = Duration::from_secs_f64(ctx.timeout_secs);

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            warn!(command = command, "Failed to spawn command hook: {e}");
            return CommandHookResult {
                outcome: HookOutcome::NonBlockingError,
                stdout: String::new(),
                stderr: format!("Failed to spawn: {e}"),
                exit_code: -1,
                json_output: None,
            };
        }
    };

    let result = tokio::time::timeout(timeout_duration, child.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // 尝试从 stdout 中解析 JSON
            let json_output = stdout
                .lines()
                .find(|line| line.trim().starts_with('{'))
                .and_then(|line| serde_json::from_str(line.trim()).ok());

            let outcome = match exit_code {
                0 => HookOutcome::Success,
                2 => HookOutcome::Blocking,
                _ => HookOutcome::NonBlockingError,
            };

            debug!(
                command = command,
                exit_code = exit_code,
                outcome = ?outcome,
                "Command hook completed"
            );

            CommandHookResult {
                outcome,
                stdout,
                stderr,
                exit_code,
                json_output,
            }
        }
        Ok(Err(e)) => {
            warn!(command = command, "Command hook wait error: {e}");
            CommandHookResult {
                outcome: HookOutcome::NonBlockingError,
                stdout: String::new(),
                stderr: format!("Wait error: {e}"),
                exit_code: -1,
                json_output: None,
            }
        }
        Err(_) => {
            warn!(command = command, "Command hook timed out");
            CommandHookResult {
                outcome: HookOutcome::Cancelled,
                stdout: String::new(),
                stderr: format!("Timed out after {}s", ctx.timeout_secs),
                exit_code: -1,
                json_output: None,
            }
        }
    }
}
