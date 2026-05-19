//! Desktop deep link utilities.
//!
//! Builds and opens deep link URLs for Mossen Desktop to resume CLI sessions.

use std::path::PathBuf;
use anyhow::{Result, anyhow};

const MIN_DESKTOP_VERSION: &str = "1.1.2396";

/// Check if running in dev mode.
fn is_dev_mode() -> bool {
    if std::env::var("NODE_ENV").ok().as_deref() == Some("development") {
        return true;
    }

    let paths_to_check: Vec<String> = vec![
        std::env::args().nth(1).unwrap_or_default(),
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    ];
    let build_dirs = [
        "/build-ant/",
        "/build-ant-native/",
        "/build-external/",
        "/build-external-native/",
    ];

    paths_to_check
        .iter()
        .any(|p| build_dirs.iter().any(|dir| p.contains(dir)))
}

/// Builds a deep link URL for Mossen Desktop to resume a CLI session.
/// Format: mossen://resume?session={sessionId}&cwd={cwd}
/// In dev mode: mossen-dev://resume?session={sessionId}&cwd={cwd}
fn build_desktop_deep_link(session_id: &str, cwd: &str) -> String {
    let protocol = if is_dev_mode() {
        "mossen-dev"
    } else {
        "mossen"
    };
    let mut url = url::Url::parse(&format!("{protocol}://resume"))
        .expect("static URL parse should not fail");
    url.query_pairs_mut()
        .append_pair("session", session_id)
        .append_pair("cwd", cwd);
    url.to_string()
}

/// Status of the Mossen Desktop installation.
#[derive(Debug, Clone)]
pub enum DesktopInstallStatus {
    NotInstalled,
    VersionTooOld { version: String },
    Ready { version: String },
}

/// Check if Mossen Desktop app is installed.
/// On macOS, checks for /Applications/Mossen.app.
/// On Linux, checks if xdg-open can handle mossen:// protocol.
/// On Windows, checks if the protocol handler exists.
/// In dev mode, always returns true.
pub async fn is_desktop_installed() -> bool {
    if is_dev_mode() {
        return true;
    }

    #[cfg(target_os = "macos")]
    {
        tokio::fs::metadata("/Applications/Mossen.app")
            .await
            .is_ok()
    }

    #[cfg(target_os = "linux")]
    {
        match tokio::process::Command::new("xdg-mime")
            .args(["query", "default", "x-scheme-handler/mossen"])
            .output()
            .await
        {
            Ok(output) => output.status.success() && !output.stdout.is_empty(),
            Err(_) => false,
        }
    }

    #[cfg(target_os = "windows")]
    {
        match tokio::process::Command::new("reg")
            .args(["query", r"HKEY_CLASSES_ROOT\mossen", "/ve"])
            .output()
            .await
        {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

/// Detect the installed Mossen Desktop version.
/// On macOS, reads CFBundleShortVersionString from the app plist.
/// On Windows, finds the highest app-X.Y.Z directory in the Squirrel install.
/// Returns None if version cannot be determined.
pub async fn get_desktop_version() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = tokio::process::Command::new("defaults")
            .args([
                "read",
                "/Applications/Mossen.app/Contents/Info.plist",
                "CFBundleShortVersionString",
            ])
            .output()
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if version.is_empty() {
            None
        } else {
            Some(version)
        }
    }

    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
        let install_dir = PathBuf::from(&local_app_data).join("Mossen");
        let mut entries = tokio::fs::read_dir(&install_dir).await.ok()?;
        let mut versions: Vec<String> = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(ver) = name.strip_prefix("app-") {
                if semver_coerce(ver).is_some() {
                    versions.push(ver.to_string());
                }
            }
        }
        versions.sort_by(|a, b| {
            let ca = semver_coerce(a);
            let cb = semver_coerce(b);
            ca.cmp(&cb)
        });
        versions.last().cloned()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Attempt to coerce a string into a semver-like (major, minor, patch) tuple.
fn semver_coerce(v: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() < 3 {
        return None;
    }
    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts[1].parse::<u64>().ok()?;
    let patch = parts[2].parse::<u64>().ok()?;
    Some((major, minor, patch))
}

/// Compare two semver strings: returns true if `version >= min_version`.
fn semver_gte(version: &str, min_version: &str) -> bool {
    match (semver_coerce(version), semver_coerce(min_version)) {
        (Some(v), Some(m)) => v >= m,
        _ => false,
    }
}

/// Check Desktop install status including version compatibility.
pub async fn get_desktop_install_status() -> DesktopInstallStatus {
    let installed = is_desktop_installed().await;
    if !installed {
        return DesktopInstallStatus::NotInstalled;
    }

    let version = match get_desktop_version().await {
        Some(v) => v,
        None => return DesktopInstallStatus::Ready { version: "unknown".to_string() },
    };

    if !semver_gte(&version, MIN_DESKTOP_VERSION) {
        return DesktopInstallStatus::VersionTooOld { version };
    }

    DesktopInstallStatus::Ready { version }
}

/// Opens a deep link URL using the platform-specific mechanism.
/// Returns true if the command succeeded, false otherwise.
async fn open_deep_link(deep_link_url: &str) -> bool {
    tracing::debug!("Opening deep link: {}", deep_link_url);

    #[cfg(target_os = "macos")]
    {
        if is_dev_mode() {
            let result = tokio::process::Command::new("osascript")
                .args([
                    "-e",
                    &format!(
                        "tell application \"Electron\" to open location \"{}\"",
                        deep_link_url
                    ),
                ])
                .output()
                .await;
            return result.map(|o| o.status.success()).unwrap_or(false);
        }
        let result = tokio::process::Command::new("open")
            .arg(deep_link_url)
            .output()
            .await;
        return result.map(|o| o.status.success()).unwrap_or(false);
    }

    #[cfg(target_os = "linux")]
    {
        let result = tokio::process::Command::new("xdg-open")
            .arg(deep_link_url)
            .output()
            .await;
        return result.map(|o| o.status.success()).unwrap_or(false);
    }

    #[cfg(target_os = "windows")]
    {
        let result = tokio::process::Command::new("cmd")
            .args(["/c", "start", "", deep_link_url])
            .output()
            .await;
        return result.map(|o| o.status.success()).unwrap_or(false);
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = deep_link_url;
        false
    }
}

/// Result from opening a session in Desktop.
#[derive(Debug)]
pub struct OpenDesktopResult {
    pub success: bool,
    pub error: Option<String>,
    pub deep_link_url: Option<String>,
}

/// Build and open a deep link to resume the current session in Mossen Desktop.
pub async fn open_current_session_in_desktop(
    session_id: &str,
    cwd: &str,
) -> OpenDesktopResult {
    let installed = is_desktop_installed().await;
    if !installed {
        return OpenDesktopResult {
            success: false,
            error: Some(
                "The desktop companion app is not installed. Install it from the platform desktop downloads page.".to_string(),
            ),
            deep_link_url: None,
        };
    }

    let deep_link_url = build_desktop_deep_link(session_id, cwd);
    let opened = open_deep_link(&deep_link_url).await;

    if !opened {
        return OpenDesktopResult {
            success: false,
            error: Some(
                "Failed to open the desktop companion app. Please try opening it manually."
                    .to_string(),
            ),
            deep_link_url: Some(deep_link_url),
        };
    }

    OpenDesktopResult {
        success: true,
        error: None,
        deep_link_url: Some(deep_link_url),
    }
}
