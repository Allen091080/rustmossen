use std::path::{Path, PathBuf};

use tokio::fs;
use tokio::process::Command;

/// Release channel for installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseChannel {
    Latest,
    Stable,
}

/// Get the local install directory.
pub fn get_local_install_dir(config_home: &Path) -> PathBuf {
    config_home.join("local")
}

/// Get the local mossen executable path.
pub fn get_local_mossen_path(config_home: &Path) -> PathBuf {
    get_local_install_dir(config_home).join("mossen")
}

/// Check if we're running from our managed local installation.
pub fn is_running_from_local_installation() -> bool {
    let exec_path = std::env::args().nth(1).unwrap_or_default();
    exec_path.contains("/.mossen/local/node_modules/")
}

/// Write content to path only if the file does not already exist.
/// Uses O_EXCL for atomic create-if-missing.
async fn write_if_missing(path: &Path, content: &str, mode: Option<u32>) -> std::io::Result<bool> {
    use tokio::io::AsyncWriteExt;

    // Check if file exists first
    if fs::metadata(path).await.is_ok() {
        return Ok(false);
    }

    // Try to create exclusively
    let mut file = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(e),
    };

    file.write_all(content.as_bytes()).await?;
    file.sync_all().await?;

    #[cfg(unix)]
    if let Some(m) = mode {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, std::fs::Permissions::from_mode(m)).await?;
    }

    Ok(true)
}

/// Ensure the local package environment is set up.
pub async fn ensure_local_package_environment(config_home: &Path) -> bool {
    let local_install_dir = get_local_install_dir(config_home);

    // Create installation directory
    if let Err(_) = fs::create_dir_all(&local_install_dir).await {
        return false;
    }

    // Create package.json if it doesn't exist
    let package_json = serde_json::json!({
        "name": "mossen-local",
        "version": "0.0.1",
        "private": true
    });
    if let Err(_) = write_if_missing(
        &local_install_dir.join("package.json"),
        &serde_json::to_string_pretty(&package_json).unwrap_or_default(),
        None,
    )
    .await
    {
        return false;
    }

    // Create the wrapper script if it doesn't exist
    let wrapper_path = local_install_dir.join("mossen");
    let wrapper_content = format!(
        "#!/bin/sh\nexec \"{}/node_modules/.bin/mossen\" \"$@\"",
        local_install_dir.to_string_lossy()
    );
    match write_if_missing(&wrapper_path, &wrapper_content, Some(0o755)).await {
        Ok(true) => {
            // Ensure executable bit
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&wrapper_path, std::fs::Permissions::from_mode(0o755))
                    .await;
            }
        }
        Ok(false) => {}
        Err(_) => return false,
    }

    true
}

/// Install result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallResult {
    InProgress,
    Success,
    InstallFailed,
}

/// Install or update Mossen CLI package in the local directory.
pub async fn install_or_update_mossen_package(
    config_home: &Path,
    channel: ReleaseChannel,
    specific_version: Option<&str>,
    package_url: &str,
) -> InstallResult {
    if !ensure_local_package_environment(config_home).await {
        return InstallResult::InstallFailed;
    }

    let version_spec = match specific_version {
        Some(v) => v.to_string(),
        None => match channel {
            ReleaseChannel::Stable => "stable".to_string(),
            ReleaseChannel::Latest => "latest".to_string(),
        },
    };

    let local_install_dir = get_local_install_dir(config_home);
    let result = Command::new("npm")
        .args(["install", &format!("{}@{}", package_url, version_spec)])
        .current_dir(&local_install_dir)
        .output()
        .await;

    match result {
        Ok(output) => {
            if !output.status.success() {
                let code = output.status.code().unwrap_or(-1);
                if code == 190 {
                    return InstallResult::InProgress;
                }
                return InstallResult::InstallFailed;
            }
            InstallResult::Success
        }
        Err(_) => InstallResult::InstallFailed,
    }
}

/// Check if local installation exists.
pub async fn local_installation_exists(config_home: &Path) -> bool {
    let bin_path = get_local_install_dir(config_home)
        .join("node_modules")
        .join(".bin")
        .join("mossen");
    fs::metadata(&bin_path).await.is_ok()
}

/// Get shell type to determine appropriate path setup.
pub fn get_shell_type() -> &'static str {
    let shell_path = std::env::var("SHELL").unwrap_or_default();
    if shell_path.contains("zsh") {
        "zsh"
    } else if shell_path.contains("bash") {
        "bash"
    } else if shell_path.contains("fish") {
        "fish"
    } else {
        "unknown"
    }
}
