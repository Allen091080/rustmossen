//! # exec_sync_wrapper — 同步执行命令封装
//!
//! 对应 TypeScript `utils/execSyncWrapper.ts`。
//! 带慢操作日志的 execSync 封装。

use std::process::{Command, Output};

/// 同步执行命令（已废弃——优先使用异步方案）。
///
/// 封装了慢操作日志记录。
#[deprecated(note = "Use async alternatives when possible. Sync exec calls block.")]
pub fn exec_sync_deprecated(command: &str) -> Result<Output, std::io::Error> {
    let _span = tracing::debug_span!("exec_sync", cmd = &command[..command.len().min(100)]).entered();

    if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", command]).output()
    } else {
        Command::new("sh").args(["-c", command]).output()
    }
}

/// 同步执行命令并返回 stdout 字符串。
#[deprecated(note = "Use async alternatives when possible.")]
pub fn exec_sync_string(command: &str) -> Result<String, String> {
    #[allow(deprecated)]
    let output = exec_sync_deprecated(command)
        .map_err(|e| format!("exec error: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
