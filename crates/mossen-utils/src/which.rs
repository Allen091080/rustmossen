//! # which — 查找命令可执行文件的完整路径
//!
//! 对应 TypeScript `utils/which.ts`。

use std::process::Command;
use tokio::process::Command as AsyncCommand;

/// 异步查找命令的完整路径。
///
/// 在 Windows 上使用 where.exe，在 POSIX 系统上使用 which。
pub async fn which(command: &str) -> Option<String> {
    if cfg!(target_os = "windows") {
        which_windows_async(command).await
    } else {
        which_posix_async(command).await
    }
}

/// 同步查找命令的完整路径。
pub fn which_sync(command: &str) -> Option<String> {
    if cfg!(target_os = "windows") {
        which_windows_sync(command)
    } else {
        which_posix_sync(command)
    }
}

async fn which_windows_async(command: &str) -> Option<String> {
    let output = AsyncCommand::new("where.exe")
        .arg(command)
        .output()
        .await
        .ok()?;

    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    // where.exe 返回多个路径，用换行分隔，返回第一个
    trimmed.lines().next().map(|s| s.to_string())
}

async fn which_posix_async(command: &str) -> Option<String> {
    let output = AsyncCommand::new("which")
        .arg(command)
        .output()
        .await
        .ok()?;

    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn which_windows_sync(command: &str) -> Option<String> {
    let output = Command::new("where.exe").arg(command).output().ok()?;

    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    trimmed.lines().next().map(|s| s.to_string())
}

fn which_posix_sync(command: &str) -> Option<String> {
    let output = Command::new("which").arg(command).output().ok()?;

    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
