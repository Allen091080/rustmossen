//! # cross_project_resume — 跨项目恢复
//!
//! 对应 TypeScript `utils/crossProjectResume.ts`。
//! 检测恢复会话是否来自不同项目目录。

use std::path::MAIN_SEPARATOR;

/// 跨项目恢复结果
#[derive(Debug, Clone)]
pub enum CrossProjectResumeResult {
    /// 不是跨项目
    NotCrossProject,
    /// 是同一仓库的 worktree
    SameRepoWorktree { project_path: String },
    /// 不同的项目
    DifferentProject { command: String, project_path: String },
}

/// 日志选项（简化）
pub struct LogOption {
    pub project_path: Option<String>,
    pub session_id: Option<String>,
}

/// 检查日志是否来自不同项目目录，并确定是相关 worktree 还是完全不同的项目。
///
/// 对于同一仓库的 worktree，可以直接恢复无需 cd。
/// 对于不同项目，生成 cd 命令。
pub fn check_cross_project_resume(
    log: &LogOption,
    show_all_projects: bool,
    worktree_paths: &[String],
    current_cwd: &str,
    user_type: Option<&str>,
) -> CrossProjectResumeResult {
    if !show_all_projects {
        return CrossProjectResumeResult::NotCrossProject;
    }

    let project_path = match &log.project_path {
        Some(p) if p != current_cwd => p.clone(),
        _ => return CrossProjectResumeResult::NotCrossProject,
    };

    let session_id = log.session_id.as_deref().unwrap_or("");

    // 仅对 ant 用户进行 worktree 检测（分阶段推出）
    if user_type != Some("ant") {
        let command = format!(
            "cd {} && mossen --resume {}",
            shell_quote(&project_path),
            session_id
        );
        return CrossProjectResumeResult::DifferentProject {
            command,
            project_path,
        };
    }

    // 检查 log.projectPath 是否在同一仓库的 worktree 下
    let sep = MAIN_SEPARATOR.to_string();
    let is_same_repo = worktree_paths.iter().any(|wt| {
        project_path == *wt || project_path.starts_with(&format!("{}{}", wt, sep))
    });

    if is_same_repo {
        return CrossProjectResumeResult::SameRepoWorktree { project_path };
    }

    // 不同的仓库——生成 cd 命令
    let command = format!(
        "cd {} && mossen --resume {}",
        shell_quote(&project_path),
        session_id
    );
    CrossProjectResumeResult::DifferentProject {
        command,
        project_path,
    }
}

/// 简单的 shell 引用
fn shell_quote(s: &str) -> String {
    if s.contains(' ') || s.contains('\'') || s.contains('"') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}
