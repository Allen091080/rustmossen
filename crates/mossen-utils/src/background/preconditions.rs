//! Preconditions for background remote sessions
//! Translated from utils/background/remote/preconditions.ts

use anyhow::Result;
use std::path::Path;
use tracing::debug;

/// Checks if the explicit Mossen bridge adapter credential is missing or stale.
pub async fn check_needs_hosted_login(is_hosted_subscriber: bool) -> bool {
    if !is_hosted_subscriber {
        return false;
    }
    // In production, calls check_and_refresh_oauth_token_if_needed()
    // Placeholder: check if token refresh is needed
    check_and_refresh_oauth_token_if_needed().await
}

/// Check and refresh OAuth token if needed
async fn check_and_refresh_oauth_token_if_needed() -> bool {
    // This would integrate with the auth system
    // Returns true if login is needed
    false
}

/// Checks if git working directory is clean (no uncommitted changes).
/// Ignores untracked files since they won't be lost during branch switching.
pub async fn check_is_git_clean(cwd: &Path) -> Result<bool> {
    let output = tokio::process::Command::new("git")
        .args(["status", "--porcelain", "-uno"])
        .current_dir(cwd)
        .output()
        .await?;
    Ok(output.stdout.is_empty())
}

/// Checks if user has access to at least one remote environment
pub async fn check_has_remote_environment(
    fetch_environments: impl std::future::Future<Output = Result<Vec<String>>>,
) -> bool {
    match fetch_environments.await {
        Ok(environments) => !environments.is_empty(),
        Err(e) => {
            debug!("checkHasRemoteEnvironment failed: {}", e);
            false
        }
    }
}

/// Checks if current directory is inside a git repository (has .git/).
pub fn check_is_in_git_repo(cwd: &Path) -> bool {
    find_git_root(cwd).is_some()
}

/// Walk up directories to find a .git directory
fn find_git_root(start: &Path) -> Option<std::path::PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Checks if current repository has a GitHub remote configured.
pub async fn check_has_git_remote(cwd: &Path) -> bool {
    let output = tokio::process::Command::new("git")
        .args(["remote", "-v"])
        .current_dir(cwd)
        .output()
        .await;
    match output {
        Ok(out) => !out.stdout.is_empty(),
        Err(_) => false,
    }
}

/// Checks if GitHub app is installed on a specific repository
pub async fn check_github_app_installed(
    owner: &str,
    repo: &str,
    access_token: Option<&str>,
    org_uuid: Option<&str>,
    base_api_url: &str,
) -> bool {
    let access_token = match access_token {
        Some(t) => t,
        None => {
            debug!("checkGithubAppInstalled: No access token found, assuming app not installed");
            return false;
        }
    };

    let org_uuid = match org_uuid {
        Some(u) => u,
        None => {
            debug!("checkGithubAppInstalled: No org UUID found, assuming app not installed");
            return false;
        }
    };

    let url = format!(
        "{}/api/oauth/organizations/{}/code/repos/{}/{}",
        base_api_url, org_uuid, owner, repo
    );

    debug!("Checking GitHub app installation for {}/{}", owner, repo);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-organization-uuid", org_uuid)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await;

    match response {
        Ok(resp) => {
            let status_code = resp.status().as_u16();
            if status_code == 200 {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(status) = body.get("status") {
                        if status.is_null() {
                            debug!(
                                "GitHub app is not installed on {}/{} (status is null)",
                                owner, repo
                            );
                            return false;
                        }
                        let installed = status
                            .get("app_installed")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        debug!(
                            "GitHub app {} installed on {}/{}",
                            if installed { "is" } else { "is not" },
                            owner,
                            repo
                        );
                        return installed;
                    }
                }
            }
            debug!(
                "checkGithubAppInstalled: Unexpected response status {}",
                status_code
            );
            false
        }
        Err(e) => {
            debug!("checkGithubAppInstalled error: {}", e);
            false
        }
    }
}

/// Checks if the user has synced their GitHub credentials via /web-setup
pub async fn check_github_token_synced(
    access_token: Option<&str>,
    org_uuid: Option<&str>,
    base_api_url: &str,
) -> bool {
    let access_token = match access_token {
        Some(t) => t,
        None => {
            debug!("checkGithubTokenSynced: No access token found");
            return false;
        }
    };

    let org_uuid = match org_uuid {
        Some(u) => u,
        None => {
            debug!("checkGithubTokenSynced: No org UUID found");
            return false;
        }
    };

    let url = format!(
        "{}/api/oauth/organizations/{}/sync/github/auth",
        base_api_url, org_uuid
    );

    debug!("Checking if GitHub token is synced via web-setup");

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-organization-uuid", org_uuid)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await;

    match response {
        Ok(resp) => {
            let status_code = resp.status().as_u16();
            if status_code == 200 {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let synced = body
                        .get("is_authenticated")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    debug!("GitHub token synced: {} (status={})", synced, status_code);
                    return synced;
                }
            }
            if (400..500).contains(&status_code) {
                debug!(
                    "checkGithubTokenSynced: Got {}, token not synced",
                    status_code
                );
            }
            false
        }
        Err(e) => {
            debug!("checkGithubTokenSynced error: {}", e);
            false
        }
    }
}

/// Repo access method for remote operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoAccessMethod {
    GithubApp,
    TokenSync,
    None,
}

/// Result of checking repo access
pub struct RepoAccessResult {
    pub has_access: bool,
    pub method: RepoAccessMethod,
}

/// Tiered check for whether a GitHub repo is accessible for remote operations.
pub async fn check_repo_for_remote_access(
    owner: &str,
    repo: &str,
    access_token: Option<&str>,
    org_uuid: Option<&str>,
    base_api_url: &str,
    token_sync_feature_enabled: bool,
) -> RepoAccessResult {
    if check_github_app_installed(owner, repo, access_token, org_uuid, base_api_url).await {
        return RepoAccessResult {
            has_access: true,
            method: RepoAccessMethod::GithubApp,
        };
    }
    if token_sync_feature_enabled
        && check_github_token_synced(access_token, org_uuid, base_api_url).await
    {
        return RepoAccessResult {
            has_access: true,
            method: RepoAccessMethod::TokenSync,
        };
    }
    RepoAccessResult {
        has_access: false,
        method: RepoAccessMethod::None,
    }
}
