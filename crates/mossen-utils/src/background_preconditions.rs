//! # background_preconditions — 远程会话前提条件检查
//!
//! 对应 TypeScript `utils/background/remote/preconditions.ts`。
//! 检查远程会话的前提条件（登录状态、Git 状态、GitHub App 安装等）。

use crate::auth::{
    check_and_refresh_oauth_token_if_needed, get_hosted_oauth_tokens, is_hosted_subscriber,
};
use crate::cwd::get_cwd;
use crate::git::{find_git_root, get_is_clean};

/// `BASE_API_URL` 的默认值；对应 TS `getOauthConfig().BASE_API_URL`。
/// Rust 端通过环境变量 `MOSSEN_CODE_API_BASE_URL` 覆盖，否则回落到 console 的默认值。
fn base_api_url() -> String {
    std::env::var("MOSSEN_CODE_API_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://console.mossen.invalid".to_string())
}

/// 检查桥接适配器凭据是否缺失或过期。
/// 返回 true 表示需要桥接适配器凭据，false 表示不需要。
pub async fn check_needs_hosted_login() -> bool {
    if !is_hosted_subscriber() {
        return false;
    }
    check_and_refresh_oauth_token_if_needed(0, false).await
}

/// 检查 git 工作目录是否干净（没有未提交的更改）。
/// 忽略未跟踪的文件，因为它们在分支切换时不会丢失。
/// 返回 true 表示 git 是干净的，false 表示不是。
pub async fn check_is_git_clean() -> bool {
    get_is_clean(true).await
}

/// 检查用户是否有远程环境访问权限。
/// 返回 true 表示用户有远程环境，false 表示没有。
pub async fn check_has_remote_environment() -> bool {
    // 需要 access_token 和 org_uuid 才能调用 fetch_environments。
    // 若任一缺失，按照 TS 端 catch 分支处理：返回 false。
    let tokens = match get_hosted_oauth_tokens() {
        Some(t) if !t.access_token.is_empty() => t,
        _ => return false,
    };

    // Rust 端尚未集中暴露 `getOrganizationUUID`，因此回落到从 OAuth 范围或环境推断；
    // 如果没有就退到 false（与 TS catch 等价）。
    let org_uuid = match std::env::var("MOSSEN_ORGANIZATION_UUID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            tracing::debug!(
                "check_has_remote_environment: organization UUID unavailable, treating as no remote env"
            );
            return false;
        }
    };

    let client = reqwest::Client::new();
    match crate::teleport::environments::fetch_environments(
        &client,
        &base_api_url(),
        &tokens.access_token,
        &org_uuid,
    )
    .await
    {
        Ok(envs) => !envs.is_empty(),
        Err(e) => {
            tracing::debug!("check_has_remote_environment failed: {}", e);
            false
        }
    }
}

/// 检查当前目录是否在 git 仓库中（有 .git/ 目录）。
/// 不同于 check_has_git_remote — 本地仓库可以通过此检查但不能通过那个。
pub fn check_is_in_git_repo() -> bool {
    find_git_root(&get_cwd()).is_some()
}

/// 检查当前仓库是否配置了 GitHub remote。
/// 对于只有本地仓库（git init 没有 origin）返回 false。
pub async fn check_has_git_remote() -> bool {
    let cwd = get_cwd();
    let origin = fetch_origin_url().await;
    crate::detect_repository::detect_current_repository(&cwd, async move { origin })
        .await
        .is_some()
}

/// 检查 GitHub App 是否安装在特定仓库上。
///
/// * `owner` - 仓库所有者（例如 "mossen"）
/// * `repo` - 仓库名称（例如 "mossen-cli-internal"）
/// 返回 true 表示 GitHub App 已安装，false 表示没有。
pub async fn check_github_app_installed(owner: &str, repo: &str) -> bool {
    let tokens = match get_hosted_oauth_tokens() {
        Some(t) if !t.access_token.is_empty() => t,
        _ => {
            tracing::debug!("checkGithubAppInstalled: No access token, assuming app not installed");
            return false;
        }
    };

    let org_uuid = match std::env::var("MOSSEN_ORGANIZATION_UUID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            tracing::debug!("checkGithubAppInstalled: No org UUID, assuming app not installed");
            return false;
        }
    };

    crate::background::preconditions::check_github_app_installed(
        owner,
        repo,
        Some(&tokens.access_token),
        Some(&org_uuid),
        &base_api_url(),
    )
    .await
}

/// 检查用户是否通过 /web-setup 同步了 GitHub 凭据。
/// 返回 true 表示 GitHub token 已同步，false 表示没有。
pub async fn check_github_token_synced() -> bool {
    let tokens = match get_hosted_oauth_tokens() {
        Some(t) if !t.access_token.is_empty() => t,
        _ => return false,
    };

    let org_uuid = match std::env::var("MOSSEN_ORGANIZATION_UUID") {
        Ok(v) if !v.is_empty() => v,
        _ => return false,
    };

    crate::background::preconditions::check_github_token_synced(
        Some(&tokens.access_token),
        Some(&org_uuid),
        &base_api_url(),
    )
    .await
}

/// 仓库访问方法
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoAccessMethod {
    GithubApp,
    TokenSync,
    None,
}

/// 对 GitHub 仓库进行分层检查以确定是否可以进行远程操作。
/// 1. GitHub App 安装在仓库上
/// 2. 通过 /web-setup 同步的 GitHub token
/// 3. 两者都没有 — 调用者应提示用户设置访问权限
pub async fn check_repo_for_remote_access(owner: &str, repo: &str) -> (bool, RepoAccessMethod) {
    if check_github_app_installed(owner, repo).await {
        return (true, RepoAccessMethod::GithubApp);
    }

    // `mossen_cobalt_lantern` feature flag — Rust 端通过环境变量启用，没有时
    // 默认为 false（与 TS `getFeatureValue_CACHED_MAY_BE_STALE(_, false)` 等价）。
    let cobalt_lantern_on = matches!(
        std::env::var("MOSSEN_FEATURE_MOSSEN_COBALT_LANTERN")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("yes")
    );
    if cobalt_lantern_on && check_github_token_synced().await {
        return (true, RepoAccessMethod::TokenSync);
    }

    (false, RepoAccessMethod::None)
}

/// 获取当前仓库 origin URL（封装 `git remote get-url origin`）。
async fn fetch_origin_url() -> Option<String> {
    let output = tokio::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(get_cwd())
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_is_in_git_repo() {
        // 需要在 git 仓库中运行测试
        let _ = check_is_in_git_repo();
    }
}
