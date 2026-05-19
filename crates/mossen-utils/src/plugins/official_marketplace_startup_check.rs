use std::path::{Path, PathBuf};

use tracing::debug;

/// Reason why the official marketplace was not installed.
#[derive(Debug, Clone, PartialEq)]
pub enum OfficialMarketplaceSkipReason {
    AlreadyAttempted,
    AlreadyInstalled,
    CustomBackendDisabled,
    PolicyBlocked,
    GitUnavailable,
    GcsUnavailable,
    Unknown,
}

/// Configuration for retry logic.
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub backoff_multiplier: u32,
    pub max_delay_ms: u64,
}

pub const RETRY_CONFIG: RetryConfig = RetryConfig {
    max_attempts: 10,
    initial_delay_ms: 60 * 60 * 1000,      // 1 hour
    backoff_multiplier: 2,
    max_delay_ms: 7 * 24 * 60 * 60 * 1000, // 1 week
};

/// Result of the auto-install check.
#[derive(Debug, Clone)]
pub struct OfficialMarketplaceCheckResult {
    pub installed: bool,
    pub skipped: bool,
    pub reason: Option<OfficialMarketplaceSkipReason>,
    pub config_save_failed: Option<bool>,
}

/// Trait for global config access.
pub trait GlobalConfigProvider: Send + Sync {
    fn get_auto_install_attempted(&self) -> bool;
    fn get_auto_installed(&self) -> bool;
    fn get_fail_reason(&self) -> Option<String>;
    fn get_retry_count(&self) -> u32;
    fn get_next_retry_time(&self) -> Option<u64>;
    fn save_success(&self);
    fn save_failure(&self, reason: &str, retry_count: u32, next_retry_time: u64);
    fn save_policy_blocked(&self);
}

/// Trait for environment checks.
pub trait EnvironmentChecker: Send + Sync {
    fn is_auto_install_disabled_env(&self) -> bool;
    fn is_auto_install_explicitly_enabled(&self) -> bool;
    fn is_custom_backend_enabled(&self) -> bool;
    fn is_source_allowed_by_policy(&self, source: &serde_json::Value) -> bool;
    fn get_feature_value_git_fallback(&self) -> bool;
}

/// Trait for marketplace operations.
#[async_trait::async_trait]
pub trait MarketplaceOps: Send + Sync {
    async fn load_known_marketplaces_config(&self) -> std::collections::HashMap<String, serde_json::Value>;
    async fn save_known_marketplaces_config(&self, config: &std::collections::HashMap<String, serde_json::Value>) -> Result<(), anyhow::Error>;
    async fn add_marketplace_source(&self, source: &serde_json::Value) -> Result<(), anyhow::Error>;
    fn get_marketplaces_cache_dir(&self) -> PathBuf;
    fn get_official_marketplace_name(&self) -> &str;
    fn get_official_marketplace_source(&self) -> serde_json::Value;
}

/// Trait for GCS fetch operations.
#[async_trait::async_trait]
pub trait GcsFetcher: Send + Sync {
    async fn fetch_official_marketplace_from_gcs(
        &self,
        install_location: &Path,
        cache_dir: &Path,
    ) -> Option<String>;
}

/// Trait for git availability check.
#[async_trait::async_trait]
pub trait GitChecker: Send + Sync {
    async fn check_git_available(&self) -> bool;
    fn mark_git_unavailable(&self);
}

/// Calculate next retry delay using exponential backoff.
fn calculate_next_retry_delay(retry_count: u32) -> u64 {
    let delay = RETRY_CONFIG.initial_delay_ms
        * (RETRY_CONFIG.backoff_multiplier as u64).pow(retry_count);
    delay.min(RETRY_CONFIG.max_delay_ms)
}

/// Determine if installation should be retried.
fn should_retry_installation(
    attempted: bool,
    installed: bool,
    fail_reason: Option<&str>,
    retry_count: u32,
    next_retry_time: Option<u64>,
    now: u64,
) -> bool {
    if !attempted {
        return true;
    }
    if installed {
        return false;
    }
    if retry_count >= RETRY_CONFIG.max_attempts {
        return false;
    }
    if fail_reason == Some("policy_blocked") {
        return false;
    }
    if let Some(nrt) = next_retry_time {
        if now < nrt {
            return false;
        }
    }
    matches!(
        fail_reason,
        Some("unknown") | Some("git_unavailable") | Some("gcs_unavailable") | None
    )
}

/// Check if official marketplace auto-install is disabled.
pub fn is_official_marketplace_auto_install_disabled(env: &dyn EnvironmentChecker) -> bool {
    if env.is_auto_install_disabled_env() {
        return true;
    }
    env.is_custom_backend_enabled() && !env.is_auto_install_explicitly_enabled()
}

/// Check and install the official marketplace on startup.
pub async fn check_and_install_official_marketplace(
    config: &dyn GlobalConfigProvider,
    env: &dyn EnvironmentChecker,
    marketplace: &dyn MarketplaceOps,
    gcs: &dyn GcsFetcher,
    git: &dyn GitChecker,
) -> OfficialMarketplaceCheckResult {
    // Check if disabled for custom backend
    if env.is_custom_backend_enabled() && !env.is_auto_install_explicitly_enabled() {
        debug!("Official marketplace auto-install disabled for custom backend, skipping");
        return OfficialMarketplaceCheckResult {
            installed: false,
            skipped: true,
            reason: Some(OfficialMarketplaceSkipReason::CustomBackendDisabled),
            config_save_failed: None,
        };
    }

    let now = current_time_ms();

    // Check retry logic
    if !should_retry_installation(
        config.get_auto_install_attempted(),
        config.get_auto_installed(),
        config.get_fail_reason().as_deref(),
        config.get_retry_count(),
        config.get_next_retry_time(),
        now,
    ) {
        let reason = match config.get_fail_reason().as_deref() {
            Some("policy_blocked") => OfficialMarketplaceSkipReason::PolicyBlocked,
            Some("git_unavailable") => OfficialMarketplaceSkipReason::GitUnavailable,
            Some("gcs_unavailable") => OfficialMarketplaceSkipReason::GcsUnavailable,
            _ => OfficialMarketplaceSkipReason::AlreadyAttempted,
        };
        debug!("Official marketplace auto-install skipped: {:?}", reason);
        return OfficialMarketplaceCheckResult {
            installed: false,
            skipped: true,
            reason: Some(reason),
            config_save_failed: None,
        };
    }

    // Check env var disable
    if env.is_auto_install_disabled_env() {
        debug!("Official marketplace auto-install disabled via env var, skipping");
        config.save_policy_blocked();
        return OfficialMarketplaceCheckResult {
            installed: false,
            skipped: true,
            reason: Some(OfficialMarketplaceSkipReason::PolicyBlocked),
            config_save_failed: None,
        };
    }

    // Check if already installed
    let known = marketplace.load_known_marketplaces_config().await;
    let official_name = marketplace.get_official_marketplace_name();
    if known.contains_key(official_name) {
        debug!("Official marketplace '{}' already installed, skipping", official_name);
        config.save_success();
        return OfficialMarketplaceCheckResult {
            installed: false,
            skipped: true,
            reason: Some(OfficialMarketplaceSkipReason::AlreadyInstalled),
            config_save_failed: None,
        };
    }

    // Check enterprise policy
    let source = marketplace.get_official_marketplace_source();
    if !env.is_source_allowed_by_policy(&source) {
        debug!("Official marketplace blocked by enterprise policy, skipping");
        config.save_policy_blocked();
        return OfficialMarketplaceCheckResult {
            installed: false,
            skipped: true,
            reason: Some(OfficialMarketplaceSkipReason::PolicyBlocked),
            config_save_failed: None,
        };
    }

    // Try GCS mirror first
    let cache_dir = marketplace.get_marketplaces_cache_dir();
    let install_location = cache_dir.join(official_name);
    let gcs_sha = gcs
        .fetch_official_marketplace_from_gcs(&install_location, &cache_dir)
        .await;

    if let Some(_sha) = gcs_sha {
        // GCS succeeded — register marketplace
        let mut known = marketplace.load_known_marketplaces_config().await;
        known.insert(
            official_name.to_string(),
            serde_json::json!({
                "source": source,
                "installLocation": install_location.to_string_lossy(),
                "lastUpdated": chrono::Utc::now().to_rfc3339(),
            }),
        );
        let _ = marketplace.save_known_marketplaces_config(&known).await;
        config.save_success();
        return OfficialMarketplaceCheckResult {
            installed: true,
            skipped: false,
            reason: None,
            config_save_failed: None,
        };
    }

    // GCS failed — check git fallback flag
    if !env.get_feature_value_git_fallback() {
        debug!("Official marketplace GCS failed; git fallback disabled by flag — skipping install");
        let retry_count = config.get_retry_count() + 1;
        let next_retry = now + calculate_next_retry_delay(retry_count);
        config.save_failure("gcs_unavailable", retry_count, next_retry);
        return OfficialMarketplaceCheckResult {
            installed: false,
            skipped: true,
            reason: Some(OfficialMarketplaceSkipReason::GcsUnavailable),
            config_save_failed: None,
        };
    }

    // Check git availability
    let git_available = git.check_git_available().await;
    if !git_available {
        debug!("Git not available, skipping official marketplace auto-install");
        let retry_count = config.get_retry_count() + 1;
        let next_retry = now + calculate_next_retry_delay(retry_count);
        config.save_failure("git_unavailable", retry_count, next_retry);
        return OfficialMarketplaceCheckResult {
            installed: false,
            skipped: true,
            reason: Some(OfficialMarketplaceSkipReason::GitUnavailable),
            config_save_failed: None,
        };
    }

    // Attempt installation
    debug!("Attempting to auto-install official marketplace");
    match marketplace.add_marketplace_source(&source).await {
        Ok(()) => {
            debug!("Successfully auto-installed official marketplace");
            config.save_success();
            OfficialMarketplaceCheckResult {
                installed: true,
                skipped: false,
                reason: None,
                config_save_failed: None,
            }
        }
        Err(e) => {
            let error_msg = e.to_string();

            // macOS xcrun shim detection
            if error_msg.contains("xcrun: error:") {
                git.mark_git_unavailable();
                debug!("Official marketplace auto-install: git is a non-functional macOS xcrun shim");
                return OfficialMarketplaceCheckResult {
                    installed: false,
                    skipped: true,
                    reason: Some(OfficialMarketplaceSkipReason::GitUnavailable),
                    config_save_failed: None,
                };
            }

            debug!("Failed to auto-install official marketplace: {}", error_msg);
            let retry_count = config.get_retry_count() + 1;
            let next_retry = now + calculate_next_retry_delay(retry_count);
            config.save_failure("unknown", retry_count, next_retry);
            OfficialMarketplaceCheckResult {
                installed: false,
                skipped: true,
                reason: Some(OfficialMarketplaceSkipReason::Unknown),
                config_save_failed: None,
            }
        }
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
