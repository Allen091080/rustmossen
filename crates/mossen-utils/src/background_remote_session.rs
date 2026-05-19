//! # background_remote_session — 背景远程会话
//!
//! 对应 TypeScript `utils/background/remote/remoteSession.ts`。
//! 管理远程会话的后台会话类型和前提条件检查。

use serde::{Deserialize, Serialize};

use crate::background_preconditions::{
    check_github_app_installed, check_has_remote_environment, check_is_in_git_repo,
    check_needs_hosted_login,
};
use crate::cwd::get_cwd;

/// 背景远程会话类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundRemoteSession {
    pub id: String,
    pub command: String,
    pub start_time: u64,
    pub status: SessionStatus,
    pub title: String,
    pub session_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Starting,
    Running,
    Completed,
    Failed,
    Killed,
}

/// 背景远程会话前提条件失败
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BackgroundRemoteSessionPrecondition {
    NotLoggedIn,
    NoRemoteEnvironment,
    NotInGitRepo,
    NoGitRemote,
    GithubAppNotInstalled,
    PolicyBlocked,
}

/// 是否允许远程会话策略；对应 TS `isPolicyAllowed('allow_remote_sessions')`。
///
/// Rust 端尚未有完整策略服务，因此读取 `MOSSEN_POLICY_DISABLE_REMOTE_SESSIONS`
/// 作为关闭开关——管理员设置该环境变量即可阻断远程会话；缺省允许。
fn is_remote_sessions_policy_allowed() -> bool {
    !matches!(
        std::env::var("MOSSEN_POLICY_DISABLE_REMOTE_SESSIONS")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// 是否启用 CCR bundle seeding 路径；对应 TS 中 `CCR_FORCE_BUNDLE`、
/// `CCR_ENABLE_BUNDLE` 以及 `tengu_ccr_bundle_seed_enabled` GrowthBook gate。
/// Rust 端只检查环境变量（GrowthBook 桥接未到位）。
fn bundle_seed_gate_on(skip_bundle: bool) -> bool {
    if skip_bundle {
        return false;
    }
    let truthy = |k: &str| {
        matches!(
            std::env::var(k).ok().as_deref(),
            Some("1") | Some("true") | Some("yes")
        )
    };
    truthy("CCR_FORCE_BUNDLE")
        || truthy("CCR_ENABLE_BUNDLE")
        || truthy("MOSSEN_FEATURE_TENGU_CCR_BUNDLE_SEED_ENABLED")
}

/// 在当前 cwd 下探测仓库（origin URL 经 `git remote get-url`）。
async fn detect_repo_in_cwd() -> Option<crate::detect_repository::ParsedRepository> {
    let cwd = get_cwd();
    let origin = fetch_origin_url(&cwd).await;
    crate::detect_repository::detect_current_repository_with_host(&cwd, async move { origin })
        .await
}

async fn fetch_origin_url(cwd: &str) -> Option<String> {
    let output = tokio::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(cwd)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() { None } else { Some(url) }
}

/// 检查创建背景远程会话的资格。
/// 返回失败的前提条件数组（空数组表示所有检查都通过）。
pub async fn check_background_remote_session_eligibility(
    skip_bundle: bool,
) -> Vec<BackgroundRemoteSessionPrecondition> {
    let mut errors = Vec::new();

    if !is_remote_sessions_policy_allowed() {
        errors.push(BackgroundRemoteSessionPrecondition::PolicyBlocked);
        return errors;
    }

    let (needs_login, has_remote_env, repository) = tokio::join!(
        check_needs_hosted_login(),
        check_has_remote_environment(),
        detect_repo_in_cwd(),
    );

    if needs_login {
        errors.push(BackgroundRemoteSessionPrecondition::NotLoggedIn);
    }

    if !has_remote_env {
        errors.push(BackgroundRemoteSessionPrecondition::NoRemoteEnvironment);
    }

    let bundle_on = bundle_seed_gate_on(skip_bundle);

    if !check_is_in_git_repo() {
        errors.push(BackgroundRemoteSessionPrecondition::NotInGitRepo);
    } else if bundle_on {
        // 有 .git/ 且启用 bundle seeding；跳过 remote + GitHub App 检查。
    } else if repository.is_none() {
        errors.push(BackgroundRemoteSessionPrecondition::NoGitRemote);
    } else if let Some(repo) = repository {
        if repo.host == "github.com" {
            let has_github_app = check_github_app_installed(&repo.owner, &repo.name).await;
            if !has_github_app {
                errors.push(BackgroundRemoteSessionPrecondition::GithubAppNotInstalled);
            }
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_status_serialization() {
        let status = SessionStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"running\"");
    }
}
