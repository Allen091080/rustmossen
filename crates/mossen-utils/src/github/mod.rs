//! GitHub utilities — translated from utils/github/ghAuthStatus.ts

use std::process::Command;

/// GitHub CLI authentication status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhAuthStatus {
    Authenticated,
    NotAuthenticated,
    NotInstalled,
}

/// Returns gh CLI install + auth status for telemetry.
/// Uses which() first to detect install, then exit code of `gh auth token`
/// to detect auth. Uses `auth token` instead of `auth status` because the
/// latter makes a network request to GitHub's API, while `auth token` only
/// reads local config/keyring.
pub async fn get_gh_auth_status() -> GhAuthStatus {
    // Check if gh is installed
    let gh_path = which::which("gh");
    if gh_path.is_err() {
        return GhAuthStatus::NotInstalled;
    }

    // Run `gh auth token` with stdout/stderr suppressed
    let result = tokio::task::spawn_blocking(|| {
        Command::new("gh")
            .args(["auth", "token"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
    })
    .await;

    match result {
        Ok(Ok(status)) if status.success() => GhAuthStatus::Authenticated,
        _ => GhAuthStatus::NotAuthenticated,
    }
}
