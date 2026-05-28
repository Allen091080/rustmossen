//! # worktree_resume — 工作树恢复辅助
//!
//! 对应 TypeScript `utils/worktreeResume.ts`。
//! 提供工作树会话的路径比较、日志过滤和优先级排序功能。

use unicode_normalization::UnicodeNormalization;

/// 日志选项（会话条目）
#[derive(Debug, Clone)]
pub struct LogOption {
    pub project_path: Option<String>,
    pub worktree_session: Option<WorktreeSession>,
    // 其他字段由上层提供
}

/// 工作树会话信息
#[derive(Debug, Clone)]
pub struct WorktreeSession {
    pub worktree_name: Option<String>,
}

/// 展开路径的可比较变体（处理 macOS /private 前缀）。
fn expand_comparable_path_variants(path: &str) -> Vec<String> {
    let normalized: String = path.nfc().collect();
    let mut variants = vec![normalized.clone()];

    if cfg!(target_os = "macos") {
        if normalized.starts_with("/private/") {
            variants.push(normalized["/private".len()..].to_string());
        } else if normalized == "/tmp"
            || normalized.starts_with("/tmp/")
            || normalized == "/var"
            || normalized.starts_with("/var/")
        {
            variants.push(format!("/private{}", normalized));
        }
    }

    variants
}

/// 检查两个路径是否可比较相等。
fn are_comparable_paths_equal(left: Option<&str>, right: Option<&str>) -> bool {
    let (left, right) = match (left, right) {
        (Some(l), Some(r)) => (l, r),
        _ => return false,
    };

    let right_variants: std::collections::HashSet<String> =
        expand_comparable_path_variants(right).into_iter().collect();
    expand_comparable_path_variants(left)
        .iter()
        .any(|candidate| right_variants.contains(candidate))
}

/// 检查日志条目是否在当前工作树中。
pub fn is_log_in_current_worktree(log: &LogOption, current_cwd: &str) -> bool {
    are_comparable_paths_equal(log.project_path.as_deref(), Some(current_cwd))
}

/// 优先排序当前工作树的日志。
///
/// 将当前工作树的日志排在前面，其余保持原有顺序。
pub fn prioritize_current_worktree_logs(logs: &[LogOption], current_cwd: &str) -> Vec<LogOption> {
    let current_worktree_logs: Vec<&LogOption> = logs
        .iter()
        .filter(|log| is_log_in_current_worktree(log, current_cwd))
        .collect();

    if current_worktree_logs.is_empty() || current_worktree_logs.len() == logs.len() {
        return logs.to_vec();
    }

    let mut result: Vec<LogOption> = Vec::with_capacity(logs.len());
    // 先添加当前工作树的日志
    for log in logs {
        if is_log_in_current_worktree(log, current_cwd) {
            result.push(log.clone());
        }
    }
    // 再添加其他日志
    for log in logs {
        if !is_log_in_current_worktree(log, current_cwd) {
            result.push(log.clone());
        }
    }
    result
}

/// 选择首选的当前工作树日志。
///
/// 如果只有一个日志，返回它。
/// 如果恰好有一个当前工作树日志，返回它。
/// 否则返回 None。
pub fn select_preferred_current_worktree_log<'a>(
    logs: &'a [LogOption],
    current_cwd: &str,
) -> Option<&'a LogOption> {
    if logs.len() == 1 {
        return logs.first();
    }

    let current_worktree_logs: Vec<&LogOption> = logs
        .iter()
        .filter(|log| is_log_in_current_worktree(log, current_cwd))
        .collect();

    if current_worktree_logs.len() == 1 {
        return current_worktree_logs.into_iter().next();
    }

    None
}

/// 获取工作树元数据后缀文本。
///
/// 根据日志的工作树信息，返回适当的后缀显示文本。
pub fn get_worktree_metadata_suffix(log: &LogOption, current_cwd: &str) -> String {
    let project_path = match &log.project_path {
        Some(p) => p,
        None => return String::new(),
    };

    if is_log_in_current_worktree(log, current_cwd) {
        return " · current worktree".to_string();
    }

    if let Some(ref session) = log.worktree_session {
        if let Some(ref name) = session.worktree_name {
            return format!(" · {}", name);
        }
    }

    format!(" · {}", project_path)
}
