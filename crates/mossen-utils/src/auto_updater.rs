//! Auto-updater for Mossen CLI.
//!
//! Handles version checking, update installation, and release feed management.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use tokio::fs;
use tracing::{debug, error};

/// Official GCS bucket URL for release feed.
const OFFICIAL_GCS_BUCKET_URL: &str =
    "https://storage.googleapis.com/cli-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/cli-releases";

/// Lock file timeout (5 minutes).
const LOCK_TIMEOUT_MS: u64 = 5 * 60 * 1000;

/// Installation status for auto-updater.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallStatus {
    Success,
    NoPermissions,
    InstallFailed,
    InProgress,
}

/// Result of an auto-update attempt.
#[derive(Debug, Clone)]
pub struct AutoUpdaterResult {
    pub version: Option<String>,
    pub status: InstallStatus,
    pub notifications: Vec<String>,
}

/// Max version configuration from server.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct MaxVersionConfig {
    pub external: Option<String>,
    pub internal: Option<String>,
    pub external_message: Option<String>,
    pub ant_message: Option<String>,
}

/// Npm dist-tags.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct NpmDistTags {
    #[serde(default)]
    pub latest: Option<String>,
    #[serde(default)]
    pub stable: Option<String>,
}

/// Release channel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseChannel {
    Latest,
    Stable,
}

impl ReleaseChannel {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Latest => "latest",
            Self::Stable => "stable",
        }
    }
}

/// Get the release feed URL based on configuration.
pub fn get_release_feed_url() -> String {
    if let Ok(url) = std::env::var("MOSSEN_CODE_PLATFORM_RELEASE_FEED_URL") {
        let url = url.trim().to_string();
        if !url.is_empty() {
            return url;
        }
    }
    OFFICIAL_GCS_BUCKET_URL.to_string()
}

/// Get the installer documentation URL.
pub fn get_installer_docs_url() -> String {
    if let Ok(url) = std::env::var("MOSSEN_CODE_PLATFORM_INSTALLER_DOCS_URL") {
        let url = url.trim().to_string();
        if !url.is_empty() {
            return url;
        }
    }
    String::new()
}

/// Get the installer docs line for display.
fn get_installer_docs_line() -> String {
    let docs_url = get_installer_docs_url();
    if docs_url.is_empty() {
        "Use your platform installer or package-manager documentation for this build.".to_string()
    } else {
        format!("See {} for installer and upgrade options.", docs_url)
    }
}

/// Get update required message.
pub fn get_update_required_message(
    required_version: &str,
    current_version: &str,
    product_display_name: &str,
    product_cli_name: &str,
) -> String {
    format!(
        "\nIt looks like your version of {} ({}) needs an update.\n\
         A newer version ({} or higher) is required to continue.\n\n\
         To update, please run:\n    {} update\n\n\
         This will ensure you have access to the latest features and improvements.\n",
        product_display_name, current_version, required_version, product_cli_name
    )
}

/// Get unsupported WSL update message.
pub fn get_unsupported_wsl_update_message(
    product_display_name: &str,
    product_cli_name: &str,
) -> String {
    format!(
        "\nError: Windows NPM detected in WSL\n\n\
         You're running {} in WSL but using the Windows NPM installation from /mnt/c/.\n\
         This configuration is not supported for updates.\n\n\
         To fix this issue:\n  \
         1. Install Node.js within your Linux distribution: e.g. sudo apt install nodejs npm\n  \
         2. Make sure Linux NPM is in your PATH before the Windows version\n  \
         3. Try updating again with '{} update'\n",
        product_display_name, product_cli_name
    )
}

/// Check if target version should be skipped based on minimum version setting.
pub fn should_skip_version(target_version: &str, minimum_version: Option<&str>) -> bool {
    match minimum_version {
        Some(min) => {
            let should_skip = !version_gte(target_version, min);
            if should_skip {
                debug!(
                    "Skipping update to {} - below minimumVersion {}",
                    target_version, min
                );
            }
            should_skip
        }
        None => false,
    }
}

/// Get the path to the update lock file.
pub fn get_lock_file_path(config_home: &Path) -> PathBuf {
    config_home.join(".update.lock")
}

/// Attempt to acquire the update lock.
///
/// Returns true if lock was acquired, false if another process holds it.
pub async fn acquire_lock(config_home: &Path) -> Result<bool> {
    let lock_path = get_lock_file_path(config_home);

    // Check for existing lock
    match fs::metadata(&lock_path).await {
        Ok(metadata) => {
            if let Ok(modified) = metadata.modified() {
                let age = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or(Duration::ZERO);
                if age.as_millis() < LOCK_TIMEOUT_MS as u128 {
                    return Ok(false);
                }
                // Lock is stale - re-verify before removing
                match fs::metadata(&lock_path).await {
                    Ok(recheck_meta) => {
                        if let Ok(recheck_modified) = recheck_meta.modified() {
                            let recheck_age = std::time::SystemTime::now()
                                .duration_since(recheck_modified)
                                .unwrap_or(Duration::ZERO);
                            if recheck_age.as_millis() < LOCK_TIMEOUT_MS as u128 {
                                return Ok(false);
                            }
                        }
                        fs::remove_file(&lock_path).await.ok();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Ok(false),
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // No lock file, proceed
        }
        Err(_) => return Ok(false),
    }

    // Create lock file atomically
    let pid = std::process::id();
    match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .await
    {
        Ok(_file) => {
            fs::write(&lock_path, pid.to_string()).await?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Directory doesn't exist, create it
            if let Some(parent) = lock_path.parent() {
                fs::create_dir_all(parent).await.ok();
            }
            match tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
                .await
            {
                Ok(_file) => {
                    fs::write(&lock_path, pid.to_string()).await?;
                    Ok(true)
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
                Err(e) => Err(e.into()),
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Release the update lock if held by this process.
pub async fn release_lock(config_home: &Path) {
    let lock_path = get_lock_file_path(config_home);
    let pid = std::process::id().to_string();

    match fs::read_to_string(&lock_path).await {
        Ok(content) if content == pid => {
            fs::remove_file(&lock_path).await.ok();
        }
        Ok(_) => {} // Different PID holds the lock
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            error!("Failed to read lock file: {}", e);
        }
    }
}

/// Check global install permissions.
pub async fn check_global_install_permissions() -> (bool, Option<String>) {
    let is_bun = std::env::var("BUN_INSTALL").is_ok();
    let (cmd, args) = if is_bun {
        ("bun", vec!["pm", "bin", "-g"])
    } else {
        ("npm", vec!["-g", "config", "get", "prefix"])
    };

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    let output = tokio::process::Command::new(cmd)
        .args(&args)
        .current_dir(&home)
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Check write access
            let path = Path::new(&prefix);
            let has_permissions = path.exists()
                && std::fs::metadata(path)
                    .map(|m| !m.permissions().readonly())
                    .unwrap_or(false);
            (has_permissions, Some(prefix))
        }
        _ => (false, None),
    }
}

/// Get the latest version from the release feed for a given channel.
pub async fn get_latest_version_from_gcs(channel: ReleaseChannel) -> Option<String> {
    let release_feed_url = get_release_feed_url();
    if release_feed_url.is_empty() {
        return None;
    }

    let url = format!("{}/{}", release_feed_url, channel.as_str());
    let client = reqwest::Client::new();

    match client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(text) => {
                let version = text.trim().to_string();
                if version.is_empty() {
                    None
                } else {
                    Some(version)
                }
            }
            Err(e) => {
                debug!("Failed to read release feed response: {}", e);
                None
            }
        },
        Err(e) => {
            debug!(
                "Failed to fetch {} from release feed: {}",
                channel.as_str(),
                e
            );
            None
        }
    }
}

/// Get available versions from GCS bucket (latest + stable).
pub async fn get_gcs_dist_tags() -> NpmDistTags {
    let (latest, stable) = tokio::join!(
        get_latest_version_from_gcs(ReleaseChannel::Latest),
        get_latest_version_from_gcs(ReleaseChannel::Stable),
    );
    NpmDistTags { latest, stable }
}

/// 对应 TS `assertMinVersion`：检查当前进程版本是否满足最低限制。
///
/// 通过 npm dist-tags 中 `minimum_version` 读取。当前实现读取环境变量
/// `MOSSEN_MIN_VERSION` 作为信源；不满足时返回错误。
pub async fn assert_min_version(current_version: &str) -> anyhow::Result<()> {
    if let Ok(min) = std::env::var("MOSSEN_MIN_VERSION") {
        if !version_gte(current_version, &min) {
            anyhow::bail!(
                "Current version {} is below the minimum required version {}",
                current_version,
                min,
            );
        }
    }
    Ok(())
}

/// 对应 TS `getMaxVersion`：返回 npm dist-tags 中的最大版本。
pub async fn get_max_version() -> Option<String> {
    get_npm_dist_tags().await.latest
}

/// 对应 TS `getMaxVersionMessage`：返回 max-version 提示文案。
pub async fn get_max_version_message() -> Option<String> {
    let max = get_max_version().await?;
    Some(format!(
        "A newer version ({}) is available. Run `npm i -g @internal/cli` to upgrade.",
        max
    ))
}

/// 对应 TS `getNpmDistTags`：调用 `npm view ... dist-tags --json`。
pub async fn get_npm_dist_tags() -> NpmDistTags {
    let output = tokio::process::Command::new("npm")
        .args(["view", "@internal/cli", "dist-tags", "--json"])
        .output()
        .await;
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            serde_json::from_str::<NpmDistTags>(&stdout).unwrap_or_default()
        }
        _ => NpmDistTags::default(),
    }
}

/// 对应 TS `getVersionHistory`：从 `npm view` 拉取版本列表，返回最近 `limit` 个。
pub async fn get_version_history(limit: usize) -> Vec<String> {
    let output = tokio::process::Command::new("npm")
        .args(["view", "@internal/cli", "versions", "--json"])
        .output()
        .await;
    let stdout = match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        _ => return Vec::new(),
    };
    let mut versions: Vec<String> = serde_json::from_str(&stdout).unwrap_or_default();
    versions.reverse();
    versions.truncate(limit);
    versions
}

/// 对应 TS `installGlobalPackage`：通过 `npm install -g` 安装指定版本。
pub async fn install_global_package(version: &str) -> anyhow::Result<()> {
    let spec = format!("@internal/cli@{version}");
    let output = tokio::process::Command::new("npm")
        .args(["install", "-g", &spec])
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(
            "npm install -g {} failed: {}",
            spec,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Simple semver "greater than or equal" comparison.
/// Compares major.minor.patch numerically, ignoring build metadata.
fn version_gte(version: &str, minimum: &str) -> bool {
    let parse = |v: &str| -> (u64, u64, u64) {
        let v = v.split('+').next().unwrap_or(v); // strip build metadata
        let parts: Vec<u64> = v
            .split('.')
            .take(3)
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };

    let v = parse(version);
    let m = parse(minimum);
    v >= m
}

/// Simple semver "less than" comparison.
pub fn version_lt(version: &str, other: &str) -> bool {
    !version_gte(version, other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_gte() {
        assert!(version_gte("2.0.0", "1.0.0"));
        assert!(version_gte("1.1.0", "1.0.0"));
        assert!(version_gte("1.0.1", "1.0.0"));
        assert!(version_gte("1.0.0", "1.0.0"));
        assert!(!version_gte("0.9.0", "1.0.0"));
    }

    #[test]
    fn test_version_gte_with_build_metadata() {
        assert!(version_gte("2.0.0+abc123", "1.0.0"));
        assert!(version_gte("1.0.0+abc123", "1.0.0+def456"));
    }

    #[test]
    fn test_should_skip_version() {
        assert!(!should_skip_version("2.0.0", Some("1.0.0")));
        assert!(should_skip_version("0.9.0", Some("1.0.0")));
        assert!(!should_skip_version("1.0.0", None));
    }

    #[test]
    fn test_release_channel() {
        assert_eq!(ReleaseChannel::Latest.as_str(), "latest");
        assert_eq!(ReleaseChannel::Stable.as_str(), "stable");
    }
}
