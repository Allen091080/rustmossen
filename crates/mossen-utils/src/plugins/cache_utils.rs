use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tokio::fs;
use tracing::debug;

const ORPHANED_AT_FILENAME: &str = ".orphaned_at";
const CLEANUP_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000; // 7 days

/// TTL for prune-plan tokens (10 minutes).
pub const PRUNE_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

/// Clear all plugin-related caches.
pub trait PluginCacheClearing: Send + Sync {
    fn clear_plugin_cache(&self);
    fn clear_plugin_command_cache(&self);
    fn clear_plugin_agent_cache(&self);
    fn clear_plugin_hook_cache(&self);
    fn clear_plugin_options_cache(&self);
    fn clear_plugin_output_style_cache(&self);
    fn clear_all_output_styles_cache(&self);
    fn clear_commands_cache(&self);
    fn clear_agent_definitions_cache(&self);
    fn clear_prompt_cache(&self);
    fn reset_sent_skill_names(&self);
    fn prune_removed_plugin_hooks(&self);
}

/// Clear all plugin caches.
pub fn clear_all_plugin_caches(clearer: &dyn PluginCacheClearing) {
    clearer.clear_plugin_cache();
    clearer.clear_plugin_command_cache();
    clearer.clear_plugin_agent_cache();
    clearer.clear_plugin_hook_cache();
    clearer.prune_removed_plugin_hooks();
    clearer.clear_plugin_options_cache();
    clearer.clear_plugin_output_style_cache();
    clearer.clear_all_output_styles_cache();
}

/// Clear all caches including commands and agents.
pub fn clear_all_caches(clearer: &dyn PluginCacheClearing) {
    clear_all_plugin_caches(clearer);
    clearer.clear_commands_cache();
    clearer.clear_agent_definitions_cache();
    clearer.clear_prompt_cache();
    clearer.reset_sent_skill_names();
}

/// Mark a plugin version as orphaned.
pub async fn mark_plugin_version_orphaned(version_path: &Path) {
    let orphaned_at_path = version_path.join(ORPHANED_AT_FILENAME);
    let now = current_time_ms().to_string();
    if let Err(e) = fs::write(&orphaned_at_path, &now).await {
        debug!("Failed to write .orphaned_at: {:?}: {}", version_path, e);
    }
}

/// Clean up orphaned plugin versions that have been orphaned for more than 7 days.
pub async fn cleanup_orphaned_plugin_versions_in_background(
    cache_path: &Path,
    installed_paths: &HashSet<PathBuf>,
    is_zip_cache_enabled: bool,
) {
    if is_zip_cache_enabled {
        return;
    }

    let now = current_time_ms();

    // Pass 1: Remove .orphaned_at from installed versions
    for path in installed_paths {
        let orphaned_at = path.join(ORPHANED_AT_FILENAME);
        let _ = fs::remove_file(&orphaned_at).await;
    }

    // Pass 2: Process orphaned versions
    let marketplaces = read_subdirs(cache_path).await;
    for marketplace in &marketplaces {
        let marketplace_path = cache_path.join(marketplace);
        let plugins = read_subdirs(&marketplace_path).await;
        for plugin in &plugins {
            let plugin_path = marketplace_path.join(plugin);
            let versions = read_subdirs(&plugin_path).await;
            for version in &versions {
                let version_path = plugin_path.join(version);
                if installed_paths.contains(&version_path) {
                    continue;
                }
                process_orphaned_plugin_version(&version_path, now).await;
            }
            remove_if_empty(&plugin_path).await;
        }
        remove_if_empty(&marketplace_path).await;
    }
}

async fn process_orphaned_plugin_version(version_path: &Path, now: u64) {
    let orphaned_at_path = version_path.join(ORPHANED_AT_FILENAME);
    let orphaned_at = match fs::metadata(&orphaned_at_path).await {
        Ok(meta) => meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                mark_plugin_version_orphaned(version_path).await;
                return;
            }
            debug!("Failed to stat orphaned marker: {:?}: {}", version_path, e);
            return;
        }
    };

    if let Some(at) = orphaned_at {
        if now - at > CLEANUP_AGE_MS {
            if let Err(e) = fs::remove_dir_all(version_path).await {
                debug!("Failed to delete orphaned version: {:?}: {}", version_path, e);
            }
        }
    }
}

async fn remove_if_empty(dir_path: &Path) {
    if read_subdirs(dir_path).await.is_empty() {
        let _ = fs::remove_dir_all(dir_path).await;
    }
}

async fn read_subdirs(dir_path: &Path) -> Vec<String> {
    match fs::read_dir(dir_path).await {
        Ok(mut entries) => {
            let mut result = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            result.push(name.to_string());
                        }
                    }
                }
            }
            result
        }
        Err(_) => Vec::new(),
    }
}

/// Prune plan entry.
#[derive(Debug, Clone)]
pub struct PrunePlanEntry {
    pub version_path: PathBuf,
    pub marketplace: String,
    pub plugin: String,
    pub version: String,
    pub orphaned_at_ms: Option<u64>,
    pub age_days: Option<i64>,
    pub size_bytes: i64,
}

/// Plugin prune plan.
#[derive(Debug, Clone)]
pub struct PluginPrunePlan {
    pub token: String,
    pub created_at: u64,
    pub expired_orphans: Vec<PrunePlanEntry>,
    pub unmarked_orphans: Vec<PrunePlanEntry>,
    pub fresh_orphans: Vec<PrunePlanEntry>,
    pub installed_skipped: Vec<PrunePlanEntry>,
    pub zip_cache_mode: bool,
}

/// Plugin prune result.
#[derive(Debug, Clone)]
pub struct PluginPruneResult {
    pub marked: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
    pub cleaned_dirs: Vec<PathBuf>,
    pub errors: Vec<PruneError>,
}

#[derive(Debug, Clone)]
pub struct PruneError {
    pub path: PathBuf,
    pub phase: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum PluginPruneError {
    UnknownToken,
    ExpiredToken,
    ZipCacheMode,
}

static PRUNE_PLAN_STORE: Lazy<Mutex<std::collections::HashMap<String, PluginPrunePlan>>> =
    Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

fn evict_expired_plans(now: u64) {
    let mut store = PRUNE_PLAN_STORE.lock().unwrap();
    store.retain(|_, plan| now - plan.created_at <= PRUNE_PLAN_TOKEN_TTL_MS);
}

/// Build a dry-run prune plan.
pub async fn get_plugin_prune_plan(
    cache_path: &Path,
    installed_paths: &HashSet<PathBuf>,
    is_zip_cache_enabled: bool,
) -> PluginPrunePlan {
    let now = current_time_ms();
    evict_expired_plans(now);

    let mut expired_orphans = Vec::new();
    let mut unmarked_orphans = Vec::new();
    let mut fresh_orphans = Vec::new();
    let mut installed_skipped = Vec::new();

    if !is_zip_cache_enabled {
        for marketplace in read_subdirs(cache_path).await {
            let marketplace_path = cache_path.join(&marketplace);
            for plugin in read_subdirs(&marketplace_path).await {
                let plugin_path = marketplace_path.join(&plugin);
                for version in read_subdirs(&plugin_path).await {
                    let version_path = plugin_path.join(&version);
                    let entry = build_plan_entry(&version_path, &marketplace, &plugin, &version, now).await;

                    if installed_paths.contains(&version_path) {
                        installed_skipped.push(entry);
                        continue;
                    }
                    match entry.orphaned_at_ms {
                        None => unmarked_orphans.push(entry),
                        Some(at) if now - at > CLEANUP_AGE_MS => expired_orphans.push(entry),
                        _ => fresh_orphans.push(entry),
                    }
                }
            }
        }
    }

    let token = format!("{:08x}", rand::random::<u32>());
    let plan = PluginPrunePlan {
        token: token.clone(),
        created_at: now,
        expired_orphans,
        unmarked_orphans,
        fresh_orphans,
        installed_skipped,
        zip_cache_mode: is_zip_cache_enabled,
    };
    PRUNE_PLAN_STORE.lock().unwrap().insert(token, plan.clone());
    plan
}

/// Execute a prune plan by token.
pub async fn execute_plugin_prune_plan(
    token: &str,
    installed_paths: &HashSet<PathBuf>,
) -> Result<PluginPruneResult, PluginPruneError> {
    let now = current_time_ms();
    evict_expired_plans(now);

    let plan = {
        let mut store = PRUNE_PLAN_STORE.lock().unwrap();
        match store.remove(token) {
            Some(p) => p,
            None => return Err(PluginPruneError::UnknownToken),
        }
    };

    if now - plan.created_at > PRUNE_PLAN_TOKEN_TTL_MS {
        return Err(PluginPruneError::ExpiredToken);
    }
    if plan.zip_cache_mode {
        return Err(PluginPruneError::ZipCacheMode);
    }

    let mut result = PluginPruneResult {
        marked: Vec::new(),
        deleted: Vec::new(),
        cleaned_dirs: Vec::new(),
        errors: Vec::new(),
    };

    // Phase 1: mark unmarked orphans
    for entry in &plan.unmarked_orphans {
        if installed_paths.contains(&entry.version_path) {
            continue;
        }
        mark_plugin_version_orphaned(&entry.version_path).await;
        result.marked.push(entry.version_path.clone());
    }

    // Phase 2: delete expired orphans
    let mut parent_dirs = HashSet::new();
    for entry in &plan.expired_orphans {
        if installed_paths.contains(&entry.version_path) {
            continue;
        }
        match fs::remove_dir_all(&entry.version_path).await {
            Ok(()) => {
                result.deleted.push(entry.version_path.clone());
                if let Some(parent) = entry.version_path.parent() {
                    parent_dirs.insert(parent.to_path_buf());
                    if let Some(gp) = parent.parent() {
                        parent_dirs.insert(gp.to_path_buf());
                    }
                }
            }
            Err(e) => {
                result.errors.push(PruneError {
                    path: entry.version_path.clone(),
                    phase: "delete".to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    // Phase 3: clean up empty parent dirs
    let mut parents: Vec<PathBuf> = parent_dirs.into_iter().collect();
    parents.sort_by(|a, b| b.to_string_lossy().len().cmp(&a.to_string_lossy().len()));
    for dir in &parents {
        if read_subdirs(dir).await.is_empty() {
            let _ = fs::remove_dir_all(dir).await;
            result.cleaned_dirs.push(dir.clone());
        }
    }

    Ok(result)
}

async fn build_plan_entry(
    version_path: &Path,
    marketplace: &str,
    plugin: &str,
    version: &str,
    now: u64,
) -> PrunePlanEntry {
    let orphaned_at_path = version_path.join(ORPHANED_AT_FILENAME);
    let orphaned_at_ms = fs::metadata(&orphaned_at_path)
        .await
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);

    let age_days = orphaned_at_ms.map(|at| ((now - at) / (24 * 60 * 60 * 1000)) as i64);
    let size_bytes = compute_dir_size_bytes(version_path).await;

    PrunePlanEntry {
        version_path: version_path.to_path_buf(),
        marketplace: marketplace.to_string(),
        plugin: plugin.to_string(),
        version: version.to_string(),
        orphaned_at_ms,
        age_days,
        size_bytes,
    }
}

fn compute_dir_size_bytes(dir_path: &Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = i64> + Send + '_>> {
    Box::pin(async move {
    let mut total: i64 = 0;
    let mut entries = match fs::read_dir(dir_path).await {
        Ok(e) => e,
        Err(_) => return -1,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(ft) = entry.file_type().await {
            let child = entry.path();
            if ft.is_dir() {
                let sub = compute_dir_size_bytes(&child).await;
                if sub < 0 {
                    return -1;
                }
                total += sub;
            } else if ft.is_file() {
                match fs::metadata(&child).await {
                    Ok(m) => total += m.len() as i64,
                    Err(_) => return -1,
                }
            }
        }
    }
    total
    })
}

/// Plugin cache summary for /plugin status.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginCacheSummary {
    pub zip_cache_mode: bool,
    pub cache_path: PathBuf,
    pub marketplace_count: usize,
    pub unique_plugin_count: usize,
    pub cache_version_count: usize,
    pub installed_count: usize,
    pub expired_orphan_count: usize,
    pub unmarked_orphan_count: usize,
    pub fresh_orphan_count: usize,
    pub installed_skipped_count: usize,
    pub cache_bytes: i64,
}

/// Summarize plugin cache status.
pub async fn summarize_plugin_cache(
    cache_path: &Path,
    installed_paths: &HashSet<PathBuf>,
    is_zip_cache_enabled: bool,
) -> PluginCacheSummary {
    if is_zip_cache_enabled {
        return PluginCacheSummary {
            zip_cache_mode: true,
            cache_path: cache_path.to_path_buf(),
            marketplace_count: 0,
            unique_plugin_count: 0,
            cache_version_count: 0,
            installed_count: 0,
            expired_orphan_count: 0,
            unmarked_orphan_count: 0,
            fresh_orphan_count: 0,
            installed_skipped_count: 0,
            cache_bytes: -1,
        };
    }

    let now = current_time_ms();
    let mut marketplace_count = 0;
    let mut unique_plugin_count = 0;
    let mut cache_version_count = 0;
    let mut expired_orphan_count = 0;
    let mut unmarked_orphan_count = 0;
    let mut fresh_orphan_count = 0;
    let mut installed_skipped_count = 0;

    for marketplace in read_subdirs(cache_path).await {
        marketplace_count += 1;
        let mp = cache_path.join(&marketplace);
        for plugin in read_subdirs(&mp).await {
            unique_plugin_count += 1;
            let pp = mp.join(&plugin);
            for version in read_subdirs(&pp).await {
                cache_version_count += 1;
                let vp = pp.join(&version);
                if installed_paths.contains(&vp) {
                    installed_skipped_count += 1;
                    continue;
                }
                let orphaned_at_path = vp.join(ORPHANED_AT_FILENAME);
                let orphaned_at_ms = fs::metadata(&orphaned_at_path)
                    .await
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64);
                match orphaned_at_ms {
                    None => unmarked_orphan_count += 1,
                    Some(at) if now - at > CLEANUP_AGE_MS => expired_orphan_count += 1,
                    _ => fresh_orphan_count += 1,
                }
            }
        }
    }

    let cache_bytes = compute_dir_size_bytes(cache_path).await;

    PluginCacheSummary {
        zip_cache_mode: false,
        cache_path: cache_path.to_path_buf(),
        marketplace_count,
        unique_plugin_count,
        cache_version_count,
        installed_count: installed_paths.len(),
        expired_orphan_count,
        unmarked_orphan_count,
        fresh_orphan_count,
        installed_skipped_count,
        cache_bytes,
    }
}

/// Reset prune plan store for testing.
pub fn reset_prune_plan_store_for_testing() {
    PRUNE_PLAN_STORE.lock().unwrap().clear();
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
