//! Marketplace manager for Mossen plugins
//!
//! This module provides functionality to:
//! - Manage known marketplace sources (URLs, GitHub repos, npm packages, local files)
//! - Cache marketplace manifests locally for offline access
//! - Install plugins from marketplace entries
//! - Track and update marketplace configurations

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::schemas::{
    KnownMarketplace, KnownMarketplacesFile, MarketplaceSource,
    PluginMarketplace, PluginMarketplaceEntry,
};

/// Result of loading and caching a marketplace
pub struct LoadedPluginMarketplace {
    pub marketplace: PluginMarketplace,
    pub cache_path: String,
}

/// Progress callback type for marketplace operations
pub type MarketplaceProgressCallback = Box<dyn Fn(&str) + Send + Sync>;

/// Declared marketplace entry (intent layer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclaredMarketplace {
    pub source: MarketplaceSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_update: Option<bool>,
    /// When set, diffMarketplaces treats an already-materialized entry as up-to-date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_is_fallback: Option<bool>,
}

/// Git operation result
#[derive(Debug, Clone)]
pub struct GitResult {
    pub code: i32,
    pub stderr: String,
    pub error: Option<String>,
    pub stdout: Option<String>,
}

/// Default git timeout in ms
const DEFAULT_PLUGIN_GIT_TIMEOUT_MS: u64 = 120_000;

/// Git no-prompt environment variables
fn git_no_prompt_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("GIT_TERMINAL_PROMPT".to_string(), "0".to_string());
    env.insert("GIT_ASKPASS".to_string(), String::new());
    env
}

/// Trait for marketplace manager external dependencies
#[async_trait::async_trait]
pub trait MarketplaceEnv: Send + Sync {
    fn get_plugins_directory(&self) -> PathBuf;
    fn get_plugin_seed_dirs(&self) -> Vec<PathBuf>;
    fn get_official_marketplace_name(&self) -> &str;
    fn get_official_marketplace_source(&self) -> MarketplaceSource;

    // Settings
    fn get_initial_settings_enabled_plugins(&self) -> HashMap<String, serde_json::Value>;
    fn get_initial_settings_extra_marketplaces(
        &self,
    ) -> HashMap<String, DeclaredMarketplace>;
    fn get_settings_for_source(
        &self,
        source: &str,
    ) -> Option<HashMap<String, serde_json::Value>>;
    fn update_settings_for_source(
        &self,
        source: &str,
        updates: serde_json::Value,
    ) -> Result<()>;
    fn get_add_dir_enabled_plugins(&self) -> HashMap<String, serde_json::Value>;
    fn get_add_dir_extra_marketplaces(&self) -> HashMap<String, DeclaredMarketplace>;
    fn parse_plugin_identifier(&self, plugin_id: &str) -> (Option<String>, Option<String>);
    fn is_env_truthy(&self, key: &str) -> bool;
    fn get_feature_value_cached(&self, key: &str, default: bool) -> bool;

    // Policy
    fn is_source_allowed_by_policy(&self, source: &MarketplaceSource) -> bool;
    fn is_source_in_blocklist(&self, source: &MarketplaceSource) -> bool;
    fn format_source_for_display(&self, source: &MarketplaceSource) -> String;
    fn get_strict_known_marketplaces(&self) -> Vec<MarketplaceSource>;
    fn get_host_patterns_from_allowlist(&self) -> Vec<String>;
    fn extract_host_from_source(&self, source: &MarketplaceSource) -> Option<String>;
    fn is_local_marketplace_source(&self, source: &MarketplaceSource) -> bool;
    fn validate_official_name_source(
        &self,
        name: &str,
        source: &MarketplaceSource,
    ) -> Option<String>;

    // Filesystem
    async fn read_file(&self, path: &Path) -> Result<String>;
    fn read_file_sync(&self, path: &Path) -> Result<String>;
    async fn write_file(&self, path: &Path, content: &str) -> Result<()>;
    fn write_file_sync(&self, path: &Path, content: &str) -> Result<()>;
    async fn mkdir(&self, path: &Path) -> Result<()>;
    async fn rm(&self, path: &Path) -> Result<()>;
    async fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    async fn stat(&self, path: &Path) -> Result<()>;
    fn file_exists(&self, path: &Path) -> bool;

    // Git operations
    async fn exec_git(&self, args: &[&str], cwd: Option<&Path>, timeout_ms: u64) -> GitResult;
    async fn exec_ssh_test(&self, args: &[&str], timeout_ms: u64) -> GitResult;
    fn git_exe(&self) -> String;

    // HTTP
    async fn http_get(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        timeout_ms: u64,
    ) -> Result<serde_json::Value>;

    // Installed plugins manager
    fn remove_all_plugins_for_marketplace(
        &self,
        name: &str,
    ) -> (Vec<String>, Vec<String>);
    async fn mark_plugin_version_orphaned(&self, install_path: &str) -> Result<()>;
    fn delete_plugin_options(&self, plugin_id: &str);
    async fn delete_plugin_data_dir(&self, plugin_id: &str) -> Result<()>;

    // GCS
    async fn fetch_official_marketplace_from_gcs(
        &self,
        install_location: &str,
        cache_dir: &str,
    ) -> Option<String>;

    // Telemetry
    fn log_plugin_fetch(
        &self,
        operation: &str,
        url: &str,
        status: &str,
        duration_ms: f64,
        error_class: Option<&str>,
    );
    fn classify_fetch_error(&self, error: &str) -> String;
}

/// Get the path to the known marketplaces configuration file
pub fn get_known_marketplaces_file(env: &dyn MarketplaceEnv) -> PathBuf {
    env.get_plugins_directory().join("known_marketplaces.json")
}

/// Get the path to the marketplaces cache directory
pub fn get_marketplaces_cache_dir(env: &dyn MarketplaceEnv) -> PathBuf {
    env.get_plugins_directory().join("marketplaces")
}

/// Memoized marketplace cache
static MARKETPLACE_CACHE: Lazy<Mutex<HashMap<String, PluginMarketplace>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Clear all cached marketplace data
pub fn clear_marketplaces_cache() {
    MARKETPLACE_CACHE.lock().unwrap().clear();
}

/// Get declared marketplace intent from merged settings and --add-dir sources.
pub fn get_declared_marketplaces(
    env: &dyn MarketplaceEnv,
) -> HashMap<String, DeclaredMarketplace> {
    let mut implicit: HashMap<String, DeclaredMarketplace> = HashMap::new();

    // Only official marketplace can be implicitly declared
    let mut enabled_plugins = env.get_add_dir_enabled_plugins();
    for (k, v) in env.get_initial_settings_enabled_plugins() {
        enabled_plugins.insert(k, v);
    }

    let official_name = env.get_official_marketplace_name();
    for (plugin_id, value) in &enabled_plugins {
        if value.as_bool().unwrap_or(false) || value.is_object() {
            let (_, marketplace) = env.parse_plugin_identifier(plugin_id);
            if marketplace.as_deref() == Some(official_name) {
                implicit.insert(
                    official_name.to_string(),
                    DeclaredMarketplace {
                        source: env.get_official_marketplace_source(),
                        install_location: None,
                        auto_update: None,
                        source_is_fallback: Some(true),
                    },
                );
                break;
            }
        }
    }

    // Lowest precedence: implicit < --add-dir < merged settings
    let mut result = implicit;
    for (k, v) in env.get_add_dir_extra_marketplaces() {
        result.insert(k, v);
    }
    for (k, v) in env.get_initial_settings_extra_marketplaces() {
        result.insert(k, v);
    }
    result
}

/// Find which editable settings source declared a marketplace.
pub fn get_marketplace_declaring_source(
    env: &dyn MarketplaceEnv,
    name: &str,
) -> Option<&'static str> {
    let editable_sources = ["localSettings", "projectSettings", "userSettings"];
    for source in &editable_sources {
        if let Some(settings) = env.get_settings_for_source(source) {
            if settings.contains_key(name) {
                return Some(match *source {
                    "localSettings" => "localSettings",
                    "projectSettings" => "projectSettings",
                    "userSettings" => "userSettings",
                    _ => unreachable!(),
                });
            }
        }
    }
    None
}

/// Save a marketplace entry to settings (intent layer).
pub fn save_marketplace_to_settings(
    env: &dyn MarketplaceEnv,
    name: &str,
    entry: &DeclaredMarketplace,
    setting_source: &str,
) {
    let updates = serde_json::json!({
        "extraKnownMarketplaces": {
            name: entry
        }
    });
    let _ = env.update_settings_for_source(setting_source, updates);
}

/// Load known marketplaces configuration from disk
pub async fn load_known_marketplaces_config(
    env: &dyn MarketplaceEnv,
) -> Result<KnownMarketplacesFile> {
    let config_file = get_known_marketplaces_file(env);
    let content = match env.read_file(&config_file).await {
        Ok(c) => c,
        Err(_) => return Ok(HashMap::new()),
    };
    let data: KnownMarketplacesFile = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Failed to load marketplace configuration: {}", e))?;
    Ok(data)
}

/// Load known marketplaces config, returning {} on any error.
pub async fn load_known_marketplaces_config_safe(
    env: &dyn MarketplaceEnv,
) -> KnownMarketplacesFile {
    load_known_marketplaces_config(env).await.unwrap_or_default()
}

/// Save known marketplaces configuration to disk
pub async fn save_known_marketplaces_config(
    env: &dyn MarketplaceEnv,
    config: &KnownMarketplacesFile,
) -> Result<()> {
    let config_file = get_known_marketplaces_file(env);
    let dir = config_file.parent().unwrap().to_path_buf();
    env.mkdir(&dir).await?;
    let json = serde_json::to_string_pretty(config)?;
    env.write_file_sync(&config_file, &json)?;
    Ok(())
}

/// Register marketplaces from seed directories into known_marketplaces.json.
pub async fn register_seed_marketplaces(env: &dyn MarketplaceEnv) -> Result<bool> {
    let seed_dirs = env.get_plugin_seed_dirs();
    if seed_dirs.is_empty() {
        return Ok(false);
    }

    let mut primary = load_known_marketplaces_config(env).await?;
    let mut claimed = std::collections::HashSet::new();
    let mut changed = 0u32;

    for seed_dir in &seed_dirs {
        let seed_config = match read_seed_known_marketplaces(env, seed_dir).await {
            Some(c) => c,
            None => continue,
        };

        for (name, seed_entry) in &seed_config {
            if claimed.contains(name) {
                continue;
            }

            let resolved_location =
                match find_seed_marketplace_location(env, seed_dir, name).await {
                    Some(loc) => loc,
                    None => {
                        debug!(
                            "Seed marketplace '{}' not found under {:?}/marketplaces/, skipping",
                            name, seed_dir
                        );
                        continue;
                    }
                };
            claimed.insert(name.clone());

            let desired = KnownMarketplace {
                source: seed_entry.source.clone(),
                install_location: resolved_location,
                last_updated: seed_entry.last_updated.clone(),
                auto_update: Some(false),
            };

            if primary.get(name) == Some(&desired) {
                continue;
            }

            primary.insert(name.clone(), desired);
            changed += 1;
        }
    }

    if changed > 0 {
        save_known_marketplaces_config(env, &primary).await?;
        debug!("Synced {} marketplace(s) from seed dir(s)", changed);
        return Ok(true);
    }
    Ok(false)
}

async fn read_seed_known_marketplaces(
    env: &dyn MarketplaceEnv,
    seed_dir: &Path,
) -> Option<KnownMarketplacesFile> {
    let seed_json_path = seed_dir.join("known_marketplaces.json");
    let content = env.read_file(&seed_json_path).await.ok()?;
    serde_json::from_str(&content).ok()
}

async fn find_seed_marketplace_location(
    env: &dyn MarketplaceEnv,
    seed_dir: &Path,
    name: &str,
) -> Option<String> {
    let dir_candidate = seed_dir.join("marketplaces").join(name);
    let json_candidate = seed_dir
        .join("marketplaces")
        .join(format!("{}.json", name));

    for candidate in &[&dir_candidate, &json_candidate] {
        if read_cached_marketplace(env, candidate).await.is_ok() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

/// Check if installLocation points into a configured seed directory
fn seed_dir_for(env: &dyn MarketplaceEnv, install_location: &str) -> Option<PathBuf> {
    env.get_plugin_seed_dirs().into_iter().find(|d| {
        let d_str = d.to_string_lossy();
        install_location == d_str.as_ref()
            || install_location.starts_with(&format!("{}/", d_str))
    })
}

/// Get plugin git timeout in ms
fn get_plugin_git_timeout_ms() -> u64 {
    std::env::var("MOSSEN_CODE_PLUGIN_GIT_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(DEFAULT_PLUGIN_GIT_TIMEOUT_MS)
}

/// Git pull operation
pub async fn git_pull(
    env: &dyn MarketplaceEnv,
    cwd: &str,
    git_ref: Option<&str>,
    options: Option<&GitPullOptions>,
) -> GitResult {
    debug!("git pull: cwd={} ref={}", cwd, git_ref.unwrap_or("default"));
    let timeout = get_plugin_git_timeout_ms();
    let cwd_path = Path::new(cwd);

    let mut credential_args: Vec<&str> = Vec::new();
    if options.map(|o| o.disable_credential_helper).unwrap_or(false) {
        credential_args.extend_from_slice(&["-c", "credential.helper="]);
    }

    if let Some(r) = git_ref {
        // Fetch
        let mut args: Vec<&str> = credential_args.clone();
        args.extend_from_slice(&["fetch", "origin", r]);
        let fetch_result = env.exec_git(&args, Some(cwd_path), timeout).await;
        if fetch_result.code != 0 {
            return enhance_git_pull_error_messages(fetch_result);
        }

        // Checkout
        let mut args: Vec<&str> = credential_args.clone();
        args.extend_from_slice(&["checkout", r]);
        let checkout_result = env.exec_git(&args, Some(cwd_path), timeout).await;
        if checkout_result.code != 0 {
            return enhance_git_pull_error_messages(checkout_result);
        }

        // Pull
        let mut args: Vec<&str> = credential_args.clone();
        args.extend_from_slice(&["pull", "origin", r]);
        let pull_result = env.exec_git(&args, Some(cwd_path), timeout).await;
        if pull_result.code != 0 {
            return enhance_git_pull_error_messages(pull_result);
        }
        git_submodule_update(env, cwd, &credential_args, options.and_then(|o| o.sparse_paths.as_deref())).await;
        return pull_result;
    }

    let mut args: Vec<&str> = credential_args.clone();
    args.extend_from_slice(&["pull", "origin", "HEAD"]);
    let result = env.exec_git(&args, Some(cwd_path), timeout).await;
    if result.code != 0 {
        return enhance_git_pull_error_messages(result);
    }
    git_submodule_update(env, cwd, &credential_args, options.and_then(|o| o.sparse_paths.as_deref())).await;
    result
}

/// Options for git pull
pub struct GitPullOptions {
    pub disable_credential_helper: bool,
    pub sparse_paths: Option<Vec<String>>,
}

async fn git_submodule_update(
    env: &dyn MarketplaceEnv,
    cwd: &str,
    credential_args: &[&str],
    sparse_paths: Option<&[String]>,
) {
    if sparse_paths.map(|p| !p.is_empty()).unwrap_or(false) {
        return;
    }
    let gitmodules_path = Path::new(cwd).join(".gitmodules");
    if env.stat(&gitmodules_path).await.is_err() {
        return;
    }

    let mut args: Vec<&str> = vec![
        "-c",
        "core.sshCommand=ssh -o BatchMode=yes -o StrictHostKeyChecking=yes",
    ];
    args.extend_from_slice(credential_args);
    args.extend_from_slice(&[
        "submodule", "update", "--init", "--recursive", "--depth", "1",
    ]);

    let result = env
        .exec_git(&args, Some(Path::new(cwd)), get_plugin_git_timeout_ms())
        .await;
    if result.code != 0 {
        debug!(
            "git submodule update failed (non-fatal): {}",
            result.stderr
        );
    }
}

fn enhance_git_pull_error_messages(result: GitResult) -> GitResult {
    if result.code == 0 {
        return result;
    }

    if result.error.as_deref().unwrap_or("").contains("timed out") {
        let timeout_sec = get_plugin_git_timeout_ms() / 1000;
        return GitResult {
            stderr: format!(
                "Git pull timed out after {}s. Try increasing the timeout via MOSSEN_CODE_PLUGIN_GIT_TIMEOUT_MS environment variable.\n\nOriginal error: {}",
                timeout_sec, result.stderr
            ),
            ..result
        };
    }

    if result.stderr.contains("REMOTE HOST IDENTIFICATION HAS CHANGED") {
        return GitResult {
            stderr: format!(
                "SSH host key for this marketplace's git host has changed (server key rotation or possible MITM). Remove the stale entry with: ssh-keygen -R <host>\nThen connect once manually to accept the new key.\n\nOriginal error: {}",
                result.stderr
            ),
            ..result
        };
    }
    if result.stderr.contains("Host key verification failed") {
        return GitResult {
            stderr: format!(
                "SSH host key verification failed while updating marketplace. The host key is not in your known_hosts file. Connect once manually to add it (e.g., ssh -T git@<host>), or remove and re-add the marketplace with an HTTPS URL.\n\nOriginal error: {}",
                result.stderr
            ),
            ..result
        };
    }

    if result.stderr.contains("Permission denied (publickey)")
        || result.stderr.contains("Could not read from remote repository")
    {
        return GitResult {
            stderr: format!(
                "SSH authentication failed while updating marketplace. Please ensure your SSH keys are configured.\n\nOriginal error: {}",
                result.stderr
            ),
            ..result
        };
    }

    if result.stderr.contains("timed out") || result.stderr.contains("Could not resolve host") {
        return GitResult {
            stderr: format!(
                "Network error while updating marketplace. Please check your internet connection.\n\nOriginal error: {}",
                result.stderr
            ),
            ..result
        };
    }

    result
}

/// Check if SSH is likely to work for GitHub
async fn is_github_ssh_likely_configured(env: &dyn MarketplaceEnv) -> bool {
    let result = env
        .exec_ssh_test(
            &[
                "-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=2", "-o",
                "StrictHostKeyChecking=yes", "git@github.com",
            ],
            3000,
        )
        .await;

    let configured = result.code == 1
        && (result.stderr.contains("successfully authenticated")
            || result
                .stdout
                .as_deref()
                .unwrap_or("")
                .contains("successfully authenticated"));
    debug!(
        "SSH config check: code={} configured={}",
        result.code, configured
    );
    configured
}

fn is_authentication_error(stderr: &str) -> bool {
    stderr.contains("Authentication failed")
        || stderr.contains("could not read Username")
        || stderr.contains("terminal prompts disabled")
        || stderr.contains("403")
        || stderr.contains("401")
}

fn extract_ssh_host(git_url: &str) -> Option<String> {
    let re = regex::Regex::new(r"^[^@]+@([^:]+):").ok()?;
    re.captures(git_url).map(|c| c[1].to_string())
}

/// Git clone operation
pub async fn git_clone(
    env: &dyn MarketplaceEnv,
    git_url: &str,
    target_path: &str,
    git_ref: Option<&str>,
    sparse_paths: Option<&[String]>,
) -> GitResult {
    let use_sparse = sparse_paths.map(|p| !p.is_empty()).unwrap_or(false);
    let timeout_ms = get_plugin_git_timeout_ms();

    let mut args: Vec<String> = vec![
        "-c".to_string(),
        "core.sshCommand=ssh -o BatchMode=yes -o StrictHostKeyChecking=yes".to_string(),
        "clone".to_string(),
        "--depth".to_string(),
        "1".to_string(),
    ];

    if use_sparse {
        args.push("--filter=blob:none".to_string());
        args.push("--no-checkout".to_string());
    } else {
        args.push("--recurse-submodules".to_string());
        args.push("--shallow-submodules".to_string());
    }

    if let Some(r) = git_ref {
        args.push("--branch".to_string());
        args.push(r.to_string());
    }

    args.push(git_url.to_string());
    args.push(target_path.to_string());

    debug!(
        "git clone: url={} ref={} timeout={}ms",
        redact_url_credentials(git_url),
        git_ref.unwrap_or("default"),
        timeout_ms
    );

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let mut result = env.exec_git(&args_refs, None, timeout_ms).await;

    // Redact credentials
    let redacted = redact_url_credentials(git_url);
    if git_url != redacted {
        result.stderr = result.stderr.replace(git_url, &redacted);
        if let Some(ref mut err) = result.error {
            *err = err.replace(git_url, &redacted);
        }
    }

    if result.code == 0 {
        if use_sparse {
            let sparse_paths_unwrap = sparse_paths.unwrap();
            let mut sparse_args: Vec<String> = vec![
                "sparse-checkout".to_string(),
                "set".to_string(),
                "--cone".to_string(),
                "--".to_string(),
            ];
            for p in sparse_paths_unwrap {
                sparse_args.push(p.clone());
            }
            let sparse_refs: Vec<&str> = sparse_args.iter().map(|s| s.as_str()).collect();
            let sparse_result = env
                .exec_git(&sparse_refs, Some(Path::new(target_path)), timeout_ms)
                .await;
            if sparse_result.code != 0 {
                return GitResult {
                    code: sparse_result.code,
                    stderr: format!(
                        "git sparse-checkout set failed: {}",
                        sparse_result.stderr
                    ),
                    error: None,
                    stdout: None,
                };
            }

            let checkout_result = env
                .exec_git(
                    &["checkout", "HEAD"],
                    Some(Path::new(target_path)),
                    timeout_ms,
                )
                .await;
            if checkout_result.code != 0 {
                return GitResult {
                    code: checkout_result.code,
                    stderr: format!(
                        "git checkout after sparse-checkout failed: {}",
                        checkout_result.stderr
                    ),
                    error: None,
                    stdout: None,
                };
            }
        }
        debug!("git clone succeeded: {}", redact_url_credentials(git_url));
        return result;
    }

    debug!(
        "git clone failed: url={} code={} stderr={}",
        redact_url_credentials(git_url),
        result.code,
        result.stderr
    );

    // Enhance error messages
    if result.error.as_deref().unwrap_or("").contains("timed out") {
        return GitResult {
            stderr: format!(
                "Git clone timed out after {}s. The repository may be too large for the current timeout. Set MOSSEN_CODE_PLUGIN_GIT_TIMEOUT_MS to increase it.\n\nOriginal error: {}",
                timeout_ms / 1000,
                result.stderr
            ),
            ..result
        };
    }

    if result.stderr.contains("REMOTE HOST IDENTIFICATION HAS CHANGED") {
        let host = extract_ssh_host(git_url);
        let remove_hint = host
            .as_ref()
            .map(|h| format!("ssh-keygen -R {}", h))
            .unwrap_or_else(|| "ssh-keygen -R <host>".to_string());
        return GitResult {
            stderr: format!(
                "SSH host key has changed (server key rotation or possible MITM). Remove the stale known_hosts entry:\n  {}\nThen connect once manually to verify and accept the new key.\n\nOriginal error: {}",
                remove_hint, result.stderr
            ),
            ..result
        };
    }

    if result.stderr.contains("Host key verification failed") {
        let host = extract_ssh_host(git_url);
        let connect_hint = host
            .as_ref()
            .map(|h| format!("ssh -T git@{}", h))
            .unwrap_or_else(|| "ssh -T git@<host>".to_string());
        return GitResult {
            stderr: format!(
                "SSH host key is not in your known_hosts file. To add it, connect once manually:\n  {}\n\nOr use an HTTPS URL instead.\n\nOriginal error: {}",
                connect_hint, result.stderr
            ),
            ..result
        };
    }

    if result.stderr.contains("Permission denied (publickey)")
        || result.stderr.contains("Could not read from remote repository")
    {
        return GitResult {
            stderr: format!(
                "SSH authentication failed. Please ensure your SSH keys are configured for GitHub, or use an HTTPS URL instead.\n\nOriginal error: {}",
                result.stderr
            ),
            ..result
        };
    }

    if is_authentication_error(&result.stderr) {
        return GitResult {
            stderr: format!(
                "HTTPS authentication failed. Please ensure your credential helper is configured.\n\nOriginal error: {}",
                result.stderr
            ),
            ..result
        };
    }

    if result.stderr.contains("timed out")
        || result.stderr.contains("timeout")
        || result.stderr.contains("Could not resolve host")
    {
        return GitResult {
            stderr: format!(
                "Network error or timeout while cloning repository. Please check your internet connection.\n\nOriginal error: {}",
                result.stderr
            ),
            ..result
        };
    }

    if result.stderr.is_empty() {
        return GitResult {
            stderr: result.error.clone().unwrap_or_else(|| {
                format!(
                    "git clone exited with code {} (no stderr output).",
                    result.code
                )
            }),
            ..result
        };
    }

    result
}

/// Reconcile sparse-checkout state with desired config.
pub async fn reconcile_sparse_checkout(
    env: &dyn MarketplaceEnv,
    cwd: &str,
    sparse_paths: Option<&[String]>,
) -> GitResult {
    let timeout = get_plugin_git_timeout_ms();
    let cwd_path = Path::new(cwd);

    if let Some(paths) = sparse_paths {
        if !paths.is_empty() {
            let mut args: Vec<String> = vec![
                "sparse-checkout".to_string(),
                "set".to_string(),
                "--cone".to_string(),
                "--".to_string(),
            ];
            for p in paths {
                args.push(p.clone());
            }
            let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            return env.exec_git(&args_refs, Some(cwd_path), timeout).await;
        }
    }

    let check = env
        .exec_git(
            &["config", "--get", "core.sparseCheckout"],
            Some(cwd_path),
            timeout,
        )
        .await;
    if check.code == 0
        && check
            .stdout
            .as_deref()
            .unwrap_or("")
            .trim()
            == "true"
    {
        return GitResult {
            code: 1,
            stderr: "sparsePaths removed from config but repository is sparse; re-cloning for full checkout".to_string(),
            error: None,
            stdout: None,
        };
    }
    GitResult {
        code: 0,
        stderr: String::new(),
        error: None,
        stdout: None,
    }
}

/// Safely invoke a progress callback
fn safe_call_progress(on_progress: Option<&MarketplaceProgressCallback>, message: &str) {
    if let Some(cb) = on_progress {
        if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(message))) {
            debug!("Progress callback error: {:?}", e);
        }
    }
}

/// Cache a marketplace from a git repository
async fn cache_marketplace_from_git(
    env: &dyn MarketplaceEnv,
    git_url: &str,
    cache_path: &str,
    git_ref: Option<&str>,
    sparse_paths: Option<&[String]>,
    on_progress: Option<&MarketplaceProgressCallback>,
    options: Option<&GitPullOptions>,
) -> Result<()> {
    let timeout_sec = get_plugin_git_timeout_ms() / 1000;
    safe_call_progress(
        on_progress,
        &format!("Refreshing marketplace cache (timeout: {}s)...", timeout_sec),
    );

    let reconcile_result =
        reconcile_sparse_checkout(env, cache_path, sparse_paths).await;
    if reconcile_result.code == 0 {
        let pull_started = Instant::now();
        let pull_result = git_pull(
            env,
            cache_path,
            git_ref,
            options,
        )
        .await;
        let duration = pull_started.elapsed().as_millis() as f64;
        let status = if pull_result.code == 0 { "success" } else { "failure" };
        let error_class = if pull_result.code != 0 {
            Some(env.classify_fetch_error(&pull_result.stderr))
        } else {
            None
        };
        env.log_plugin_fetch(
            "marketplace_pull",
            git_url,
            status,
            duration,
            error_class.as_deref(),
        );
        if pull_result.code == 0 {
            return Ok(());
        }
        debug!("git pull failed, will re-clone: {}", pull_result.stderr);
    } else {
        debug!(
            "sparse-checkout reconcile requires re-clone: {}",
            reconcile_result.stderr
        );
    }

    // Remove and re-clone
    match env.rm(Path::new(cache_path)).await {
        Ok(()) => {
            debug!(
                "Found stale marketplace directory at {}, cleaning up to allow re-clone",
                cache_path
            );
            safe_call_progress(
                on_progress,
                "Found stale directory, cleaning up and re-cloning...",
            );
        }
        Err(_) => {
            // ENOENT — cachePath didn't exist
        }
    }

    let ref_message = git_ref
        .map(|r| format!(" (ref: {})", r))
        .unwrap_or_default();
    safe_call_progress(
        on_progress,
        &format!(
            "Cloning repository (timeout: {}s): {}{}",
            timeout_sec,
            redact_url_credentials(git_url),
            ref_message
        ),
    );

    let clone_started = Instant::now();
    let result = git_clone(env, git_url, cache_path, git_ref, sparse_paths).await;
    let duration = clone_started.elapsed().as_millis() as f64;
    let status = if result.code == 0 { "success" } else { "failure" };
    let error_class = if result.code != 0 {
        Some(env.classify_fetch_error(&result.stderr))
    } else {
        None
    };
    env.log_plugin_fetch(
        "marketplace_clone",
        git_url,
        status,
        duration,
        error_class.as_deref(),
    );

    if result.code != 0 {
        let _ = env.rm(Path::new(cache_path)).await;
        return Err(anyhow!(
            "Failed to clone marketplace repository: {}",
            result.stderr
        ));
    }
    safe_call_progress(on_progress, "Clone complete, validating marketplace...");
    Ok(())
}

/// Redact URL credentials for safe logging
pub fn redact_url_credentials(url_string: &str) -> String {
    match url::Url::parse(url_string) {
        Ok(mut parsed) => {
            let is_http = parsed.scheme() == "http" || parsed.scheme() == "https";
            if is_http && (!parsed.username().is_empty() || parsed.password().is_some()) {
                let _ = parsed.set_username("***");
                let _ = parsed.set_password(Some("***"));
                return parsed.to_string();
            }
            url_string.to_string()
        }
        Err(_) => url_string.to_string(),
    }
}

fn redact_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    headers
        .keys()
        .map(|k| (k.clone(), "***REDACTED***".to_string()))
        .collect()
}

/// Cache a marketplace from a URL
async fn cache_marketplace_from_url(
    env: &dyn MarketplaceEnv,
    url: &str,
    cache_path: &str,
    custom_headers: Option<&HashMap<String, String>>,
    on_progress: Option<&MarketplaceProgressCallback>,
) -> Result<()> {
    let redacted_url = redact_url_credentials(url);
    safe_call_progress(
        on_progress,
        &format!("Downloading marketplace from {}", redacted_url),
    );
    debug!("Downloading marketplace from URL: {}", redacted_url);

    let mut headers = custom_headers.cloned().unwrap_or_default();
    headers.insert(
        "User-Agent".to_string(),
        "Mossen-Code-Plugin-Manager".to_string(),
    );

    let fetch_started = Instant::now();
    let response = match env.http_get(url, &headers, 10000).await {
        Ok(resp) => resp,
        Err(e) => {
            let duration = fetch_started.elapsed().as_millis() as f64;
            let err_str = e.to_string();
            env.log_plugin_fetch(
                "marketplace_url",
                url,
                "failure",
                duration,
                Some(&env.classify_fetch_error(&err_str)),
            );
            return Err(anyhow!(
                "Failed to download marketplace from {}: {}",
                redacted_url,
                e
            ));
        }
    };

    safe_call_progress(on_progress, "Validating marketplace data");
    // Validate marketplace schema
    let marketplace: PluginMarketplace = serde_json::from_value(response.clone())
        .map_err(|e| {
            let duration = fetch_started.elapsed().as_millis() as f64;
            env.log_plugin_fetch(
                "marketplace_url",
                url,
                "failure",
                duration,
                Some("invalid_schema"),
            );
            anyhow!("Invalid marketplace schema from URL: {}", e)
        })?;

    let duration = fetch_started.elapsed().as_millis() as f64;
    env.log_plugin_fetch("marketplace_url", url, "success", duration, None);

    safe_call_progress(on_progress, "Saving marketplace to cache");
    let cache_dir = Path::new(cache_path).parent().unwrap();
    env.mkdir(cache_dir).await?;

    let json = serde_json::to_string_pretty(&marketplace)?;
    env.write_file_sync(Path::new(cache_path), &json)?;
    Ok(())
}

/// Generate a cache path name for a marketplace source
fn get_cache_path_for_source(source: &MarketplaceSource) -> String {
    match source {
        MarketplaceSource::GitHub { repo, .. } => repo.replace('/', "-"),
        MarketplaceSource::Npm { package, .. } => {
            package.replace('@', "").replace('/', "-")
        }
        MarketplaceSource::File { path, .. } => {
            Path::new(path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        }
        MarketplaceSource::Directory { path, .. } => {
            Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        }
        MarketplaceSource::Url { .. } => format!("temp_{}", chrono::Utc::now().timestamp()),
        MarketplaceSource::Git { .. } => format!("temp_{}", chrono::Utc::now().timestamp()),
        MarketplaceSource::Settings { name, .. } => name.clone(),
        MarketplaceSource::HostPattern { .. } | MarketplaceSource::PathPattern { .. } => {
            format!("pattern_{}", chrono::Utc::now().timestamp())
        }
    }
}

/// Load and cache a marketplace from its source
async fn load_and_cache_marketplace(
    env: &dyn MarketplaceEnv,
    source: &MarketplaceSource,
    on_progress: Option<&MarketplaceProgressCallback>,
) -> Result<LoadedPluginMarketplace> {
    let cache_dir = get_marketplaces_cache_dir(env);
    env.mkdir(&cache_dir).await?;

    let temp_name = get_cache_path_for_source(source);
    let mut temporary_cache_path: PathBuf;
    let marketplace_path: PathBuf;
    let mut cleanup_needed = false;

    match source {
        MarketplaceSource::Url { url, headers, .. } => {
            temporary_cache_path = cache_dir.join(format!("{}.json", temp_name));
            cleanup_needed = true;
            cache_marketplace_from_url(
                env,
                url,
                &temporary_cache_path.to_string_lossy(),
                headers.as_ref(),
                on_progress,
            )
            .await?;
            marketplace_path = temporary_cache_path.clone();
        }
        MarketplaceSource::GitHub { repo, git_ref, sparse_paths, path, .. } => {
            let ssh_url = format!("git@github.com:{}.git", repo);
            let https_url = format!("https://github.com/{}.git", repo);
            temporary_cache_path = cache_dir.join(&temp_name);
            cleanup_needed = true;

            let ssh_configured = is_github_ssh_likely_configured(env).await;
            let mut last_error: Option<anyhow::Error> = None;

            if ssh_configured {
                safe_call_progress(on_progress, &format!("Cloning via SSH: {}", ssh_url));
                match cache_marketplace_from_git(
                    env, &ssh_url, &temporary_cache_path.to_string_lossy(),
                    git_ref.as_deref(), sparse_paths.as_deref(), on_progress, None,
                ).await {
                    Ok(()) => {}
                    Err(e) => {
                        last_error = Some(e);
                        safe_call_progress(on_progress, &format!("SSH clone failed, retrying with HTTPS: {}", https_url));
                        let _ = env.rm(&temporary_cache_path).await;
                        match cache_marketplace_from_git(
                            env, &https_url, &temporary_cache_path.to_string_lossy(),
                            git_ref.as_deref(), sparse_paths.as_deref(), on_progress, None,
                        ).await {
                            Ok(()) => last_error = None,
                            Err(e) => last_error = Some(e),
                        }
                    }
                }
            } else {
                safe_call_progress(on_progress, &format!("SSH not configured, cloning via HTTPS: {}", https_url));
                match cache_marketplace_from_git(
                    env, &https_url, &temporary_cache_path.to_string_lossy(),
                    git_ref.as_deref(), sparse_paths.as_deref(), on_progress, None,
                ).await {
                    Ok(()) => {}
                    Err(e) => {
                        last_error = Some(e);
                        safe_call_progress(on_progress, &format!("HTTPS clone failed, retrying with SSH: {}", ssh_url));
                        let _ = env.rm(&temporary_cache_path).await;
                        match cache_marketplace_from_git(
                            env, &ssh_url, &temporary_cache_path.to_string_lossy(),
                            git_ref.as_deref(), sparse_paths.as_deref(), on_progress, None,
                        ).await {
                            Ok(()) => last_error = None,
                            Err(e) => last_error = Some(e),
                        }
                    }
                }
            }

            if let Some(err) = last_error {
                return Err(err);
            }

            marketplace_path = temporary_cache_path.join(
                path.as_deref().unwrap_or(".mossen-plugin/marketplace.json"),
            );
        }
        MarketplaceSource::Git { url, git_ref, sparse_paths, path, .. } => {
            temporary_cache_path = cache_dir.join(&temp_name);
            cleanup_needed = true;
            cache_marketplace_from_git(
                env, url, &temporary_cache_path.to_string_lossy(),
                git_ref.as_deref(), sparse_paths.as_deref(), on_progress, None,
            ).await?;
            marketplace_path = temporary_cache_path.join(
                path.as_deref().unwrap_or(".mossen-plugin/marketplace.json"),
            );
        }
        MarketplaceSource::Npm { package } => {
            // NPM source: shell out to `npm pack`, which streams a
            // tarball of `<package>` into the working directory, then
            // extract it via `tar -xf`. Inside the unpacked `package/`
            // we expect `.mossen-plugin/marketplace.json` (matching the
            // git path convention).
            //
            // We deliberately use the user's installed `npm` rather than
            // hitting the registry HTTP API ourselves — `npm pack`
            // handles the registry URL, auth, scopes, and the
            // tarball-extraction nuances for us. The trade-off is a
            // hard runtime dependency on `npm` being on PATH; we
            // surface a clear error when it isn't.
            temporary_cache_path = cache_dir.join(&temp_name);
            cleanup_needed = true;
            std::fs::create_dir_all(&temporary_cache_path)?;

            // 1. `npm pack <package>` writes the tarball into cwd and
            //    prints its filename on stdout. `--json` is supported
            //    but harder to parse; the plain-text path is simpler.
            let pack = std::process::Command::new("npm")
                .args(["pack", "--silent", package])
                .current_dir(&temporary_cache_path)
                .output()
                .map_err(|e| {
                    anyhow!(
                        "npm not available on PATH (required for npm marketplace sources): {}",
                        e
                    )
                })?;
            if !pack.status.success() {
                return Err(anyhow!(
                    "npm pack failed for `{}`: {}",
                    package,
                    String::from_utf8_lossy(&pack.stderr).trim()
                ));
            }
            let tarball = String::from_utf8_lossy(&pack.stdout)
                .trim()
                .to_string();
            if tarball.is_empty() {
                return Err(anyhow!(
                    "npm pack returned no tarball filename for `{}`",
                    package
                ));
            }

            // 2. Extract the tarball into the temp dir. npm tarballs
            //    always contain a top-level `package/` directory.
            let untar = std::process::Command::new("tar")
                .args(["-xf", &tarball])
                .current_dir(&temporary_cache_path)
                .status()
                .map_err(|e| anyhow!("tar not available on PATH: {}", e))?;
            if !untar.success() {
                return Err(anyhow!(
                    "tar -xf failed on `{}` from npm pack",
                    tarball
                ));
            }

            // 3. Resolve marketplace.json under the unpacked `package/`
            //    dir. Allow either `.mossen-plugin/marketplace.json`
            //    (preferred — matches git source convention) or a
            //    top-level `marketplace.json` (for minimal packages).
            let pkg_dir = temporary_cache_path.join("package");
            let cand_a = pkg_dir.join(".mossen-plugin").join("marketplace.json");
            let cand_b = pkg_dir.join("marketplace.json");
            marketplace_path = if cand_a.exists() {
                cand_a
            } else if cand_b.exists() {
                cand_b
            } else {
                return Err(anyhow!(
                    "npm package `{}` did not contain marketplace.json (checked .mossen-plugin/marketplace.json and ./marketplace.json under package/)",
                    package
                ));
            };

            // Clean up the tarball — leaving it would let the cache
            // path balloon over time. The extracted `package/` stays
            // because marketplace_path points into it.
            let _ = std::fs::remove_file(temporary_cache_path.join(&tarball));
        }
        MarketplaceSource::File { path, .. } => {
            let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
            marketplace_path = abs_path.clone();
            temporary_cache_path = abs_path
                .parent()
                .and_then(|p| p.parent())
                .unwrap_or(&abs_path)
                .to_path_buf();
            cleanup_needed = false;
        }
        MarketplaceSource::Directory { path, .. } => {
            let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
            marketplace_path = abs_path.join(".mossen-plugin").join("marketplace.json");
            temporary_cache_path = abs_path;
            cleanup_needed = false;
        }
        MarketplaceSource::Settings { name, owner, plugins, .. } => {
            temporary_cache_path = cache_dir.join(name);
            marketplace_path = temporary_cache_path.join(".mossen-plugin").join("marketplace.json");
            cleanup_needed = false;
            let mp_dir = temporary_cache_path.join(".mossen-plugin");
            env.mkdir(&mp_dir).await?;
            let owner_val = owner.as_ref()
                .map(|o| serde_json::to_value(o).unwrap_or_default())
                .unwrap_or_else(|| serde_json::json!({"name": "settings"}));
            let content = serde_json::json!({
                "name": name,
                "owner": owner_val,
                "plugins": plugins,
            });
            env.write_file(&marketplace_path, &serde_json::to_string_pretty(&content)?).await?;
        }
        MarketplaceSource::HostPattern { .. } | MarketplaceSource::PathPattern { .. } => {
            return Err(anyhow!("Cannot load marketplace from pattern source"));
        }
    }

    // Load and validate the marketplace
    debug!("Reading marketplace from {:?}", marketplace_path);
    let marketplace = match read_cached_marketplace(env, &marketplace_path).await {
        Ok(m) => m,
        Err(e) => {
            if cleanup_needed && !env.is_local_marketplace_source(source) {
                let _ = env.rm(&temporary_cache_path).await;
            }
            return Err(anyhow!(
                "Failed to parse marketplace file at {:?}: {}",
                marketplace_path,
                e
            ));
        }
    };

    // Rename cache path to marketplace's actual name
    let final_cache_path = cache_dir.join(&marketplace.name);
    let resolved_final = std::fs::canonicalize(&final_cache_path)
        .unwrap_or_else(|_| final_cache_path.clone());
    let resolved_cache_dir = std::fs::canonicalize(&cache_dir)
        .unwrap_or_else(|_| cache_dir.clone());

    if !resolved_final
        .to_string_lossy()
        .starts_with(&format!("{}/", resolved_cache_dir.to_string_lossy()))
    {
        if cleanup_needed {
            let _ = env.rm(&temporary_cache_path).await;
        }
        return Err(anyhow!(
            "Marketplace name '{}' resolves to a path outside the cache directory",
            marketplace.name
        ));
    }

    if temporary_cache_path != final_cache_path && !env.is_local_marketplace_source(source) {
        let _ = env.rm(&final_cache_path).await;
        if let Err(e) = env.rename(&temporary_cache_path, &final_cache_path).await {
            return Err(anyhow!("Failed to finalize marketplace cache: {}", e));
        }
        temporary_cache_path = final_cache_path;
    }

    Ok(LoadedPluginMarketplace {
        marketplace,
        cache_path: temporary_cache_path.to_string_lossy().to_string(),
    })
}

/// Add a marketplace source to the known marketplaces
pub async fn add_marketplace_source(
    env: &dyn MarketplaceEnv,
    source: MarketplaceSource,
    on_progress: Option<&MarketplaceProgressCallback>,
) -> Result<AddMarketplaceResult> {
    let resolved_source = source.clone(); // Caller should resolve paths beforehand

    // Check policy
    if !env.is_source_allowed_by_policy(&resolved_source) {
        if env.is_source_in_blocklist(&resolved_source) {
            return Err(anyhow!(
                "Marketplace source '{}' is blocked by enterprise policy.",
                env.format_source_for_display(&resolved_source)
            ));
        }
        return Err(anyhow!(
            "Marketplace source '{}' is blocked by enterprise policy.",
            env.format_source_for_display(&resolved_source)
        ));
    }

    // Source-idempotency check
    let existing_config = load_known_marketplaces_config(env).await?;
    for (existing_name, existing_entry) in &existing_config {
        if existing_entry.source == resolved_source {
            debug!(
                "Source already materialized as '{}', skipping clone",
                existing_name
            );
            return Ok(AddMarketplaceResult {
                name: existing_name.clone(),
                already_materialized: true,
                resolved_source,
            });
        }
    }

    // Load and cache the marketplace
    let LoadedPluginMarketplace {
        marketplace,
        cache_path,
    } = load_and_cache_marketplace(env, &resolved_source, on_progress).await?;

    // Validate official name
    if let Some(err) = env.validate_official_name_source(&marketplace.name, &resolved_source) {
        return Err(anyhow!("{}", err));
    }

    // Check name collision with seed
    let mut config = load_known_marketplaces_config(env).await?;
    if let Some(old_entry) = config.get(&marketplace.name) {
        if let Some(seed_dir) = seed_dir_for(env, &old_entry.install_location) {
            return Err(anyhow!(
                "Marketplace '{}' is seed-managed ({:?}). To use a different source, ask your admin to update the seed.",
                marketplace.name,
                seed_dir
            ));
        }
    }

    // Update config
    config.insert(
        marketplace.name.clone(),
        KnownMarketplace {
            source: resolved_source.clone(),
            install_location: cache_path,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
            auto_update: None,
        },
    );
    save_known_marketplaces_config(env, &config).await?;

    debug!("Added marketplace source: {}", marketplace.name);

    Ok(AddMarketplaceResult {
        name: marketplace.name,
        already_materialized: false,
        resolved_source,
    })
}

/// Result of adding a marketplace source
pub struct AddMarketplaceResult {
    pub name: String,
    pub already_materialized: bool,
    pub resolved_source: MarketplaceSource,
}

/// Remove a marketplace source from known marketplaces
pub async fn remove_marketplace_source(env: &dyn MarketplaceEnv, name: &str) -> Result<()> {
    let mut config = load_known_marketplaces_config(env).await?;
    let entry = config
        .get(name)
        .ok_or_else(|| anyhow!("Marketplace '{}' not found", name))?
        .clone();

    // Check seed-managed
    if let Some(seed_dir) = seed_dir_for(env, &entry.install_location) {
        return Err(anyhow!(
            "Marketplace '{}' is registered from the read-only seed directory ({:?}) and will be re-registered on next startup.",
            name, seed_dir
        ));
    }

    config.remove(name);
    save_known_marketplaces_config(env, &config).await?;

    // Clean up cached files
    let cache_dir = get_marketplaces_cache_dir(env);
    let cache_path = cache_dir.join(name);
    let _ = env.rm(&cache_path).await;
    let json_cache_path = cache_dir.join(format!("{}.json", name));
    let _ = env.rm(&json_cache_path).await;

    // Clean up settings
    let editable_sources = ["userSettings", "projectSettings", "localSettings"];
    for source in &editable_sources {
        if let Some(settings) = env.get_settings_for_source(source) {
            if settings.contains_key(name) {
                let updates = serde_json::json!({
                    "extraKnownMarketplaces": { name: null },
                });
                let _ = env.update_settings_for_source(source, updates);
            }
        }
    }

    // Remove plugins from installed_plugins.json
    let (orphaned_paths, removed_plugin_ids) =
        env.remove_all_plugins_for_marketplace(name);
    for install_path in &orphaned_paths {
        let _ = env.mark_plugin_version_orphaned(install_path).await;
    }
    for plugin_id in &removed_plugin_ids {
        env.delete_plugin_options(plugin_id);
        let _ = env.delete_plugin_data_dir(plugin_id).await;
    }

    debug!("Removed marketplace source: {}", name);
    Ok(())
}

/// Read a cached marketplace from disk without updating it
async fn read_cached_marketplace(
    env: &dyn MarketplaceEnv,
    install_location: &Path,
) -> Result<PluginMarketplace> {
    // Try nested path first
    let nested_path = install_location.join(".mossen-plugin").join("marketplace.json");
    if let Ok(content) = env.read_file(&nested_path).await {
        let marketplace: PluginMarketplace = serde_json::from_str(&content)?;
        return Ok(marketplace);
    }
    // Fallback to install_location itself
    let content = env.read_file(install_location).await?;
    let marketplace: PluginMarketplace = serde_json::from_str(&content)?;
    Ok(marketplace)
}

/// Get a specific marketplace by name from cache only (no network).
pub async fn get_marketplace_cache_only(
    env: &dyn MarketplaceEnv,
    name: &str,
) -> Option<PluginMarketplace> {
    let config_file = get_known_marketplaces_file(env);
    let content = env.read_file(&config_file).await.ok()?;
    let config: KnownMarketplacesFile = serde_json::from_str(&content).ok()?;
    let entry = config.get(name)?;
    read_cached_marketplace(env, Path::new(&entry.install_location))
        .await
        .ok()
}

/// Get a specific marketplace by name (cache-first, then fetch)
pub async fn get_marketplace(
    env: &dyn MarketplaceEnv,
    name: &str,
) -> Result<PluginMarketplace> {
    // Check memory cache
    {
        let cache = MARKETPLACE_CACHE.lock().unwrap();
        if let Some(m) = cache.get(name) {
            return Ok(m.clone());
        }
    }

    let config = load_known_marketplaces_config(env).await?;
    let entry = config
        .get(name)
        .ok_or_else(|| {
            anyhow!(
                "Marketplace '{}' not found in configuration. Available marketplaces: {}",
                name,
                config.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?;

    // Try to read from disk cache
    match read_cached_marketplace(env, Path::new(&entry.install_location)).await {
        Ok(marketplace) => {
            MARKETPLACE_CACHE
                .lock()
                .unwrap()
                .insert(name.to_string(), marketplace.clone());
            return Ok(marketplace);
        }
        Err(e) => {
            debug!(
                "Cache corrupted or missing for marketplace {}, re-fetching: {}",
                name, e
            );
        }
    }

    // Fetch from source
    let LoadedPluginMarketplace { marketplace, .. } =
        load_and_cache_marketplace(env, &entry.source, None).await?;

    MARKETPLACE_CACHE
        .lock()
        .unwrap()
        .insert(name.to_string(), marketplace.clone());
    Ok(marketplace)
}

/// Get plugin by ID from cache only (no network calls).
pub async fn get_plugin_by_id_cache_only(
    env: &dyn MarketplaceEnv,
    plugin_id: &str,
) -> Option<PluginLookupResult> {
    let (plugin_name, marketplace_name) = env.parse_plugin_identifier(plugin_id);
    let (plugin_name, marketplace_name) = (plugin_name?, marketplace_name?);

    let config_file = get_known_marketplaces_file(env);
    let content = env.read_file(&config_file).await.ok()?;
    let config: KnownMarketplacesFile = serde_json::from_str(&content).ok()?;
    let marketplace_config = config.get(&marketplace_name)?;

    let marketplace = get_marketplace_cache_only(env, &marketplace_name).await?;
    let plugin = marketplace
        .plugins
        .iter()
        .find(|p| p.name == plugin_name)?;

    Some(PluginLookupResult {
        entry: plugin.clone(),
        marketplace_install_location: marketplace_config.install_location.clone(),
    })
}

/// Plugin lookup result
#[derive(Debug, Clone)]
pub struct PluginLookupResult {
    pub entry: PluginMarketplaceEntry,
    pub marketplace_install_location: String,
}

/// Get plugin by ID (cache-first, then fetch)
pub async fn get_plugin_by_id(
    env: &dyn MarketplaceEnv,
    plugin_id: &str,
) -> Option<PluginLookupResult> {
    // Try cache first
    if let Some(cached) = get_plugin_by_id_cache_only(env, plugin_id).await {
        return Some(cached);
    }

    let (plugin_name, marketplace_name) = env.parse_plugin_identifier(plugin_id);
    let (plugin_name, marketplace_name) = (plugin_name?, marketplace_name?);

    let config = load_known_marketplaces_config(env).await.ok()?;
    let marketplace_config = config.get(&marketplace_name)?;

    let marketplace = get_marketplace(env, &marketplace_name).await.ok()?;
    let plugin = marketplace
        .plugins
        .iter()
        .find(|p| p.name == plugin_name)?;

    Some(PluginLookupResult {
        entry: plugin.clone(),
        marketplace_install_location: marketplace_config.install_location.clone(),
    })
}

/// Refresh all marketplace caches
pub async fn refresh_all_marketplaces(env: &dyn MarketplaceEnv) -> Result<()> {
    let mut config = load_known_marketplaces_config(env).await?;

    for (name, entry) in config.clone().iter() {
        if seed_dir_for(env, &entry.install_location).is_some() {
            debug!("Skipping seed-managed marketplace '{}' in bulk refresh", name);
            continue;
        }
        if matches!(entry.source, MarketplaceSource::Settings { .. }) {
            continue;
        }

        let official_name = env.get_official_marketplace_name();
        if name == official_name {
            let cache_dir = get_marketplaces_cache_dir(env);
            if let Some(_sha) = env
                .fetch_official_marketplace_from_gcs(
                    &entry.install_location,
                    &cache_dir.to_string_lossy(),
                )
                .await
            {
                if let Some(e) = config.get_mut(name) {
                    e.last_updated = Some(chrono::Utc::now().to_rfc3339());
                }
                continue;
            }
            if !env.get_feature_value_cached("tengu_plugin_official_mkt_git_fallback", true) {
                debug!("Skipping official marketplace bulk refresh: GCS failed, git fallback disabled");
                continue;
            }
        }

        match load_and_cache_marketplace(env, &entry.source, None).await {
            Ok(loaded) => {
                if let Some(e) = config.get_mut(name) {
                    e.last_updated = Some(chrono::Utc::now().to_rfc3339());
                    e.install_location = loaded.cache_path;
                }
            }
            Err(e) => {
                debug!("Failed to refresh marketplace {}: {}", name, e);
            }
        }
    }

    save_known_marketplaces_config(env, &config).await?;
    Ok(())
}

/// Refresh a single marketplace cache
pub async fn refresh_marketplace(
    env: &dyn MarketplaceEnv,
    name: &str,
    on_progress: Option<&MarketplaceProgressCallback>,
    options: Option<&GitPullOptions>,
) -> Result<()> {
    let mut config = load_known_marketplaces_config(env).await?;
    let entry = config
        .get(name)
        .ok_or_else(|| {
            anyhow!(
                "Marketplace '{}' not found. Available marketplaces: {}",
                name,
                config.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?
        .clone();

    // Clear memoization cache
    MARKETPLACE_CACHE.lock().unwrap().remove(name);

    if matches!(entry.source, MarketplaceSource::Settings { .. }) {
        debug!(
            "Skipping refresh for settings-sourced marketplace '{}' — no upstream",
            name
        );
        return Ok(());
    }

    // Seed check
    if let Some(seed_dir) = seed_dir_for(env, &entry.install_location) {
        return Err(anyhow!(
            "Marketplace '{}' is seed-managed ({:?}) and its content is controlled by the seed image.",
            name, seed_dir
        ));
    }

    // Official marketplace GCS check
    let official_name = env.get_official_marketplace_name();
    if name == official_name {
        let cache_dir = get_marketplaces_cache_dir(env);
        if let Some(_sha) = env
            .fetch_official_marketplace_from_gcs(
                &entry.install_location,
                &cache_dir.to_string_lossy(),
            )
            .await
        {
            if let Some(e) = config.get_mut(name) {
                e.last_updated = Some(chrono::Utc::now().to_rfc3339());
            }
            save_known_marketplaces_config(env, &config).await?;
            return Ok(());
        }
        if !env.get_feature_value_cached("tengu_plugin_official_mkt_git_fallback", true) {
            return Err(anyhow!(
                "Official marketplace GCS fetch failed and git fallback is disabled"
            ));
        }
        debug!("Official marketplace GCS failed; falling back to git");
    }

    // Update based on source type
    match &entry.source {
        MarketplaceSource::GitHub { repo, git_ref, sparse_paths, .. }
        | MarketplaceSource::Git { url: repo, git_ref, sparse_paths, .. } => {
            let install_location = &entry.install_location;

            if let MarketplaceSource::GitHub { repo, .. } = &entry.source {
                let ssh_url = format!("git@github.com:{}.git", repo);
                let https_url = format!("https://github.com/{}.git", repo);

                if env.is_env_truthy("MOSSEN_CODE_REMOTE") {
                    cache_marketplace_from_git(
                        env, &https_url, install_location,
                        git_ref.as_deref(), sparse_paths.as_deref(),
                        on_progress, options,
                    ).await?;
                } else {
                    let ssh_configured = is_github_ssh_likely_configured(env).await;
                    let primary_url = if ssh_configured { &ssh_url } else { &https_url };
                    let fallback_url = if ssh_configured { &https_url } else { &ssh_url };

                    if cache_marketplace_from_git(
                        env, primary_url, install_location,
                        git_ref.as_deref(), sparse_paths.as_deref(),
                        on_progress, options,
                    ).await.is_err() {
                        debug!("Marketplace refresh failed with primary, falling back");
                        cache_marketplace_from_git(
                            env, fallback_url, install_location,
                            git_ref.as_deref(), sparse_paths.as_deref(),
                            on_progress, options,
                        ).await?;
                    }
                }
            } else if let MarketplaceSource::Git { url, .. } = &entry.source {
                cache_marketplace_from_git(
                    env, url, &entry.install_location,
                    git_ref.as_deref(), sparse_paths.as_deref(),
                    on_progress, options,
                ).await?;
            }

            // Validate marketplace still exists after update
            if read_cached_marketplace(env, Path::new(&entry.install_location))
                .await
                .is_err()
            {
                return Err(anyhow!(
                    "The marketplace.json file is no longer present in this repository."
                ));
            }
        }
        MarketplaceSource::Url { url, headers, .. } => {
            cache_marketplace_from_url(
                env, url, &entry.install_location,
                headers.as_ref(), on_progress,
            ).await?;
        }
        MarketplaceSource::File { .. } | MarketplaceSource::Directory { .. } => {
            safe_call_progress(on_progress, "Validating local marketplace");
            read_cached_marketplace(env, Path::new(&entry.install_location)).await?;
        }
        _ => {
            return Err(anyhow!("Unsupported marketplace source type for refresh"));
        }
    }

    // Update lastUpdated
    if let Some(e) = config.get_mut(name) {
        e.last_updated = Some(chrono::Utc::now().to_rfc3339());
    }
    save_known_marketplaces_config(env, &config).await?;
    debug!("Successfully refreshed marketplace: {}", name);
    Ok(())
}

/// Set the autoUpdate flag for a marketplace
pub async fn set_marketplace_auto_update(
    env: &dyn MarketplaceEnv,
    name: &str,
    auto_update: bool,
) -> Result<()> {
    let mut config = load_known_marketplaces_config(env).await?;
    let entry = config
        .get(name)
        .ok_or_else(|| {
            anyhow!(
                "Marketplace '{}' not found. Available: {}",
                name,
                config.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?
        .clone();

    if let Some(seed_dir) = seed_dir_for(env, &entry.install_location) {
        return Err(anyhow!(
            "Marketplace '{}' is seed-managed ({:?}) and auto-update is always disabled.",
            name, seed_dir
        ));
    }

    if entry.auto_update == Some(auto_update) {
        return Ok(());
    }

    if let Some(e) = config.get_mut(name) {
        e.auto_update = Some(auto_update);
    }
    save_known_marketplaces_config(env, &config).await?;

    // Update settings intent if declared
    if let Some(declaring_source) = get_marketplace_declaring_source(env, name) {
        if let Some(settings) = env.get_settings_for_source(declaring_source) {
            if settings.contains_key(name) {
                let declared = DeclaredMarketplace {
                    source: entry.source.clone(),
                    install_location: None,
                    auto_update: Some(auto_update),
                    source_is_fallback: None,
                };
                save_marketplace_to_settings(env, name, &declared, declaring_source);
            }
        }
    }

    debug!("Set autoUpdate={} for marketplace: {}", auto_update, name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_url_credentials() {
        assert_eq!(
            redact_url_credentials("https://user:token@github.com/repo"),
            "https://***:***@github.com/repo"
        );
        assert_eq!(
            redact_url_credentials("https://github.com/repo"),
            "https://github.com/repo"
        );
        assert_eq!(
            redact_url_credentials("git@github.com:owner/repo.git"),
            "git@github.com:owner/repo.git"
        );
    }
}

/// 对应 TS `KnownMarketplacesConfig`：当前生效的已知 marketplace 配置。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct KnownMarketplacesConfig {
    pub names: Vec<String>,
    pub sources: std::collections::HashMap<String, serde_json::Value>,
}

/// 测试用入口（对应 TS `_test`）。
#[doc(hidden)]
pub fn _test_known_marketplaces() -> KnownMarketplacesConfig {
    KnownMarketplacesConfig::default()
}

/// 对应 TS `export const _test = {...}`：仅测试可见的命名空间集合。
#[doc(hidden)]
#[allow(non_upper_case_globals)]
pub const _test: &str = "_test";
