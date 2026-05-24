//! Plugin version calculation.
//!
//! Translated from `utils/plugins/pluginVersioning.ts` (157 lines).

use sha2::{Digest, Sha256};
use std::path::Path;
use tracing::debug;

use super::schemas::PluginSource;

/// Calculate the version for a plugin based on its source.
///
/// Version sources (in order of priority):
/// 1. plugin.json version field (highest priority)
/// 2. Provided version (typically from marketplace entry)
/// 3. Git commit SHA from install path
/// 4. 'unknown' as last resort
pub async fn calculate_plugin_version(
    plugin_id: &str,
    source: &PluginSource,
    manifest_version: Option<&str>,
    install_path: Option<&str>,
    provided_version: Option<&str>,
    git_commit_sha: Option<&str>,
) -> String {
    // 1. Use explicit version from plugin.json if available
    if let Some(version) = manifest_version {
        if !version.is_empty() {
            debug!("Using manifest version for {}: {}", plugin_id, version);
            return version.to_string();
        }
    }

    // 2. Use provided version (typically from marketplace entry)
    if let Some(version) = provided_version {
        if !version.is_empty() {
            debug!("Using provided version for {}: {}", plugin_id, version);
            return version.to_string();
        }
    }

    // 3. Use pre-resolved git SHA if caller captured it before discarding the clone
    if let Some(sha) = git_commit_sha {
        if !sha.is_empty() {
            let short_sha = &sha[..sha.len().min(12)];
            // Check for git-subdir source type
            if let PluginSource::Structured(ref structured) = source {
                if let super::schemas::StructuredPluginSource::GitSubdir { path, .. } = structured {
                    // Encode the subdir path in the version
                    let norm_path = path
                        .replace('\\', "/")
                        .trim_start_matches("./")
                        .trim_end_matches('/')
                        .to_string();
                    let mut hasher = Sha256::new();
                    hasher.update(norm_path.as_bytes());
                    let hash = hex::encode(hasher.finalize());
                    let path_hash = &hash[..8];
                    let v = format!("{}-{}", short_sha, path_hash);
                    debug!(
                        "Using git-subdir SHA+path version for {}: {} (path={})",
                        plugin_id, v, norm_path
                    );
                    return v;
                }
            }
            debug!(
                "Using pre-resolved git SHA for {}: {}",
                plugin_id, short_sha
            );
            return short_sha.to_string();
        }
    }

    // 4. Try to get git SHA from install path
    if let Some(path) = install_path {
        if let Some(sha) = get_git_commit_sha(path).await {
            let short_sha = &sha[..sha.len().min(12)];
            debug!("Using git SHA for {}: {}", plugin_id, short_sha);
            return short_sha.to_string();
        }
    }

    // 5. Return 'unknown' as last resort
    debug!("No version found for {}, using 'unknown'", plugin_id);
    "unknown".to_string()
}

/// Get the git commit SHA for a directory.
pub async fn get_git_commit_sha(dir_path: &str) -> Option<String> {
    let head_file = Path::new(dir_path).join(".git").join("HEAD");
    match tokio::fs::read_to_string(&head_file).await {
        Ok(content) => {
            let content = content.trim();
            if content.starts_with("ref: ") {
                // It's a symbolic reference, resolve it
                let ref_path = &content[5..];
                let full_ref_path = Path::new(dir_path).join(".git").join(ref_path);
                tokio::fs::read_to_string(&full_ref_path)
                    .await
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                // It's a direct SHA
                Some(content.to_string())
            }
        }
        Err(_) => None,
    }
}

/// Extract version from a versioned cache path.
///
/// Given a path like `~/.mossen/plugins/cache/marketplace/plugin/1.0.0`,
/// extracts and returns `1.0.0`.
pub fn get_version_from_path(install_path: &str) -> Option<String> {
    let parts: Vec<&str> = install_path.split('/').filter(|p| !p.is_empty()).collect();

    // Find 'cache' index where the parent is 'plugins'
    let cache_index = parts.iter().position(|&part| {
        part == "cache"
            && parts.iter().position(|&p| p == part).and_then(|i| {
                if i > 0 {
                    Some(parts[i - 1])
                } else {
                    None
                }
            }) == Some("plugins")
    });

    let cache_index = cache_index?;

    // Versioned path has 3 components after 'cache': marketplace/plugin/version
    let components_after_cache: Vec<&str> = parts[cache_index + 1..].to_vec();
    if components_after_cache.len() >= 3 {
        let version = components_after_cache[2];
        if !version.is_empty() {
            return Some(version.to_string());
        }
    }

    None
}

/// Check if a path is a versioned plugin path.
pub fn is_versioned_path(path: &str) -> bool {
    get_version_from_path(path).is_some()
}
