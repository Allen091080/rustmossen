//! # get_worktree_paths — 获取 Git worktree 路径
//!
//! 对应 TypeScript `utils/getWorktreePaths.ts`。

use std::path::MAIN_SEPARATOR;
use std::time::Instant;
use tokio::process::Command;

/// 规范化可比较路径
async fn canonicalize_comparable_path(path: &str) -> String {
    match tokio::fs::canonicalize(path).await {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => {
            // NFC 规范化（Rust 字符串默认使用 UTF-8，此处简单返回）
            path.to_string()
        }
    }
}

/// 事件日志回调类型
pub type LogEventFn = Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>;

/// 获取 git 可执行文件路径
fn git_exe() -> &'static str {
    "git"
}

/// 返回当前 git 仓库的所有 worktree 路径。
/// 如果 git 不可用、不在 git 仓库中或只有一个 worktree，
/// 返回空数组。
///
/// 此版本包含分析追踪并使用 CLI 的 gitExe() 解析器。
pub async fn get_worktree_paths(cwd: &str, log_event: Option<&LogEventFn>) -> Vec<String> {
    let start_time = Instant::now();
    let comparable_cwd = canonicalize_comparable_path(cwd).await;

    let output = Command::new(git_exe())
        .args(["worktree", "list", "--porcelain"])
        .current_dir(cwd)
        .output()
        .await;

    let duration_ms = start_time.elapsed().as_millis() as u64;

    let output = match output {
        Ok(o) => o,
        Err(_) => {
            if let Some(log) = log_event {
                log(
                    "mossen_worktree_detection",
                    &serde_json::json!({
                        "duration_ms": duration_ms,
                        "worktree_count": 0,
                        "success": false,
                    }),
                );
            }
            return Vec::new();
        }
    };

    if !output.status.success() {
        if let Some(log) = log_event {
            log(
                "mossen_worktree_detection",
                &serde_json::json!({
                    "duration_ms": duration_ms,
                    "worktree_count": 0,
                    "success": false,
                }),
            );
        }
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // 解析 porcelain 输出 - 以 "worktree " 开头的行包含路径
    let worktree_paths: Vec<String> = stdout
        .lines()
        .filter(|line| line.starts_with("worktree "))
        .map(|line| line["worktree ".len()..].to_string())
        .collect();

    if let Some(log) = log_event {
        log(
            "mossen_worktree_detection",
            &serde_json::json!({
                "duration_ms": duration_ms,
                "worktree_count": worktree_paths.len(),
                "success": true,
            }),
        );
    }

    // 排序 worktrees：当前 worktree 在前，其余按字母顺序
    let mut comparable_worktree_paths = Vec::new();
    for path in &worktree_paths {
        comparable_worktree_paths.push(canonicalize_comparable_path(path).await);
    }

    let sep = MAIN_SEPARATOR.to_string();
    let current_worktree_index = comparable_worktree_paths.iter().position(|path| {
        comparable_cwd == *path || comparable_cwd.starts_with(&format!("{}{}", path, sep))
    });

    let current_worktree = current_worktree_index.map(|i| worktree_paths[i].clone());

    let mut other_worktrees: Vec<String> = worktree_paths
        .iter()
        .filter(|p| Some(p.as_str()) != current_worktree.as_deref())
        .cloned()
        .collect();
    other_worktrees.sort();

    match current_worktree {
        Some(cw) => {
            let mut result = vec![cw];
            result.extend(other_worktrees);
            result
        }
        None => other_worktrees,
    }
}
