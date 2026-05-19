//! User data utilities for analytics and identity.
//!
//! Provides core user data (device ID, session, email, platform) used as
//! base for all analytics providers including GrowthBook.

use std::env;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tokio::process::Command;

/// GitHub Actions metadata when running in CI.
#[derive(Debug, Clone, Default)]
pub struct GitHubActionsMetadata {
    pub actor: Option<String>,
    pub actor_id: Option<String>,
    pub repository: Option<String>,
    pub repository_id: Option<String>,
    pub repository_owner: Option<String>,
    pub repository_owner_id: Option<String>,
}

/// Core user data used as base for all analytics providers.
#[derive(Debug, Clone)]
pub struct CoreUserData {
    pub device_id: String,
    pub session_id: String,
    pub email: Option<String>,
    pub app_version: String,
    pub platform: String,
    pub organization_uuid: Option<String>,
    pub account_uuid: Option<String>,
    pub user_type: Option<String>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub first_token_time: Option<i64>,
    pub github_actions_metadata: Option<GitHubActionsMetadata>,
}

/// Module-level state for cached email.
static CACHED_EMAIL: Lazy<Mutex<Option<Option<String>>>> = Lazy::new(|| Mutex::new(None));

/// Initialize user data asynchronously. Should be called early in startup.
/// This pre-fetches the email so get_core_user_data() can remain synchronous.
pub async fn init_user() {
    // Need to ensure no MutexGuard crosses the await; use a tight scope.
    let needs_init = {
        let cached = CACHED_EMAIL.lock().unwrap();
        cached.is_none()
    };
    if needs_init {
        let email = get_email_async().await;
        let mut cached = CACHED_EMAIL.lock().unwrap();
        *cached = Some(email);
    }
}

/// Reset all user data caches. Call on auth changes (login/logout/account switch).
pub fn reset_user_cache() {
    let mut cached = CACHED_EMAIL.lock().unwrap();
    *cached = None;
}

/// Get core user data.
/// This is the base representation that gets transformed for different analytics providers.
pub fn get_core_user_data(
    device_id: &str,
    session_id: &str,
    app_version: &str,
    platform: &str,
    include_analytics_metadata: bool,
    oauth_account: Option<&OAuthAccountInfo>,
    subscription_type_fn: impl FnOnce() -> Option<String>,
    rate_limit_tier_fn: impl FnOnce() -> Option<String>,
    first_token_date: Option<&str>,
) -> CoreUserData {
    let mut subscription_type = None;
    let mut rate_limit_tier = None;
    let mut first_token_time = None;

    if include_analytics_metadata {
        subscription_type = subscription_type_fn();
        rate_limit_tier = rate_limit_tier_fn();
        if subscription_type.is_some() {
            if let Some(date_str) = first_token_date {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
                    first_token_time = Some(dt.timestamp_millis());
                }
            }
        }
    }

    let organization_uuid = oauth_account.and_then(|a| a.organization_uuid.clone());
    let account_uuid = oauth_account.and_then(|a| a.account_uuid.clone());

    let email = get_email(oauth_account);

    let github_actions_metadata = if is_env_truthy(env::var("GITHUB_ACTIONS").ok().as_deref()) {
        Some(GitHubActionsMetadata {
            actor: env::var("GITHUB_ACTOR").ok(),
            actor_id: env::var("GITHUB_ACTOR_ID").ok(),
            repository: env::var("GITHUB_REPOSITORY").ok(),
            repository_id: env::var("GITHUB_REPOSITORY_ID").ok(),
            repository_owner: env::var("GITHUB_REPOSITORY_OWNER").ok(),
            repository_owner_id: env::var("GITHUB_REPOSITORY_OWNER_ID").ok(),
        })
    } else {
        None
    };

    CoreUserData {
        device_id: device_id.to_string(),
        session_id: session_id.to_string(),
        email,
        app_version: app_version.to_string(),
        platform: platform.to_string(),
        organization_uuid,
        account_uuid,
        user_type: env::var("USER_TYPE").ok(),
        subscription_type,
        rate_limit_tier,
        first_token_time,
        github_actions_metadata,
    }
}

/// Get user data for GrowthBook (same as core data with analytics metadata).
pub fn get_user_for_growth_book(
    device_id: &str,
    session_id: &str,
    app_version: &str,
    platform: &str,
    oauth_account: Option<&OAuthAccountInfo>,
    subscription_type_fn: impl FnOnce() -> Option<String>,
    rate_limit_tier_fn: impl FnOnce() -> Option<String>,
    first_token_date: Option<&str>,
) -> CoreUserData {
    get_core_user_data(
        device_id,
        session_id,
        app_version,
        platform,
        true,
        oauth_account,
        subscription_type_fn,
        rate_limit_tier_fn,
        first_token_date,
    )
}

/// OAuth account info subset needed for user data.
#[derive(Debug, Clone)]
pub struct OAuthAccountInfo {
    pub email_address: Option<String>,
    pub organization_uuid: Option<String>,
    pub account_uuid: Option<String>,
}

fn get_email(oauth_account: Option<&OAuthAccountInfo>) -> Option<String> {
    // Return cached email if available
    let cached = CACHED_EMAIL.lock().unwrap();
    if let Some(ref email) = *cached {
        return email.clone();
    }
    drop(cached);

    // OAuth email
    if let Some(account) = oauth_account {
        if let Some(ref email) = account.email_address {
            return Some(email.clone());
        }
    }

    // Ant-only fallbacks
    if env::var("USER_TYPE").ok().as_deref() != Some("ant") {
        return None;
    }

    if let Ok(creator) = env::var("COO_CREATOR") {
        return Some(format!("{}@mossen.invalid", creator));
    }

    None
}

async fn get_email_async() -> Option<String> {
    // OAuth email
    // Note: in a real implementation, this would call get_oauth_account_info()
    // For now we check env fallbacks

    if env::var("USER_TYPE").ok().as_deref() != Some("ant") {
        return None;
    }

    if let Ok(creator) = env::var("COO_CREATOR") {
        return Some(format!("{}@mossen.invalid", creator));
    }

    get_git_email().await
}

/// Get the user's git email from `git config user.email`.
pub async fn get_git_email() -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", "user.email"])
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        }
    } else {
        None
    }
}

/// Helper: check if an env-style value is truthy.
fn is_env_truthy(val: Option<&str>) -> bool {
    match val {
        None => false,
        Some(v) => {
            let normalized = v.to_lowercase().trim().to_string();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
    }
}
