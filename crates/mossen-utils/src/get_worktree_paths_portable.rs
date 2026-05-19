//! # get_worktree_paths_portable — 便携 Worktree 路径检测
//!
//! 对应 TypeScript `utils/getWorktreePathsPortable.ts`。
//! 仅使用 child_process，无其他依赖。

use std::process::Command;
use std::time::Duration;

/// 使用 `git worktree list --porcelain` 获取 worktree 路径列表。
///
/// 便携实现，不依赖 execa 或其他重量级模块。
pub async fn get_worktree_paths_portable(cwd: &str) -> Vec<String> {
    get_worktree_paths_portable_sync(cwd)
}

/// 同步版本的 worktree 路径检测。
pub fn get_worktree_paths_portable_sync(cwd: &str) -> Vec<String> {
    let result = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(cwd)
        .output();

    match result {
        Ok(output) => {
            if !output.status.success() {
                return Vec::new();
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.is_empty() {
                return Vec::new();
            }
            stdout
                .lines()
                .filter(|line| line.starts_with("worktree "))
                .map(|line| {
                    let path = &line["worktree ".len()..];
                    // NFC normalize (on most systems this is already NFC)
                    path.to_string()
                })
                .collect()
        }
        Err(_) => Vec::new(),
    }
}
