//! Plugin Loader Module
//!
//! This module is responsible for discovering, loading, and validating Mossen plugins
//! from various sources including marketplaces and git repositories.
//!
//! Plugin Discovery Sources (in order of precedence):
//! 1. Marketplace-based plugins (plugin@marketplace format in settings)
//! 2. Session-only plugins (from --plugin-dir CLI flag or SDK plugins option)

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::schemas::{
    CommandMetadata, HooksSettings, PluginManifest, PluginMarketplaceEntry,
    PluginSource, StructuredPluginSource,
};

/// Loaded plugin representation
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub name: String,
    pub manifest: PluginManifest,
    pub path: PathBuf,
    pub source: String,
    pub repository: String,
    pub enabled: bool,
    pub commands_path: Option<PathBuf>,
    pub commands_paths: Option<Vec<PathBuf>>,
    pub commands_metadata: Option<HashMap<String, CommandMetadata>>,
    pub agents_path: Option<PathBuf>,
    pub agents_paths: Option<Vec<PathBuf>>,
    pub skills_path: Option<PathBuf>,
    pub skills_paths: Option<Vec<PathBuf>>,
    pub output_styles_path: Option<PathBuf>,
    pub output_styles_paths: Option<Vec<PathBuf>>,
    pub hooks_config: Option<HooksSettings>,
    pub settings: Option<HashMap<String, serde_json::Value>>,
    pub sha: Option<String>,
}

/// Plugin error types
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum PluginError {
    #[serde(rename = "path-not-found")]
    PathNotFound {
        source: String,
        plugin: Option<String>,
        path: String,
        component: String,
    },
    #[serde(rename = "plugin-not-found")]
    PluginNotFound {
        source: String,
        plugin_id: String,
        marketplace: String,
    },
    #[serde(rename = "plugin-cache-miss")]
    PluginCacheMiss {
        source: String,
        plugin: String,
        install_path: String,
    },
    #[serde(rename = "hook-load-failed")]
    HookLoadFailed {
        source: String,
        plugin: String,
        hook_path: String,
        reason: String,
    },
    #[serde(rename = "marketplace-blocked-by-policy")]
    MarketplaceBlockedByPolicy {
        source: String,
        plugin: Option<String>,
        marketplace: String,
        blocked_by_blocklist: bool,
        allowed_sources: Vec<String>,
    },
    #[serde(rename = "generic-error")]
    GenericError {
        source: String,
        plugin: Option<String>,
        error: String,
    },
}

/// Plugin component type
#[derive(Debug, Clone, Copy)]
pub enum PluginComponent {
    Commands,
    Agents,
    Skills,
    OutputStyles,
    Hooks,
}

impl PluginComponent {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Commands => "commands",
            Self::Agents => "agents",
            Self::Skills => "skills",
            Self::OutputStyles => "output-styles",
            Self::Hooks => "hooks",
        }
    }
}

/// Plugin load result
#[derive(Debug, Clone)]
pub struct PluginLoadResult {
    pub enabled: Vec<LoadedPlugin>,
    pub disabled: Vec<LoadedPlugin>,
    pub errors: Vec<PluginError>,
}

/// Trait for plugin loader external dependencies
#[async_trait::async_trait]
pub trait PluginLoaderEnv: Send + Sync {
    fn get_plugins_directory(&self) -> PathBuf;
    fn get_plugin_seed_dirs(&self) -> Vec<PathBuf>;
    fn get_builtin_marketplace_name(&self) -> &str;

    // Plugin identifier
    fn parse_plugin_identifier(&self, plugin_id: &str) -> (Option<String>, Option<String>);

    // Settings
    fn get_settings_enabled_plugins(&self) -> HashMap<String, serde_json::Value>;
    fn get_add_dir_enabled_plugins(&self) -> HashMap<String, serde_json::Value>;
    fn get_inline_plugins(&self) -> Vec<String>;
    fn get_managed_plugin_names(&self) -> Option<HashSet<String>>;
    fn is_env_truthy(&self, key: &str) -> bool;

    // Marketplace
    async fn get_marketplace_cache_only(&self, name: &str) -> Option<super::schemas::PluginMarketplace>;
    async fn get_plugin_by_id_cache_only(
        &self,
        plugin_id: &str,
    ) -> Option<(PluginMarketplaceEntry, String)>;
    async fn load_known_marketplaces_config_safe(
        &self,
    ) -> HashMap<String, super::schemas::KnownMarketplace>;

    // Policy
    fn is_source_allowed_by_policy(&self, source: &super::schemas::MarketplaceSource) -> bool;
    fn is_source_in_blocklist(&self, source: &super::schemas::MarketplaceSource) -> bool;
    fn format_source_for_display(&self, source: &super::schemas::MarketplaceSource) -> String;
    fn get_strict_known_marketplaces(&self) -> Option<Vec<super::schemas::MarketplaceSource>>;
    fn get_blocked_marketplaces(&self) -> Option<Vec<super::schemas::MarketplaceSource>>;

    // Installed plugins
    fn get_in_memory_installed_plugins(
        &self,
    ) -> super::schemas::InstalledPluginsFileV2;

    // Filesystem
    async fn path_exists(&self, path: &Path) -> bool;
    async fn read_file(&self, path: &Path) -> Result<String>;
    async fn read_dir(&self, path: &Path) -> Result<Vec<String>>;
    async fn mkdir(&self, path: &Path) -> Result<()>;
    async fn copy_dir(&self, src: &Path, dest: &Path) -> Result<()>;
    async fn copy_file(&self, src: &Path, dest: &Path) -> Result<()>;
    async fn rename(&self, src: &Path, dest: &Path) -> Result<()>;
    async fn rm(&self, path: &Path) -> Result<()>;
    async fn stat_is_dir(&self, path: &Path) -> Result<bool>;

    // Git
    fn git_exe(&self) -> String;
    async fn exec_git(&self, args: &[&str], cwd: Option<&Path>) -> super::marketplace_manager::GitResult;
    async fn check_git_available(&self) -> bool;

    // Versioning
    async fn calculate_plugin_version(
        &self,
        plugin_id: &str,
        source: &PluginSource,
        manifest: Option<&PluginManifest>,
        plugin_dir: Option<&Path>,
        fallback_version: Option<&str>,
        git_commit_sha: Option<&str>,
    ) -> String;

    // Zip cache
    fn is_plugin_zip_cache_enabled(&self) -> bool;
    async fn get_session_plugin_cache_path(&self) -> Result<PathBuf>;
    async fn extract_zip_to_directory(&self, zip_path: &Path, dest: &Path) -> Result<()>;
    async fn convert_directory_to_zip_in_place(&self, dir: &Path, zip_path: &Path) -> Result<()>;

    // Dependency resolver
    fn verify_and_demote(&self, plugins: &[LoadedPlugin]) -> (HashSet<String>, Vec<PluginError>);

    // Builtin plugins
    fn get_builtin_plugins(&self) -> PluginLoadResult;

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

    // Plugin settings cache
    fn get_plugin_settings_base(&self) -> Option<HashMap<String, serde_json::Value>>;
    fn set_plugin_settings_base(&self, settings: Option<HashMap<String, serde_json::Value>>);
    fn clear_plugin_settings_base(&self);
    fn reset_settings_cache(&self);

    // Path validation
    fn validate_path_within_base(&self, base: &Path, rel_path: &str) -> Result<PathBuf>;
}

/// Get the path where plugin cache is stored
pub fn get_plugin_cache_path(env: &dyn PluginLoaderEnv) -> PathBuf {
    env.get_plugins_directory().join("cache")
}

/// Compute the versioned cache path under a specific base plugins directory.
pub fn get_versioned_cache_path_in(
    base_dir: &Path,
    plugin_id: &str,
    version: &str,
    env: &dyn PluginLoaderEnv,
) -> PathBuf {
    let (plugin_name, marketplace) = env.parse_plugin_identifier(plugin_id);
    let sanitized_marketplace = sanitize_path_component(
        &marketplace.unwrap_or_else(|| "unknown".to_string()),
    );
    let sanitized_plugin = sanitize_path_component(
        &plugin_name.unwrap_or_else(|| plugin_id.to_string()),
    );
    let sanitized_version = sanitize_version(version);
    base_dir
        .join("cache")
        .join(&sanitized_marketplace)
        .join(&sanitized_plugin)
        .join(&sanitized_version)
}

/// Get versioned cache path for a plugin under the primary plugins directory.
pub fn get_versioned_cache_path(
    plugin_id: &str,
    version: &str,
    env: &dyn PluginLoaderEnv,
) -> PathBuf {
    get_versioned_cache_path_in(&env.get_plugins_directory(), plugin_id, version, env)
}

/// Get versioned ZIP cache path for a plugin.
pub fn get_versioned_zip_cache_path(
    plugin_id: &str,
    version: &str,
    env: &dyn PluginLoaderEnv,
) -> PathBuf {
    let mut path = get_versioned_cache_path(plugin_id, version, env);
    let new_name = format!(
        "{}.zip",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    path.set_file_name(new_name);
    path
}

/// Sanitize a path component (remove non-alphanumeric chars except - and _)
fn sanitize_path_component(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Sanitize a version string for use in path
fn sanitize_version(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Probe seed directories for a populated cache at this plugin version.
async fn probe_seed_cache(
    env: &dyn PluginLoaderEnv,
    plugin_id: &str,
    version: &str,
) -> Option<PathBuf> {
    for seed_dir in env.get_plugin_seed_dirs() {
        let seed_path = get_versioned_cache_path_in(&seed_dir, plugin_id, version, env);
        if let Ok(entries) = env.read_dir(&seed_path).await {
            if !entries.is_empty() {
                return Some(seed_path);
            }
        }
    }
    None
}

/// Probe seed cache for any version when version is 'unknown'.
pub async fn probe_seed_cache_any_version(
    env: &dyn PluginLoaderEnv,
    plugin_id: &str,
) -> Option<PathBuf> {
    for seed_dir in env.get_plugin_seed_dirs() {
        let dummy_path = get_versioned_cache_path_in(&seed_dir, plugin_id, "_", env);
        let plugin_dir = dummy_path.parent()?;
        if let Ok(versions) = env.read_dir(plugin_dir).await {
            if versions.len() != 1 {
                continue;
            }
            let version_dir = plugin_dir.join(&versions[0]);
            if let Ok(entries) = env.read_dir(&version_dir).await {
                if !entries.is_empty() {
                    return Some(version_dir);
                }
            }
        }
    }
    None
}

/// Get legacy (non-versioned) cache path for a plugin.
pub fn get_legacy_cache_path(env: &dyn PluginLoaderEnv, plugin_name: &str) -> PathBuf {
    let cache_path = get_plugin_cache_path(env);
    cache_path.join(sanitize_path_component(plugin_name))
}

/// Resolve plugin path with fallback to legacy location.
pub async fn resolve_plugin_path(
    env: &dyn PluginLoaderEnv,
    plugin_id: &str,
    version: Option<&str>,
) -> PathBuf {
    if let Some(v) = version {
        let versioned_path = get_versioned_cache_path(plugin_id, v, env);
        if env.path_exists(&versioned_path).await {
            return versioned_path;
        }
    }

    let (plugin_name, _) = env.parse_plugin_identifier(plugin_id);
    let name = plugin_name.unwrap_or_else(|| plugin_id.to_string());
    let legacy_path = get_legacy_cache_path(env, &name);
    if env.path_exists(&legacy_path).await {
        return legacy_path;
    }

    if let Some(v) = version {
        get_versioned_cache_path(plugin_id, v, env)
    } else {
        legacy_path
    }
}

/// Validate a git URL
fn validate_git_url(url: &str) -> Result<&str> {
    if let Ok(parsed) = url::Url::parse(url) {
        let protocol = parsed.scheme();
        if !["https", "http", "file"].contains(&protocol) {
            let ssh_re = regex::Regex::new(r"^git@[a-zA-Z0-9.\-]+:").unwrap();
            if !ssh_re.is_match(url) {
                return Err(anyhow!(
                    "Invalid git URL protocol: {}. Only HTTPS, HTTP, file:// and SSH (git@) URLs are supported.",
                    protocol
                ));
            }
        }
        return Ok(url);
    }

    let ssh_re = regex::Regex::new(r"^git@[a-zA-Z0-9.\-]+:").unwrap();
    if ssh_re.is_match(url) {
        return Ok(url);
    }
    Err(anyhow!("Invalid git URL: {}", url))
}

/// Install a plugin from npm using a global cache
pub async fn install_from_npm(
    env: &dyn PluginLoaderEnv,
    package_name: &str,
    target_path: &Path,
    registry: Option<&str>,
    version: Option<&str>,
) -> Result<()> {
    let npm_cache_path = env.get_plugins_directory().join("npm-cache");
    env.mkdir(&npm_cache_path).await?;

    let package_spec = if let Some(v) = version {
        format!("{}@{}", package_name, v)
    } else {
        package_name.to_string()
    };

    let package_path = npm_cache_path.join("node_modules").join(package_name);
    let needs_install = !env.path_exists(&package_path).await;

    if needs_install {
        debug!("Installing npm package {} to cache", package_spec);
        let mut args = vec!["install", &package_spec, "--prefix"];
        let npm_cache_str = npm_cache_path.to_string_lossy().to_string();
        args.push(&npm_cache_str);
        if let Some(reg) = registry {
            args.push("--registry");
            args.push(reg);
        }
        let result = env.exec_git(&args, None).await;
        if result.code != 0 {
            return Err(anyhow!("Failed to install npm package: {}", result.stderr));
        }
    }

    env.copy_dir(&package_path, target_path).await?;
    debug!("Copied npm package {} from cache to {:?}", package_name, target_path);
    Ok(())
}

/// Clone a git repository for plugin installation
pub async fn git_clone_plugin(
    env: &dyn PluginLoaderEnv,
    git_url: &str,
    target_path: &Path,
    git_ref: Option<&str>,
    sha: Option<&str>,
) -> Result<()> {
    let mut args: Vec<String> = vec![
        "clone".to_string(),
        "--depth".to_string(),
        "1".to_string(),
        "--recurse-submodules".to_string(),
        "--shallow-submodules".to_string(),
    ];

    if let Some(r) = git_ref {
        args.push("--branch".to_string());
        args.push(r.to_string());
    }

    if sha.is_some() {
        args.push("--no-checkout".to_string());
    }

    args.push(git_url.to_string());
    args.push(target_path.to_string_lossy().to_string());

    let clone_started = Instant::now();
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let clone_result = env.exec_git(&args_refs, None).await;

    if clone_result.code != 0 {
        let duration = clone_started.elapsed().as_millis() as f64;
        env.log_plugin_fetch(
            "plugin_clone",
            git_url,
            "failure",
            duration,
            Some(&env.classify_fetch_error(&clone_result.stderr)),
        );
        return Err(anyhow!(
            "Failed to clone repository: {}",
            clone_result.stderr
        ));
    }

    // If sha is specified, fetch and checkout that specific commit
    if let Some(commit_sha) = sha {
        let fetch_result = env
            .exec_git(
                &["fetch", "--depth", "1", "origin", commit_sha],
                Some(target_path),
            )
            .await;

        if fetch_result.code != 0 {
            debug!("Shallow fetch of SHA {} failed, falling back to unshallow fetch", commit_sha);
            let unshallow_result = env
                .exec_git(&["fetch", "--unshallow"], Some(target_path))
                .await;
            if unshallow_result.code != 0 {
                let duration = clone_started.elapsed().as_millis() as f64;
                env.log_plugin_fetch(
                    "plugin_clone",
                    git_url,
                    "failure",
                    duration,
                    Some(&env.classify_fetch_error(&unshallow_result.stderr)),
                );
                return Err(anyhow!(
                    "Failed to fetch commit {}: {}",
                    commit_sha,
                    unshallow_result.stderr
                ));
            }
        }

        let checkout_result = env
            .exec_git(&["checkout", commit_sha], Some(target_path))
            .await;
        if checkout_result.code != 0 {
            let duration = clone_started.elapsed().as_millis() as f64;
            env.log_plugin_fetch(
                "plugin_clone",
                git_url,
                "failure",
                duration,
                Some(&env.classify_fetch_error(&checkout_result.stderr)),
            );
            return Err(anyhow!(
                "Failed to checkout commit {}: {}",
                commit_sha,
                checkout_result.stderr
            ));
        }
    }

    let duration = clone_started.elapsed().as_millis() as f64;
    env.log_plugin_fetch("plugin_clone", git_url, "success", duration, None);
    Ok(())
}

/// Install a plugin from a git URL
async fn install_from_git(
    env: &dyn PluginLoaderEnv,
    git_url: &str,
    target_path: &Path,
    git_ref: Option<&str>,
    sha: Option<&str>,
) -> Result<()> {
    let safe_url = validate_git_url(git_url)?;
    git_clone_plugin(env, safe_url, target_path, git_ref, sha).await?;
    let ref_message = git_ref.map(|r| format!(" (ref: {})", r)).unwrap_or_default();
    debug!("Cloned repository from {}{} to {:?}", safe_url, ref_message, target_path);
    Ok(())
}

/// Install a plugin from GitHub
async fn install_from_github(
    env: &dyn PluginLoaderEnv,
    repo: &str,
    target_path: &Path,
    git_ref: Option<&str>,
    sha: Option<&str>,
) -> Result<()> {
    let re = regex::Regex::new(r"^[a-zA-Z0-9\-_.]+/[a-zA-Z0-9\-_.]+$").unwrap();
    if !re.is_match(repo) {
        return Err(anyhow!(
            "Invalid GitHub repository format: {}. Expected format: owner/repo",
            repo
        ));
    }
    let git_url = if env.is_env_truthy("MOSSEN_CODE_REMOTE") {
        format!("https://github.com/{}.git", repo)
    } else {
        format!("git@github.com:{}.git", repo)
    };
    install_from_git(env, &git_url, target_path, git_ref, sha).await
}

/// Install from git subdirectory using sparse checkout
pub async fn install_from_git_subdir(
    env: &dyn PluginLoaderEnv,
    url: &str,
    target_path: &Path,
    subdir_path: &str,
    git_ref: Option<&str>,
    sha: Option<&str>,
) -> Result<Option<String>> {
    if !env.check_git_available().await {
        return Err(anyhow!(
            "git-subdir plugin source requires git to be installed and on PATH."
        ));
    }

    let git_url = resolve_git_subdir_url(env, url)?;
    let clone_dir = PathBuf::from(format!("{}.clone", target_path.to_string_lossy()));

    let mut clone_args: Vec<String> = vec![
        "clone".to_string(),
        "--depth".to_string(),
        "1".to_string(),
        "--filter=tree:0".to_string(),
        "--no-checkout".to_string(),
    ];
    if let Some(r) = git_ref {
        clone_args.push("--branch".to_string());
        clone_args.push(r.to_string());
    }
    clone_args.push(git_url.clone());
    clone_args.push(clone_dir.to_string_lossy().to_string());

    let clone_refs: Vec<&str> = clone_args.iter().map(|s| s.as_str()).collect();
    let clone_result = env.exec_git(&clone_refs, None).await;
    if clone_result.code != 0 {
        return Err(anyhow!(
            "Failed to clone repository for git-subdir source: {}",
            clone_result.stderr
        ));
    }

    let result = async {
        let sparse_result = env
            .exec_git(
                &["sparse-checkout", "set", "--cone", "--", subdir_path],
                Some(&clone_dir),
            )
            .await;
        if sparse_result.code != 0 {
            return Err(anyhow!(
                "git sparse-checkout set failed: {}",
                sparse_result.stderr
            ));
        }

        let resolved_sha: Option<String>;
        if let Some(commit_sha) = sha {
            let fetch_sha = env
                .exec_git(
                    &["fetch", "--depth", "1", "origin", commit_sha],
                    Some(&clone_dir),
                )
                .await;
            if fetch_sha.code != 0 {
                let unshallow = env
                    .exec_git(&["fetch", "--unshallow"], Some(&clone_dir))
                    .await;
                if unshallow.code != 0 {
                    return Err(anyhow!(
                        "Failed to fetch commit {}: {}",
                        commit_sha,
                        unshallow.stderr
                    ));
                }
            }
            let checkout = env
                .exec_git(&["checkout", commit_sha], Some(&clone_dir))
                .await;
            if checkout.code != 0 {
                return Err(anyhow!(
                    "Failed to checkout commit {}: {}",
                    commit_sha,
                    checkout.stderr
                ));
            }
            resolved_sha = Some(commit_sha.to_string());
        } else {
            let checkout = env
                .exec_git(&["checkout", "HEAD"], Some(&clone_dir))
                .await;
            if checkout.code != 0 {
                return Err(anyhow!(
                    "git checkout after sparse-checkout failed: {}",
                    checkout.stderr
                ));
            }
            let rev_parse = env
                .exec_git(&["rev-parse", "HEAD"], Some(&clone_dir))
                .await;
            resolved_sha = if rev_parse.code == 0 {
                rev_parse.stdout.map(|s| s.trim().to_string())
            } else {
                None
            };
        }

        let resolved_subdir = env.validate_path_within_base(&clone_dir, subdir_path)?;
        env.rename(&resolved_subdir, target_path).await.map_err(|_| {
            anyhow!(
                "Subdirectory '{}' not found in repository {}{}",
                subdir_path,
                git_url,
                git_ref.map(|r| format!(" (ref: {})", r)).unwrap_or_default()
            )
        })?;

        Ok(resolved_sha)
    }
    .await;

    // Cleanup clone dir
    let _ = env.rm(&clone_dir).await;
    result
}

fn resolve_git_subdir_url(env: &dyn PluginLoaderEnv, url: &str) -> Result<String> {
    let re = regex::Regex::new(r"^[a-zA-Z0-9\-_.]+/[a-zA-Z0-9\-_.]+$").unwrap();
    if re.is_match(url) {
        let git_url = if env.is_env_truthy("MOSSEN_CODE_REMOTE") {
            format!("https://github.com/{}.git", url)
        } else {
            format!("git@github.com:{}.git", url)
        };
        return Ok(git_url);
    }
    validate_git_url(url)?;
    Ok(url.to_string())
}

/// Generate a temporary cache name for a plugin
pub fn generate_temporary_cache_name_for_plugin(source: &PluginSource) -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let random: String = (0..6)
        .map(|_| {
            let idx = rand::random::<u8>() % 36;
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect();

    let prefix = match source {
        PluginSource::RelativePath(_) => "local",
        PluginSource::Structured(s) => match s {
            StructuredPluginSource::Npm { .. } => "npm",
            StructuredPluginSource::Pip { .. } => "pip",
            StructuredPluginSource::GitHub { .. } => "github",
            StructuredPluginSource::Url { .. } => "git",
            StructuredPluginSource::GitSubdir { .. } => "subdir",
        },
    };

    format!("temp_{}_{}_{}", prefix, timestamp, random)
}

/// Cache a plugin from an external source
pub async fn cache_plugin(
    env: &dyn PluginLoaderEnv,
    source: &PluginSource,
    fallback_manifest: Option<&PluginManifest>,
) -> Result<CachedPlugin> {
    let cache_path = get_plugin_cache_path(env);
    env.mkdir(&cache_path).await?;

    let temp_name = generate_temporary_cache_name_for_plugin(source);
    let temp_path = cache_path.join(&temp_name);

    let mut git_commit_sha: Option<String> = None;

    debug!("Caching plugin from source to temporary path {:?}", temp_path);

    let install_result = match source {
        PluginSource::RelativePath(path) => {
            install_from_local(env, Path::new(path), &temp_path).await
        }
        PluginSource::Structured(StructuredPluginSource::Npm { package, registry, version, .. }) => {
            install_from_npm(
                env,
                package,
                &temp_path,
                registry.as_deref(),
                version.as_deref(),
            )
            .await
        }
        PluginSource::Structured(StructuredPluginSource::GitHub { repo, git_ref, sha, .. }) => {
            install_from_github(
                env,
                repo,
                &temp_path,
                git_ref.as_deref(),
                sha.as_deref(),
            )
            .await
        }
        PluginSource::Structured(StructuredPluginSource::Url { url, git_ref, sha, .. }) => {
            install_from_git(env, url, &temp_path, git_ref.as_deref(), sha.as_deref())
                .await
        }
        PluginSource::Structured(StructuredPluginSource::GitSubdir { url, path, git_ref, sha, .. }) => {
            let result = install_from_git_subdir(
                env,
                url,
                &temp_path,
                path,
                git_ref.as_deref(),
                sha.as_deref(),
            )
            .await?;
            git_commit_sha = result;
            Ok(())
        }
        PluginSource::Structured(StructuredPluginSource::Pip { .. }) => {
            Err(anyhow!("Python package plugins are not yet supported"))
        }
    };

    if let Err(e) = install_result {
        if env.path_exists(&temp_path).await {
            debug!("Cleaning up failed installation at {:?}", temp_path);
            let _ = env.rm(&temp_path).await;
        }
        return Err(e);
    }

    // Load manifest
    let manifest_path = temp_path.join(".mossen-plugin").join("plugin.json");
    let legacy_manifest_path = temp_path.join("plugin.json");

    let manifest = if env.path_exists(&manifest_path).await {
        load_and_validate_manifest(env, &manifest_path).await?
    } else if env.path_exists(&legacy_manifest_path).await {
        load_and_validate_manifest(env, &legacy_manifest_path).await?
    } else {
        fallback_manifest.cloned().unwrap_or(PluginManifest {
            name: temp_name.clone(),
            description: Some(format!("Plugin cached from {:?}", source)),
            ..Default::default()
        })
    };

    let final_name = sanitize_path_component(&manifest.name);
    let final_path = cache_path.join(&final_name);

    if env.path_exists(&final_path).await {
        debug!("Removing old cached version at {:?}", final_path);
        let _ = env.rm(&final_path).await;
    }

    env.rename(&temp_path, &final_path).await?;
    debug!("Successfully cached plugin {} to {:?}", manifest.name, final_path);

    Ok(CachedPlugin {
        path: final_path,
        manifest,
        git_commit_sha,
    })
}

/// Cached plugin result
pub struct CachedPlugin {
    pub path: PathBuf,
    pub manifest: PluginManifest,
    pub git_commit_sha: Option<String>,
}

async fn load_and_validate_manifest(
    env: &dyn PluginLoaderEnv,
    manifest_path: &Path,
) -> Result<PluginManifest> {
    let content = env.read_file(manifest_path).await?;
    let manifest: PluginManifest = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Invalid manifest at {:?}: {}", manifest_path, e))?;
    Ok(manifest)
}

/// Install a plugin from a local path
async fn install_from_local(
    env: &dyn PluginLoaderEnv,
    source_path: &Path,
    target_path: &Path,
) -> Result<()> {
    if !env.path_exists(source_path).await {
        return Err(anyhow!("Source path does not exist: {:?}", source_path));
    }
    env.copy_dir(source_path, target_path).await?;
    let git_path = target_path.join(".git");
    let _ = env.rm(&git_path).await;
    Ok(())
}

/// Copy plugin files to versioned cache directory.
pub async fn copy_plugin_to_versioned_cache(
    env: &dyn PluginLoaderEnv,
    source_path: &Path,
    plugin_id: &str,
    version: &str,
    entry: Option<&PluginMarketplaceEntry>,
    marketplace_dir: Option<&Path>,
) -> Result<PathBuf> {
    let zip_cache_mode = env.is_plugin_zip_cache_enabled();
    let cache_path = get_versioned_cache_path(plugin_id, version, env);
    let zip_path = get_versioned_zip_cache_path(plugin_id, version, env);

    // Check existing cache
    if zip_cache_mode {
        if env.path_exists(&zip_path).await {
            debug!("Plugin {} version {} already cached at {:?}", plugin_id, version, zip_path);
            return Ok(zip_path);
        }
    } else if env.path_exists(&cache_path).await {
        if let Ok(entries) = env.read_dir(&cache_path).await {
            if !entries.is_empty() {
                debug!("Plugin {} version {} already cached at {:?}", plugin_id, version, cache_path);
                return Ok(cache_path);
            }
        }
        let _ = env.rm(&cache_path).await;
    }

    // Seed cache check
    if let Some(seed_path) = probe_seed_cache(env, plugin_id, version).await {
        debug!("Using seed cache for {}@{} at {:?}", plugin_id, version, seed_path);
        return Ok(seed_path);
    }

    // Create parent directories
    if let Some(parent) = cache_path.parent() {
        env.mkdir(parent).await?;
    }

    // Copy source
    if let (Some(e), Some(mp_dir)) = (entry, marketplace_dir) {
        if let PluginSource::RelativePath(ref source_str) = e.source {
            let source_dir = env.validate_path_within_base(mp_dir, source_str)?;
            debug!("Copying source directory {} for plugin {}", source_str, plugin_id);
            env.copy_dir(&source_dir, &cache_path).await?;
        } else {
            env.copy_dir(source_path, &cache_path).await?;
        }
    } else {
        env.copy_dir(source_path, &cache_path).await?;
    }

    // Remove .git directory
    let git_path = cache_path.join(".git");
    let _ = env.rm(&git_path).await;

    // Validate content
    let cache_entries = env.read_dir(&cache_path).await?;
    if cache_entries.is_empty() {
        return Err(anyhow!(
            "Failed to copy plugin {} to versioned cache: destination is empty after copy",
            plugin_id
        ));
    }

    // Zip cache mode conversion
    if zip_cache_mode {
        env.convert_directory_to_zip_in_place(&cache_path, &zip_path).await?;
        debug!("Successfully cached plugin {} as ZIP at {:?}", plugin_id, zip_path);
        return Ok(zip_path);
    }

    debug!("Successfully cached plugin {} at {:?}", plugin_id, cache_path);
    Ok(cache_path)
}

/// Load and validate a plugin manifest from a JSON file.
pub async fn load_plugin_manifest(
    env: &dyn PluginLoaderEnv,
    manifest_path: &Path,
    plugin_name: &str,
    source: &str,
) -> Result<PluginManifest> {
    if !env.path_exists(manifest_path).await {
        return Ok(PluginManifest {
            name: plugin_name.to_string(),
            description: Some(format!("Plugin from {}", source)),
            ..Default::default()
        });
    }

    let content = env.read_file(manifest_path).await.map_err(|e| {
        anyhow!(
            "Plugin {} has a corrupt manifest file at {:?}.\n\nJSON parse error: {}",
            plugin_name,
            manifest_path,
            e
        )
    })?;

    let manifest: PluginManifest = serde_json::from_str(&content).map_err(|e| {
        anyhow!(
            "Plugin {} has an invalid manifest file at {:?}.\n\nValidation errors: {}",
            plugin_name,
            manifest_path,
            e
        )
    })?;

    Ok(manifest)
}

/// Creates a LoadedPlugin object from a plugin directory path.
pub async fn create_plugin_from_path(
    env: &dyn PluginLoaderEnv,
    plugin_path: &Path,
    source: &str,
    enabled: bool,
    fallback_name: &str,
    strict: bool,
) -> Result<(LoadedPlugin, Vec<PluginError>)> {
    let mut errors: Vec<PluginError> = Vec::new();

    // Step 1: Load manifest
    let manifest_path = plugin_path.join(".mossen-plugin").join("plugin.json");
    let manifest = load_plugin_manifest(env, &manifest_path, fallback_name, source).await?;

    // Step 2: Create base plugin object
    let mut plugin = LoadedPlugin {
        name: manifest.name.clone(),
        manifest: manifest.clone(),
        path: plugin_path.to_path_buf(),
        source: source.to_string(),
        repository: source.to_string(),
        enabled,
        commands_path: None,
        commands_paths: None,
        commands_metadata: None,
        agents_path: None,
        agents_paths: None,
        skills_path: None,
        skills_paths: None,
        output_styles_path: None,
        output_styles_paths: None,
        hooks_config: None,
        settings: None,
        sha: None,
    };

    // Step 3: Auto-detect directories
    let commands_dir = plugin_path.join("commands");
    let agents_dir = plugin_path.join("agents");
    let skills_dir = plugin_path.join("skills");
    let output_styles_dir = plugin_path.join("output-styles");

    if manifest.commands.is_none() && env.path_exists(&commands_dir).await {
        plugin.commands_path = Some(commands_dir);
    }
    if manifest.agents.is_none() && env.path_exists(&agents_dir).await {
        plugin.agents_path = Some(agents_dir);
    }
    if manifest.skills.is_none() && env.path_exists(&skills_dir).await {
        plugin.skills_path = Some(skills_dir);
    }
    if manifest.output_styles.is_none() && env.path_exists(&output_styles_dir).await {
        plugin.output_styles_path = Some(output_styles_dir);
    }

    // Step 4: Process manifest commands/agents/skills/outputStyles paths
    if let Some(ref commands) = manifest.commands {
        let valid_paths = validate_plugin_paths_from_manifest(
            env, commands, plugin_path, &manifest.name, source, PluginComponent::Commands, &mut errors,
        ).await;
        if !valid_paths.is_empty() {
            plugin.commands_paths = Some(valid_paths);
        }
    }

    if let Some(ref agents) = manifest.agents {
        let valid_paths = validate_plugin_paths_from_manifest(
            env, agents, plugin_path, &manifest.name, source, PluginComponent::Agents, &mut errors,
        ).await;
        if !valid_paths.is_empty() {
            plugin.agents_paths = Some(valid_paths);
        }
    }

    if let Some(ref skills) = manifest.skills {
        let valid_paths = validate_plugin_paths_from_manifest(
            env, skills, plugin_path, &manifest.name, source, PluginComponent::Skills, &mut errors,
        ).await;
        if !valid_paths.is_empty() {
            plugin.skills_paths = Some(valid_paths);
        }
    }

    if let Some(ref output_styles) = manifest.output_styles {
        let valid_paths = validate_plugin_paths_from_manifest(
            env, output_styles, plugin_path, &manifest.name, source, PluginComponent::OutputStyles, &mut errors,
        ).await;
        if !valid_paths.is_empty() {
            plugin.output_styles_paths = Some(valid_paths);
        }
    }

    // Step 5: Load hooks
    let standard_hooks_path = plugin_path.join("hooks").join("hooks.json");
    if env.path_exists(&standard_hooks_path).await {
        match load_plugin_hooks(env, &standard_hooks_path, &manifest.name).await {
            Ok(hooks) => {
                plugin.hooks_config = Some(hooks);
                debug!("Loaded hooks from standard location for plugin {}", manifest.name);
            }
            Err(e) => {
                errors.push(PluginError::HookLoadFailed {
                    source: source.to_string(),
                    plugin: manifest.name.clone(),
                    hook_path: standard_hooks_path.to_string_lossy().to_string(),
                    reason: e.to_string(),
                });
            }
        }
    }

    // Step 6: Load plugin settings
    let plugin_settings = load_plugin_settings(env, plugin_path, &manifest).await;
    if let Some(settings) = plugin_settings {
        plugin.settings = Some(settings);
    }

    Ok((plugin, errors))
}

/// Validate plugin paths from manifest entries
async fn validate_plugin_paths_from_manifest(
    env: &dyn PluginLoaderEnv,
    paths: &serde_json::Value,
    plugin_path: &Path,
    plugin_name: &str,
    source: &str,
    component: PluginComponent,
    errors: &mut Vec<PluginError>,
) -> Vec<PathBuf> {
    let path_list: Vec<String> = if let Some(arr) = paths.as_array() {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    } else if let Some(s) = paths.as_str() {
        vec![s.to_string()]
    } else {
        return Vec::new();
    };

    let mut valid_paths = Vec::new();
    for rel_path in &path_list {
        let full_path = plugin_path.join(rel_path);
        if env.path_exists(&full_path).await {
            valid_paths.push(full_path);
        } else {
            debug!(
                "{} path {} specified in manifest but not found for {}",
                component.as_str(), rel_path, plugin_name
            );
            errors.push(PluginError::PathNotFound {
                source: source.to_string(),
                plugin: Some(plugin_name.to_string()),
                path: full_path.to_string_lossy().to_string(),
                component: component.as_str().to_string(),
            });
        }
    }
    valid_paths
}

/// Load plugin hooks from a hooks.json file
async fn load_plugin_hooks(
    env: &dyn PluginLoaderEnv,
    hooks_path: &Path,
    _plugin_name: &str,
) -> Result<HooksSettings> {
    let content = env.read_file(hooks_path).await?;
    let raw: serde_json::Value = serde_json::from_str(&content)?;
    let hooks = raw
        .get("hooks")
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let settings: HooksSettings = serde_json::from_value(hooks)?;
    Ok(settings)
}

/// Load plugin settings from settings.json or manifest.settings
async fn load_plugin_settings(
    env: &dyn PluginLoaderEnv,
    plugin_path: &Path,
    manifest: &PluginManifest,
) -> Option<HashMap<String, serde_json::Value>> {
    let settings_path = plugin_path.join("settings.json");
    if let Ok(content) = env.read_file(&settings_path).await {
        if let Ok(parsed) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&content) {
            // Filter to allowed keys
            let filtered = filter_plugin_settings(parsed);
            if !filtered.is_empty() {
                debug!("Loaded settings from settings.json for plugin {}", manifest.name);
                return Some(filtered);
            }
        }
    }

    if let Some(ref settings) = manifest.settings {
        {
            let map: HashMap<String, serde_json::Value> = settings.clone();
            let filtered = filter_plugin_settings(map);
            if !filtered.is_empty() {
                debug!("Loaded settings from manifest for plugin {}", manifest.name);
                return Some(filtered);
            }
        }
    }

    None
}

/// Filter plugin settings to only allowed keys
fn filter_plugin_settings(
    settings: HashMap<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    let allowed_keys = ["agent"];
    settings
        .into_iter()
        .filter(|(k, _)| allowed_keys.contains(&k.as_str()))
        .collect()
}

/// Merge plugins from session, marketplace, and builtin sources.
pub fn merge_plugin_sources(
    session: Vec<LoadedPlugin>,
    marketplace: Vec<LoadedPlugin>,
    builtin: Vec<LoadedPlugin>,
    managed_names: Option<&HashSet<String>>,
) -> (Vec<LoadedPlugin>, Vec<PluginError>) {
    let mut errors: Vec<PluginError> = Vec::new();

    // Filter session plugins blocked by managed settings
    let session_plugins: Vec<LoadedPlugin> = session
        .into_iter()
        .filter(|p| {
            if let Some(managed) = managed_names {
                if managed.contains(&p.name) {
                    debug!(
                        "Plugin \"{}\" from --plugin-dir is blocked by managed settings",
                        p.name
                    );
                    errors.push(PluginError::GenericError {
                        source: p.source.clone(),
                        plugin: Some(p.name.clone()),
                        error: format!(
                            "--plugin-dir copy of \"{}\" ignored: plugin is locked by managed settings",
                            p.name
                        ),
                    });
                    return false;
                }
            }
            true
        })
        .collect();

    let session_names: HashSet<String> = session_plugins.iter().map(|p| p.name.clone()).collect();
    let marketplace_plugins: Vec<LoadedPlugin> = marketplace
        .into_iter()
        .filter(|p| {
            if session_names.contains(&p.name) {
                debug!("Plugin \"{}\" from --plugin-dir overrides installed version", p.name);
                return false;
            }
            true
        })
        .collect();

    let mut all_plugins = session_plugins;
    all_plugins.extend(marketplace_plugins);
    all_plugins.extend(builtin);

    (all_plugins, errors)
}

/// Main plugin loading function — cache only variant
pub async fn load_all_plugins_cache_only(
    env: &dyn PluginLoaderEnv,
) -> PluginLoadResult {
    if env.is_env_truthy("MOSSEN_CODE_SYNC_PLUGIN_INSTALL") {
        return load_all_plugins(env).await;
    }
    assemble_plugin_load_result(env, true).await
}

/// Main plugin loading function that discovers and loads all plugins.
pub async fn load_all_plugins(env: &dyn PluginLoaderEnv) -> PluginLoadResult {
    assemble_plugin_load_result(env, false).await
}

/// Shared body of load_all_plugins and load_all_plugins_cache_only.
async fn assemble_plugin_load_result(
    env: &dyn PluginLoaderEnv,
    cache_only: bool,
) -> PluginLoadResult {
    let inline_plugins = env.get_inline_plugins();

    // Load from marketplaces
    let marketplace_result = load_plugins_from_marketplaces(env, cache_only).await;

    // Load session-only plugins
    let session_result = if !inline_plugins.is_empty() {
        load_session_only_plugins(env, &inline_plugins).await
    } else {
        (Vec::new(), Vec::new())
    };

    // Load builtin plugins
    let builtin_result = env.get_builtin_plugins();
    let mut builtin_all = builtin_result.enabled;
    builtin_all.extend(builtin_result.disabled);

    // Merge sources
    let managed_names = env.get_managed_plugin_names();
    let (all_plugins, merge_errors) = merge_plugin_sources(
        session_result.0,
        marketplace_result.0,
        builtin_all,
        managed_names.as_ref(),
    );

    let mut all_errors: Vec<PluginError> = Vec::new();
    all_errors.extend(marketplace_result.1);
    all_errors.extend(session_result.1);
    all_errors.extend(merge_errors);

    // Verify dependencies
    let (demoted, dep_errors) = env.verify_and_demote(&all_plugins);
    all_errors.extend(dep_errors);

    let mut all_plugins = all_plugins;
    for p in &mut all_plugins {
        if demoted.contains(&p.source) {
            p.enabled = false;
        }
    }

    let enabled_plugins: Vec<LoadedPlugin> = all_plugins.iter().filter(|p| p.enabled).cloned().collect();
    let disabled_plugins: Vec<LoadedPlugin> = all_plugins.iter().filter(|p| !p.enabled).cloned().collect();

    debug!(
        "Found {} plugins ({} enabled, {} disabled)",
        all_plugins.len(),
        enabled_plugins.len(),
        disabled_plugins.len()
    );

    // Cache plugin settings
    cache_plugin_settings(env, &enabled_plugins);

    PluginLoadResult {
        enabled: enabled_plugins,
        disabled: disabled_plugins,
        errors: all_errors,
    }
}

/// Load plugins from marketplaces
async fn load_plugins_from_marketplaces(
    env: &dyn PluginLoaderEnv,
    _cache_only: bool,
) -> (Vec<LoadedPlugin>, Vec<PluginError>) {
    let mut enabled_plugins = env.get_add_dir_enabled_plugins();
    for (k, v) in env.get_settings_enabled_plugins() {
        enabled_plugins.insert(k, v);
    }

    let mut plugins: Vec<LoadedPlugin> = Vec::new();
    let mut errors: Vec<PluginError> = Vec::new();

    let builtin_name = env.get_builtin_marketplace_name();

    // Filter marketplace plugin entries
    let entries: Vec<(String, serde_json::Value)> = enabled_plugins
        .into_iter()
        .filter(|(key, value)| {
            if !key.contains('@') || value.is_null() {
                return false;
            }
            let (_, marketplace) = env.parse_plugin_identifier(key);
            marketplace.as_deref() != Some(builtin_name)
        })
        .collect();

    // Load known marketplaces config
    let known_marketplaces = env.load_known_marketplaces_config_safe().await;

    // Policy checks
    let strict_allowlist = env.get_strict_known_marketplaces();
    let blocklist = env.get_blocked_marketplaces();
    let has_enterprise_policy =
        strict_allowlist.is_some() || blocklist.as_ref().map(|b| !b.is_empty()).unwrap_or(false);

    // Get installed plugins data
    let installed_plugins_data = env.get_in_memory_installed_plugins();

    for (plugin_id, enabled_value) in &entries {
        let (plugin_name, marketplace_name) = env.parse_plugin_identifier(plugin_id);
        let marketplace_name = match marketplace_name {
            Some(m) => m,
            None => continue,
        };

        // Policy check
        let marketplace_config = known_marketplaces.get(&marketplace_name);
        if marketplace_config.is_none() && has_enterprise_policy {
            errors.push(PluginError::MarketplaceBlockedByPolicy {
                source: plugin_id.clone(),
                plugin: plugin_name.clone(),
                marketplace: marketplace_name.clone(),
                blocked_by_blocklist: strict_allowlist.is_none(),
                allowed_sources: strict_allowlist
                    .as_ref()
                    .map(|list| list.iter().map(|s| env.format_source_for_display(s)).collect())
                    .unwrap_or_default(),
            });
            continue;
        }

        if let Some(config) = marketplace_config {
            if !env.is_source_allowed_by_policy(&config.source) {
                let is_blocked = env.is_source_in_blocklist(&config.source);
                errors.push(PluginError::MarketplaceBlockedByPolicy {
                    source: plugin_id.clone(),
                    plugin: plugin_name.clone(),
                    marketplace: marketplace_name.clone(),
                    blocked_by_blocklist: is_blocked,
                    allowed_sources: if is_blocked {
                        vec![]
                    } else {
                        strict_allowlist
                            .as_ref()
                            .map(|list| list.iter().map(|s| env.format_source_for_display(s)).collect())
                            .unwrap_or_default()
                    },
                });
                continue;
            }
        }

        // Look up plugin in marketplace
        let result = env.get_plugin_by_id_cache_only(plugin_id).await;
        if result.is_none() {
            errors.push(PluginError::PluginNotFound {
                source: plugin_id.clone(),
                plugin_id: plugin_name.unwrap_or_default(),
                marketplace: marketplace_name,
            });
            continue;
        }

        let (_entry, _install_location) = result.unwrap();
        let is_enabled = enabled_value.as_bool().unwrap_or(false);

        // Load plugin from marketplace entry (simplified)
        let install_path = installed_plugins_data
            .plugins
            .get(plugin_id)
            .and_then(|entries| entries.first())
            .map(|e| e.install_path.clone());

        if let Some(ref path_str) = install_path {
            let path = PathBuf::from(path_str);
            if env.path_exists(&path).await {
                let fallback = plugin_name.as_deref().unwrap_or(plugin_id);
                match create_plugin_from_path(env, &path, plugin_id, is_enabled, fallback, true)
                    .await
                {
                    Ok((plugin, plugin_errors)) => {
                        plugins.push(plugin);
                        errors.extend(plugin_errors);
                    }
                    Err(e) => {
                        errors.push(PluginError::GenericError {
                            source: plugin_id.clone(),
                            plugin: plugin_name,
                            error: e.to_string(),
                        });
                    }
                }
            } else {
                errors.push(PluginError::PluginCacheMiss {
                    source: plugin_id.clone(),
                    plugin: plugin_name.unwrap_or_default(),
                    install_path: path_str.clone(),
                });
            }
        } else {
            errors.push(PluginError::PluginCacheMiss {
                source: plugin_id.clone(),
                plugin: plugin_name.unwrap_or_default(),
                install_path: "(not recorded)".to_string(),
            });
        }
    }

    (plugins, errors)
}

/// Load session-only plugins from --plugin-dir CLI flag.
async fn load_session_only_plugins(
    env: &dyn PluginLoaderEnv,
    session_plugin_paths: &[String],
) -> (Vec<LoadedPlugin>, Vec<PluginError>) {
    let mut plugins: Vec<LoadedPlugin> = Vec::new();
    let mut errors: Vec<PluginError> = Vec::new();

    for (index, plugin_path) in session_plugin_paths.iter().enumerate() {
        let resolved_path = PathBuf::from(plugin_path);
        if !env.path_exists(&resolved_path).await {
            debug!("Plugin path does not exist: {:?}, skipping", resolved_path);
            errors.push(PluginError::PathNotFound {
                source: format!("inline[{}]", index),
                plugin: None,
                path: resolved_path.to_string_lossy().to_string(),
                component: "commands".to_string(),
            });
            continue;
        }

        let dir_name = resolved_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        match create_plugin_from_path(
            env,
            &resolved_path,
            &format!("{}@inline", dir_name),
            true,
            &dir_name,
            true,
        )
        .await
        {
            Ok((mut plugin, plugin_errors)) => {
                plugin.source = format!("{}@inline", plugin.name);
                plugin.repository = format!("{}@inline", plugin.name);
                plugins.push(plugin);
                errors.extend(plugin_errors);
                debug!("Loaded inline plugin from path: {}", dir_name);
            }
            Err(e) => {
                errors.push(PluginError::GenericError {
                    source: format!("inline[{}]", index),
                    plugin: None,
                    error: format!("Failed to load plugin: {}", e),
                });
            }
        }
    }

    if !plugins.is_empty() {
        debug!(
            "Loaded {} session-only plugins from --plugin-dir",
            plugins.len()
        );
    }

    (plugins, errors)
}

/// Clears the memoized plugin cache.
pub fn clear_plugin_cache(env: &dyn PluginLoaderEnv, reason: Option<&str>) {
    if let Some(r) = reason {
        debug!("clearPluginCache: invalidating loadAllPlugins cache ({})", r);
    }
    if env.get_plugin_settings_base().is_some() {
        env.reset_settings_cache();
    }
    env.clear_plugin_settings_base();
}

/// Merge settings from all enabled plugins into a single record.
fn merge_plugin_settings_internal(
    plugins: &[LoadedPlugin],
) -> Option<HashMap<String, serde_json::Value>> {
    let mut merged: Option<HashMap<String, serde_json::Value>> = None;

    for plugin in plugins {
        if let Some(ref settings) = plugin.settings {
            let map = merged.get_or_insert_with(HashMap::new);
            for (key, value) in settings {
                if map.contains_key(key) {
                    debug!(
                        "Plugin \"{}\" overrides setting \"{}\" (previously set by another plugin)",
                        plugin.name, key
                    );
                }
                map.insert(key.clone(), value.clone());
            }
        }
    }

    merged
}

/// Store merged plugin settings in the synchronous cache.
pub fn cache_plugin_settings(env: &dyn PluginLoaderEnv, plugins: &[LoadedPlugin]) {
    let settings = merge_plugin_settings_internal(plugins);
    env.set_plugin_settings_base(settings.clone());
    if let Some(ref s) = settings {
        if !s.is_empty() {
            env.reset_settings_cache();
            debug!(
                "Cached plugin settings with keys: {}",
                s.keys().cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }
}
