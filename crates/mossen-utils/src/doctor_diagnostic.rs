use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Installation type for the Mossen binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallationType {
    NpmGlobal,
    NpmLocal,
    Native,
    PackageManager,
    Development,
    Unknown,
}

impl std::fmt::Display for InstallationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallationType::NpmGlobal => write!(f, "npm-global"),
            InstallationType::NpmLocal => write!(f, "npm-local"),
            InstallationType::Native => write!(f, "native"),
            InstallationType::PackageManager => write!(f, "package-manager"),
            InstallationType::Development => write!(f, "development"),
            InstallationType::Unknown => write!(f, "unknown"),
        }
    }
}

/// Diagnostic information about the installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    pub installation_type: InstallationType,
    pub version: String,
    pub installation_path: String,
    pub invoked_binary: String,
    pub config_install_method: String,
    pub auto_updates: String,
    pub has_update_permissions: Option<bool>,
    pub multiple_installations: Vec<InstallationEntry>,
    pub warnings: Vec<DiagnosticWarning>,
    pub recommendation: Option<String>,
    pub package_manager: Option<String>,
    pub ripgrep_status: RipgrepStatus,
}

/// An installation entry (detected installation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationEntry {
    pub install_type: String,
    pub path: String,
}

/// A diagnostic warning with issue and fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticWarning {
    pub issue: String,
    pub fix: String,
}

/// Ripgrep status information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepStatus {
    pub working: bool,
    pub mode: String,
    pub system_path: Option<String>,
}

/// Get the current installation type.
pub async fn get_current_installation_type() -> InstallationType {
    if std::env::var("NODE_ENV").ok().as_deref() == Some("development") {
        return InstallationType::Development;
    }

    let invoked_path = std::env::args().nth(1).unwrap_or_default();

    // Check if running in bundled mode
    if is_in_bundled_mode() {
        if detect_homebrew() || detect_package_manager().await {
            return InstallationType::PackageManager;
        }
        return InstallationType::Native;
    }

    // Check npm global paths
    let npm_global_paths = [
        "/usr/local/lib/node_modules",
        "/usr/lib/node_modules",
        "/opt/homebrew/lib/node_modules",
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/.nvm/versions/node/",
    ];

    if npm_global_paths
        .iter()
        .any(|p| invoked_path.contains(p))
    {
        return InstallationType::NpmGlobal;
    }

    if invoked_path.contains("/npm/") || invoked_path.contains("/nvm/") {
        return InstallationType::NpmGlobal;
    }

    // Check npm prefix
    if let Ok(output) = tokio::process::Command::new("npm")
        .args(["config", "get", "prefix"])
        .output()
        .await
    {
        if output.status.success() {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !prefix.is_empty() && invoked_path.starts_with(&prefix) {
                return InstallationType::NpmGlobal;
            }
        }
    }

    InstallationType::Unknown
}

/// Get the installation path.
pub async fn get_installation_path() -> String {
    if std::env::var("NODE_ENV").ok().as_deref() == Some("development") {
        return std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
    }

    if is_in_bundled_mode() {
        // Try realpath of exec
        if let Ok(path) = tokio::fs::canonicalize(std::env::current_exe().unwrap_or_default()).await
        {
            return path.to_string_lossy().to_string();
        }

        // Try which mossen
        if let Ok(output) = tokio::process::Command::new("which")
            .arg("mossen")
            .output()
            .await
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return path;
                }
            }
        }

        // Check common locations
        if let Some(home) = dirs::home_dir() {
            let local_bin = home.join(".local/bin/mossen");
            if local_bin.exists() {
                return local_bin.to_string_lossy().to_string();
            }
        }

        return "native".to_string();
    }

    std::env::args().next().unwrap_or_else(|| "unknown".to_string())
}

/// Get the invoked binary path.
pub fn get_invoked_binary() -> String {
    if is_in_bundled_mode() {
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    } else {
        std::env::args().nth(1).unwrap_or_else(|| "unknown".to_string())
    }
}

/// Detect multiple installations.
pub async fn detect_multiple_installations() -> Vec<InstallationEntry> {
    let mut installations = Vec::new();

    // Check for local installation
    if let Some(home) = dirs::home_dir() {
        let local_path = home.join(".mossen").join("local");
        if local_path.exists() {
            installations.push(InstallationEntry {
                install_type: "npm-local".to_string(),
                path: local_path.to_string_lossy().to_string(),
            });
        }

        // Check for native installation
        let native_bin = home.join(".local").join("bin").join("mossen");
        if native_bin.exists() {
            installations.push(InstallationEntry {
                install_type: "native".to_string(),
                path: native_bin.to_string_lossy().to_string(),
            });
        }
    }

    // Check for global npm installation
    if let Ok(output) = tokio::process::Command::new("npm")
        .args(["-g", "config", "get", "prefix"])
        .output()
        .await
    {
        if output.status.success() {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !prefix.is_empty() {
                let global_bin = PathBuf::from(&prefix).join("bin").join("mossen");
                if global_bin.exists() {
                    installations.push(InstallationEntry {
                        install_type: "npm-global".to_string(),
                        path: global_bin.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    installations
}

/// Detect configuration issues.
pub async fn detect_configuration_issues(
    install_type: &InstallationType,
) -> Vec<DiagnosticWarning> {
    let mut warnings = Vec::new();

    if *install_type == InstallationType::Development {
        return warnings;
    }

    // Check if ~/.local/bin is in PATH for native installations
    if *install_type == InstallationType::Native {
        let path_var = std::env::var("PATH").unwrap_or_default();
        let local_bin = dirs::home_dir()
            .map(|h| h.join(".local").join("bin").to_string_lossy().to_string())
            .unwrap_or_default();

        if !path_var.split(':').any(|p| p.trim_end_matches('/') == local_bin.trim_end_matches('/')) {
            warnings.push(DiagnosticWarning {
                issue: "Native installation exists but ~/.local/bin is not in your PATH"
                    .to_string(),
                fix: "Run: echo 'export PATH=\"$HOME/.local/bin:$PATH\"' >> ~/.bashrc then open a new terminal"
                    .to_string(),
            });
        }
    }

    warnings
}

/// Detect glob pattern warnings for Linux sandboxing.
pub fn detect_linux_glob_pattern_warnings() -> Vec<DiagnosticWarning> {
    #[cfg(target_os = "linux")]
    {
        // In a real implementation, would check sandbox configuration
        Vec::new()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

/// Run the full doctor diagnostic.
pub async fn get_doctor_diagnostic() -> DiagnosticInfo {
    let installation_type = get_current_installation_type().await;
    let version = option_env!("CARGO_PKG_VERSION")
        .unwrap_or("unknown")
        .to_string();
    let installation_path = get_installation_path().await;
    let invoked_binary = get_invoked_binary();
    let multiple_installations = detect_multiple_installations().await;
    let mut warnings = detect_configuration_issues(&installation_type).await;

    // Add glob pattern warnings for Linux
    warnings.extend(detect_linux_glob_pattern_warnings());

    // Add warnings for leftover npm installations when running native
    if installation_type == InstallationType::Native {
        let npm_installs: Vec<&InstallationEntry> = multiple_installations
            .iter()
            .filter(|i| {
                i.install_type == "npm-global"
                    || i.install_type == "npm-global-orphan"
                    || i.install_type == "npm-local"
            })
            .collect();

        for install in npm_installs {
            match install.install_type.as_str() {
                "npm-global" => {
                    warnings.push(DiagnosticWarning {
                        issue: format!(
                            "Leftover npm global installation at {}",
                            install.path
                        ),
                        fix: "Run: npm -g uninstall @mossen/mossen-code".to_string(),
                    });
                }
                "npm-global-orphan" => {
                    warnings.push(DiagnosticWarning {
                        issue: format!(
                            "Orphaned npm global package at {}",
                            install.path
                        ),
                        fix: format!("Run: rm -rf {}", install.path),
                    });
                }
                "npm-local" => {
                    warnings.push(DiagnosticWarning {
                        issue: format!(
                            "Leftover npm local installation at {}",
                            install.path
                        ),
                        fix: format!("Run: rm -rf {}", install.path),
                    });
                }
                _ => {}
            }
        }
    }

    let ripgrep_status = RipgrepStatus {
        working: true,
        mode: "system".to_string(),
        system_path: which_ripgrep().await,
    };

    DiagnosticInfo {
        installation_type,
        version,
        installation_path,
        invoked_binary,
        config_install_method: "not set".to_string(),
        auto_updates: "enabled".to_string(),
        has_update_permissions: None,
        multiple_installations,
        warnings,
        recommendation: None,
        package_manager: None,
        ripgrep_status,
    }
}

/// Check if running in bundled mode.
fn is_in_bundled_mode() -> bool {
    // In Rust, check if we're a compiled binary
    std::env::current_exe()
        .map(|p| !p.to_string_lossy().contains("target/debug"))
        .unwrap_or(false)
}

/// Detect if installed via Homebrew.
fn detect_homebrew() -> bool {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().contains("/Cellar/") || p.to_string_lossy().contains("/Caskroom/"))
        .unwrap_or(false)
}

/// Detect if installed via a package manager.
async fn detect_package_manager() -> bool {
    // Check various package managers
    detect_homebrew()
}

/// Find ripgrep binary path.
async fn which_ripgrep() -> Option<String> {
    let output = tokio::process::Command::new("which")
        .arg("rg")
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(path);
        }
    }
    None
}
