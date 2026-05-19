//! Centralized plugin directory configuration.
//!
//! Translated from `utils/plugins/pluginDirectories.ts` (178 lines).

use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

/// Get the plugins directory name based on current mode.
fn get_plugins_directory_name() -> &'static str {
    if std::env::var("MOSSEN_CODE_USE_COWORK_PLUGINS")
        .ok()
        .map(|v| is_env_truthy(&v))
        .unwrap_or(false)
    {
        "cowork_plugins"
    } else {
        "plugins"
    }
}

fn is_env_truthy(val: &str) -> bool {
    matches!(val.to_lowercase().as_str(), "1" | "true" | "yes")
}

/// Get the Mossen config home directory (~/.mossen).
fn get_mossen_config_home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("MOSSEN_CONFIG_HOME") {
        return PathBuf::from(shellexpand::tilde(&home).into_owned());
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mossen")
}

/// Get the full path to the plugins directory.
pub fn get_plugins_directory() -> PathBuf {
    if let Ok(env_override) = std::env::var("MOSSEN_CODE_PLUGIN_CACHE_DIR") {
        return PathBuf::from(shellexpand::tilde(&env_override).into_owned());
    }
    get_mossen_config_home_dir().join(get_plugins_directory_name())
}

/// Get the read-only plugin seed directories, if configured.
pub fn get_plugin_seed_dirs() -> Vec<PathBuf> {
    let raw = match std::env::var("MOSSEN_CODE_PLUGIN_SEED_DIR") {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    raw.split(std::path::MAIN_SEPARATOR)
        .filter(|s| !s.is_empty())
        .map(|s| PathBuf::from(shellexpand::tilde(s).into_owned()))
        .collect()
}

/// Sanitize a plugin ID for use in filesystem paths.
fn sanitize_plugin_id(plugin_id: &str) -> String {
    static SANITIZE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"[^a-zA-Z0-9\-_]").unwrap());
    SANITIZE_RE.replace_all(plugin_id, "-").to_string()
}

/// Pure path — no mkdir. For display (e.g. uninstall dialog).
pub fn plugin_data_dir_path(plugin_id: &str) -> PathBuf {
    get_plugins_directory()
        .join("data")
        .join(sanitize_plugin_id(plugin_id))
}

/// Persistent per-plugin data directory.
///
/// Creates the directory on call (mkdir). Unlike the version-scoped install cache
/// which is orphaned and GC'd on every update, this survives plugin updates —
/// only removed on last-scope uninstall.
pub fn get_plugin_data_dir(plugin_id: &str) -> PathBuf {
    let dir = plugin_data_dir_path(plugin_id);
    // Sync mkdir — called from substitutePluginVariables which is sync
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Size of the data dir for the uninstall confirmation prompt.
/// Returns None when the dir is absent or empty.
pub async fn get_plugin_data_dir_size(
    plugin_id: &str,
) -> Option<(u64, String)> {
    let dir = plugin_data_dir_path(plugin_id);
    let mut bytes: u64 = 0;

    async fn walk(p: &Path, bytes: &mut u64) -> std::io::Result<()> {
        let mut entries = fs::read_dir(p).await?;
        while let Some(entry) = entries.next_entry().await? {
            let full = entry.path();
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                Box::pin(walk(&full, bytes)).await?;
            } else {
                match fs::metadata(&full).await {
                    Ok(meta) => *bytes += meta.len(),
                    Err(_) => {} // Broken symlink / raced delete
                }
            }
        }
        Ok(())
    }

    match walk(&dir, &mut bytes).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(e) => {
            debug!("Error walking plugin data dir: {}", e);
            return None;
        }
    }

    if bytes == 0 {
        return None;
    }

    Some((bytes, format_file_size(bytes)))
}

/// Format a byte size into human-readable form.
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Best-effort cleanup on last-scope uninstall.
pub async fn delete_plugin_data_dir(plugin_id: &str) {
    let dir = plugin_data_dir_path(plugin_id);
    if let Err(e) = fs::remove_dir_all(&dir).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            debug!(
                "Failed to delete plugin data dir {}: {}",
                dir.display(),
                e
            );
        }
    }
}
