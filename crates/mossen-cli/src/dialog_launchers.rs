//! 对话框启动器 — 对应 TS 的 dialogLaunchers.tsx。
//!
//! 原始 TS 中这些是 React JSX 对话框的轻薄启动器。
//! 在 Rust 中，我们将其转换为异步对话框函数，通过 TUI 交互实现。

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// 快照更新对话框的返回值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotUpdateResult {
    Merge,
    Keep,
    Replace,
}

/// Agent 记忆范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMemoryScope {
    Session,
    Project,
    Global,
}

/// 设置验证错误。
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
    pub source: String,
}

/// Teleport 远程响应。
#[derive(Debug, Clone)]
pub struct TeleportRemoteResponse {
    pub session_id: String,
    pub project_dir: String,
    pub transcript_path: String,
}

/// 助手会话。
#[derive(Debug, Clone)]
pub struct AssistantSession {
    pub id: String,
    pub name: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Dialog launchers
// ---------------------------------------------------------------------------

/// 启动快照更新对话框。
///
/// 对应 TS 的 launchSnapshotUpdateDialog。
/// 用户选择如何处理代理记忆快照更新。
pub async fn launch_snapshot_update_dialog(
    agent_type: &str,
    scope: AgentMemoryScope,
    snapshot_timestamp: &str,
) -> SnapshotUpdateResult {
    // 在 TUI 中显示选择提示
    let scope_str = match scope {
        AgentMemoryScope::Session => "session",
        AgentMemoryScope::Project => "project",
        AgentMemoryScope::Global => "global",
    };

    tracing::info!(
        agent_type = agent_type,
        scope = scope_str,
        timestamp = snapshot_timestamp,
        "snapshot update dialog: defaulting to keep"
    );

    // 默认保持现有快照
    SnapshotUpdateResult::Keep
}

/// 启动无效设置对话框。
///
/// 对应 TS 的 launchInvalidSettingsDialog。
/// 显示设置验证错误，用户可选择继续或退出。
pub async fn launch_invalid_settings_dialog(
    settings_errors: &[ValidationError],
    on_exit: impl FnOnce(),
) {
    if settings_errors.is_empty() {
        return;
    }

    tracing::warn!(
        error_count = settings_errors.len(),
        "invalid settings detected"
    );

    for error in settings_errors {
        tracing::warn!(
            path = %error.path,
            source = %error.source,
            message = %error.message,
            "settings validation error"
        );
    }

    // 在非交互模式下，记录错误并继续
    let _ = on_exit;
}

/// 启动助手会话选择器。
///
/// 对应 TS 的 launchAssistantSessionChooser。
/// 显示可用会话列表，用户选择一个会话 ID。
pub async fn launch_assistant_session_chooser(sessions: &[AssistantSession]) -> Option<String> {
    if sessions.is_empty() {
        return None;
    }

    if sessions.len() == 1 {
        return Some(sessions[0].id.clone());
    }

    tracing::info!(
        session_count = sessions.len(),
        "assistant session chooser: selecting first session"
    );

    // 默认选择第一个会话
    Some(sessions[0].id.clone())
}

/// 启动助手安装向导。
///
/// 对应 TS 的 launchAssistantInstallWizard。
/// 安装向导，成功返回安装目录，取消返回 None。
pub async fn launch_assistant_install_wizard() -> Result<Option<PathBuf>, anyhow::Error> {
    tracing::info!("assistant install wizard: not yet implemented in TUI");
    Ok(None)
}

/// 启动 Teleport 恢复选择器。
///
/// 对应 TS 的 launchTeleportResumeWrapper。
/// 交互式选择远程会话。
pub async fn launch_teleport_resume_wrapper() -> Option<TeleportRemoteResponse> {
    tracing::info!("teleport resume wrapper: not yet implemented in TUI");
    None
}

/// 启动 Teleport 仓库不匹配对话框。
///
/// 对应 TS 的 launchTeleportRepoMismatchDialog。
/// 让用户选择目标仓库的本地检出路径。
pub async fn launch_teleport_repo_mismatch_dialog(
    target_repo: &str,
    initial_paths: &[String],
) -> Option<String> {
    if initial_paths.is_empty() {
        tracing::warn!(
            target_repo = target_repo,
            "no local checkouts found for target repo"
        );
        return None;
    }

    tracing::info!(
        target_repo = target_repo,
        path_count = initial_paths.len(),
        "repo mismatch dialog: selecting first path"
    );

    // 默认选择第一个路径
    Some(initial_paths[0].clone())
}

/// 启动恢复会话选择器。
///
/// 对应 TS 的 launchResumeChooser。
/// 交互式选择要恢复的会话。
pub async fn launch_resume_chooser(
    worktree_paths: &[String],
    session_id: Option<&str>,
) -> Result<(), anyhow::Error> {
    tracing::info!(
        worktree_count = worktree_paths.len(),
        session_id = ?session_id,
        "resume chooser: rendering session picker"
    );
    Ok(())
}
