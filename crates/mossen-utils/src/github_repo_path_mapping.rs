use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

/// Updates the GitHub repository path mapping in global config.
/// Stores the git root so the mapping always points to the repository root.
pub async fn update_github_repo_path_mapping(
    cwd: &Path,
    git_root: Option<&Path>,
    config: &mut GlobalConfig,
    detect_repo_fn: impl AsyncDetectRepo,
) {
    let repo = match detect_repo_fn.detect().await {
        Some(r) => r,
        None => return,
    };

    let base_path = git_root.unwrap_or(cwd);

    // Resolve symlinks for canonical storage
    let current_path = match fs::canonicalize(base_path).await {
        Ok(p) => {
            // NFC normalization for macOS
            #[cfg(target_os = "macos")]
            {
                use unicode_normalization::UnicodeNormalization;
                PathBuf::from(p.to_string_lossy().nfc().collect::<String>())
            }
            #[cfg(not(target_os = "macos"))]
            {
                p
            }
        }
        Err(_) => base_path.to_path_buf(),
    };

    let repo_key = repo.to_lowercase();
    let current_path_str = current_path.to_string_lossy().to_string();

    let existing_paths = config
        .github_repo_paths
        .entry(repo_key.clone())
        .or_insert_with(Vec::new);

    if existing_paths.first().map(|s| s.as_str()) == Some(&current_path_str) {
        // Already at the front
        return;
    }

    // Remove if present elsewhere, then prepend
    existing_paths.retain(|p| p != &current_path_str);
    existing_paths.insert(0, current_path_str);
}

/// Gets known local paths for a given GitHub repository.
pub fn get_known_paths_for_repo(config: &GlobalConfig, repo: &str) -> Vec<String> {
    let repo_key = repo.to_lowercase();
    config
        .github_repo_paths
        .get(&repo_key)
        .cloned()
        .unwrap_or_default()
}

/// Filters paths to only those that exist on the filesystem.
pub async fn filter_existing_paths(paths: &[String]) -> Vec<String> {
    let mut results = Vec::new();
    for path in paths {
        if fs::metadata(path).await.is_ok() {
            results.push(path.clone());
        }
    }
    results
}

/// Validates that a path contains the expected GitHub repository.
pub async fn validate_repo_at_path(
    path: &Path,
    expected_repo: &str,
    get_remote_url: impl AsyncGetRemoteUrl,
    parse_github_repo: impl Fn(&str) -> Option<String>,
) -> bool {
    let remote_url = match get_remote_url.get_remote_url(path).await {
        Some(url) => url,
        None => return false,
    };

    let actual_repo = match parse_github_repo(&remote_url) {
        Some(r) => r,
        None => return false,
    };

    actual_repo.to_lowercase() == expected_repo.to_lowercase()
}

/// Removes a path from the tracked paths for a given repository.
pub fn remove_path_from_repo(config: &mut GlobalConfig, repo: &str, path_to_remove: &str) {
    let repo_key = repo.to_lowercase();

    let paths = match config.github_repo_paths.get_mut(&repo_key) {
        Some(p) => p,
        None => return,
    };

    let original_len = paths.len();
    paths.retain(|p| p != path_to_remove);

    if paths.len() == original_len {
        return; // Path wasn't in the list
    }

    if paths.is_empty() {
        config.github_repo_paths.remove(&repo_key);
    }
}

/// Simplified global config for repo path mapping.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub github_repo_paths: std::collections::HashMap<String, Vec<String>>,
}

/// Trait for async repository detection.
#[async_trait::async_trait]
pub trait AsyncDetectRepo: Send + Sync {
    async fn detect(&self) -> Option<String>;
}

/// Trait for async git remote URL retrieval.
#[async_trait::async_trait]
pub trait AsyncGetRemoteUrl: Send + Sync {
    async fn get_remote_url(&self, path: &Path) -> Option<String>;
}
