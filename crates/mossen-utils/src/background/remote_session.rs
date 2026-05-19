//! Background remote session types and eligibility checks
//! Translated from utils/background/remote/remoteSession.ts

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

use super::preconditions::{
    check_github_app_installed, check_has_remote_environment,
    check_is_in_git_repo, check_needs_hosted_login,
};
use crate::todo::TodoList;

/// Background remote session type for managing teleport sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundRemoteSession {
    pub id: String,
    pub command: String,
    pub start_time: u64,
    pub status: RemoteSessionStatus,
    pub todo_list: TodoList,
    pub title: String,
    #[serde(rename = "type")]
    pub session_type: String, // always "remote_session"
    pub log: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteSessionStatus {
    Starting,
    Running,
    Completed,
    Failed,
    Killed,
}

/// Precondition failures for background remote sessions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundRemoteSessionPrecondition {
    NotLoggedIn,
    NoRemoteEnvironment,
    NotInGitRepo,
    NoGitRemote,
    GithubAppNotInstalled,
    PolicyBlocked,
}

/// Options for eligibility check
pub struct EligibilityCheckOptions {
    pub skip_bundle: bool,
}

impl Default for EligibilityCheckOptions {
    fn default() -> Self {
        Self { skip_bundle: false }
    }
}

/// Repository info detected from git remote
#[derive(Debug, Clone)]
pub struct DetectedRepository {
    pub host: String,
    pub owner: String,
    pub name: String,
}

/// Checks eligibility for creating a background remote session.
/// Returns an array of failed preconditions (empty array means all checks passed).
pub async fn check_background_remote_session_eligibility(
    options: &EligibilityCheckOptions,
    cwd: &Path,
    is_policy_allowed: bool,
    is_hosted_subscriber: bool,
    repository: Option<&DetectedRepository>,
    has_remote_env: bool,
    needs_login: bool,
    bundle_seed_gate_on: bool,
    access_token: Option<&str>,
    org_uuid: Option<&str>,
    base_api_url: &str,
) -> Vec<BackgroundRemoteSessionPrecondition> {
    let mut errors = Vec::new();

    // Check policy first - if blocked, no need to check other preconditions
    if !is_policy_allowed {
        errors.push(BackgroundRemoteSessionPrecondition::PolicyBlocked);
        return errors;
    }

    if needs_login {
        errors.push(BackgroundRemoteSessionPrecondition::NotLoggedIn);
    }

    if !has_remote_env {
        errors.push(BackgroundRemoteSessionPrecondition::NoRemoteEnvironment);
    }

    // When bundle seeding is on, in-git-repo is enough
    if !check_is_in_git_repo(cwd) {
        errors.push(BackgroundRemoteSessionPrecondition::NotInGitRepo);
    } else if bundle_seed_gate_on && !options.skip_bundle {
        // has .git/, bundle will work — skip remote+app checks
    } else if repository.is_none() {
        errors.push(BackgroundRemoteSessionPrecondition::NoGitRemote);
    } else if let Some(repo) = repository {
        if repo.host == "github.com" {
            let has_github_app = check_github_app_installed(
                &repo.owner,
                &repo.name,
                access_token,
                org_uuid,
                base_api_url,
            )
            .await;
            if !has_github_app {
                errors.push(BackgroundRemoteSessionPrecondition::GithubAppNotInstalled);
            }
        }
    }

    errors
}
