//! # exec_file_no_throw — 进程执行工具（不抛出异常）
//!
//! 对应 TypeScript `utils/execFileNoThrow.ts`。
//! 提供子进程执行并始终返回结果（不 panic）的封装。

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

const DEFAULT_TIMEOUT_SECS: u64 = 600; // 10 minutes

/// 进程执行选项
pub struct ExecFileOptions {
    pub timeout: Duration,
    pub preserve_output_on_error: bool,
    pub cwd: Option<String>,
    pub env: Option<Vec<(String, String)>>,
    pub stdin_mode: StdinMode,
    pub input: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum StdinMode {
    Ignore,
    Inherit,
    Pipe,
}

impl Default for ExecFileOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            preserve_output_on_error: true,
            cwd: None,
            env: None,
            stdin_mode: StdinMode::Ignore,
            input: None,
        }
    }
}

/// 进程执行结果
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
    pub error: Option<String>,
}

/// 获取错误消息的优先级：shortMessage > signal > exit code
fn get_error_message(short_message: Option<&str>, signal: Option<&str>, error_code: i32) -> String {
    if let Some(msg) = short_message {
        return msg.to_string();
    }
    if let Some(sig) = signal {
        return sig.to_string();
    }
    error_code.to_string()
}

/// 执行文件，始终返回结果不 panic
pub async fn exec_file_no_throw(file: &str, args: &[&str], options: ExecFileOptions) -> ExecResult {
    exec_file_no_throw_with_cwd(file, args, options).await
}

/// 带 cwd 的执行文件，始终返回结果不 panic
pub async fn exec_file_no_throw_with_cwd(
    file: &str,
    args: &[&str],
    options: ExecFileOptions,
) -> ExecResult {
    let mut cmd = Command::new(file);
    cmd.args(args);

    if let Some(ref cwd) = options.cwd {
        cmd.current_dir(Path::new(cwd));
    }

    if let Some(ref env_vars) = options.env {
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
    }

    match options.stdin_mode {
        StdinMode::Ignore => {
            cmd.stdin(Stdio::null());
        }
        StdinMode::Inherit => {
            cmd.stdin(Stdio::inherit());
        }
        StdinMode::Pipe => {
            cmd.stdin(Stdio::piped());
        }
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let result = tokio::time::timeout(options.timeout, async {
        let child = cmd.spawn();
        match child {
            Ok(child) => child.wait_with_output().await,
            Err(e) => Err(e),
        }
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let code = output.status.code().unwrap_or(1);

            if !output.status.success() {
                if options.preserve_output_on_error {
                    let error_msg = get_error_message(None, None, code);
                    ExecResult {
                        stdout,
                        stderr,
                        code,
                        error: Some(error_msg),
                    }
                } else {
                    ExecResult {
                        stdout: String::new(),
                        stderr: String::new(),
                        code,
                        error: None,
                    }
                }
            } else {
                ExecResult {
                    stdout,
                    stderr,
                    code: 0,
                    error: None,
                }
            }
        }
        Ok(Err(e)) => {
            tracing::error!("exec_file_no_throw spawn error: {}", e);
            ExecResult {
                stdout: String::new(),
                stderr: String::new(),
                code: 1,
                error: Some(e.to_string()),
            }
        }
        Err(_) => {
            // Timeout
            ExecResult {
                stdout: String::new(),
                stderr: String::new(),
                code: 1,
                error: Some("Command timed out".to_string()),
            }
        }
    }
}
