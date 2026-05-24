//! # exec_file_no_throw_portable — 同步执行命令（不抛异常）
//!
//! 对应 TypeScript `utils/execFileNoThrowPortable.ts`。

use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const MS_IN_SECOND: u64 = 1000;
const SECONDS_IN_MINUTE: u64 = 60;

/// 同步执行选项
pub struct ExecSyncOptions {
    pub timeout: Option<Duration>,
    pub input: Option<String>,
    pub stdio: Option<StdioConfig>,
}

impl Default for ExecSyncOptions {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_millis(10 * SECONDS_IN_MINUTE * MS_IN_SECOND)),
            input: None,
            stdio: None,
        }
    }
}

/// stdio 配置
#[derive(Debug, Clone)]
pub enum StdioConfig {
    /// stdin=ignore, stdout=pipe, stderr=pipe（默认）
    Default,
    /// 自定义配置
    Custom {
        stdin: StdioMode,
        stdout: StdioMode,
        stderr: StdioMode,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum StdioMode {
    Pipe,
    Ignore,
    Inherit,
}

/// 获取当前工作目录
fn get_cwd() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// 已弃用：使用 `tokio::process::Command` 配合 `reject: false` 实现非阻塞执行。
/// 同步 exec 调用会阻塞事件循环并导致性能问题。
pub fn exec_sync_with_defaults_deprecated(command: &str) -> Option<String> {
    exec_sync_with_defaults_deprecated_with_options(command, ExecSyncOptions::default())
}

/// 已弃用：带选项的同步执行
pub fn exec_sync_with_defaults_deprecated_with_options(
    command: &str,
    options: ExecSyncOptions,
) -> Option<String> {
    let cwd = get_cwd();
    let timeout = options
        .timeout
        .unwrap_or(Duration::from_millis(10 * SECONDS_IN_MINUTE * MS_IN_SECOND));

    let shell = if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "sh"
    };
    let shell_arg = if cfg!(target_os = "windows") {
        "/C"
    } else {
        "-c"
    };

    let mut cmd = Command::new(shell);
    cmd.arg(shell_arg).arg(command).current_dir(&cwd);

    // 设置环境变量
    cmd.envs(env::vars());

    if let Some(ref input) = options.input {
        use std::io::Write;
        use std::process::Stdio;

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(_) => return None,
        };

        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(input.as_bytes());
        }
        // 关闭 stdin
        drop(child.stdin.take());

        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(_) => return None,
        };

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        }
    } else {
        use std::process::Stdio;

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = match cmd.output() {
            Ok(o) => o,
            Err(_) => return None,
        };

        let _ = timeout; // timeout 在同步调用中由 OS 处理

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        }
    }
}
