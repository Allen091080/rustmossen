//! Manages plugin installation metadata stored in installed_plugins.json
//!
//! This module separates plugin installation state (global) from enabled/disabled
//! state (per-repository). The installed_plugins.json file tracks:
//! - Which plugins are installed globally
//! - Installation metadata (version, timestamps, paths)
//!
//! The enabled/disabled state remains in .mossen/settings.json for per-repo control.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use once_cell::sync::Lazy;
use tracing::debug;

use super::schemas::{
    InstalledPlugin, InstalledPluginsFileV1, InstalledPluginsFileV2, PluginInstallationEntry,
    PluginScope,
};

/// Type alias for V2 plugins map
pub type InstalledPluginsMapV2 = HashMap<String, Vec<PluginInstallationEntry>>;

/// Persistable scope (all scopes are persistable in the schema)
pub type PersistableScope = PluginScope;

/// Trait for external dependencies (settings, marketplace, git, cwd, fs)
pub trait InstalledPluginsEnv: Send + Sync {
    fn get_plugins_directory(&self) -> PathBuf;
    fn get_plugin_cache_path(&self) -> PathBuf;
    fn get_versioned_cache_path(&self, plugin_id: &str, version: &str) -> PathBuf;
    fn get_original_cwd(&self) -> PathBuf;
    fn get_cwd(&self) -> PathBuf;
    fn get_head_for_dir(&self, dir_path: &Path) -> Option<String>;
    fn get_settings_enabled_plugins(&self) -> HashMap<String, serde_json::Value>;
    fn get_settings_for_source(&self, source: &str) -> Option<HashMap<String, serde_json::Value>>;
    fn get_plugin_by_id(&self, plugin_id: &str) -> Option<MarketplacePluginInfo>;
    fn parse_plugin_identifier(&self, plugin_id: &str) -> (Option<String>, Option<String>);
    fn setting_source_to_scope(&self, source: &str) -> PluginScope;

    // Filesystem operations
    fn read_file_sync(&self, path: &Path) -> Result<String>;
    fn write_file_sync(&self, path: &Path, content: &str) -> Result<()>;
    fn mkdir_sync(&self, path: &Path) -> Result<()>;
    fn rename_sync(&self, from: &Path, to: &Path) -> Result<()>;
    fn readdir_sync(&self, path: &Path) -> Result<Vec<DirEntry>>;
    fn rm_sync(&self, path: &Path) -> Result<()>;
    fn rmdir_sync(&self, path: &Path) -> Result<()>;
    fn file_exists(&self, path: &Path) -> bool;
}

/// Directory entry for readdir
pub struct DirEntry {
    pub name: String,
    pub is_directory: bool,
}

/// Info returned from marketplace lookup
pub struct MarketplacePluginInfo {
    pub source: MarketplaceSource,
    pub marketplace_install_location: PathBuf,
    pub version: Option<String>,
}

pub enum MarketplaceSource {
    Path(String),
    External,
}

/// Migration state
static MIGRATION_COMPLETED: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// Memoized cache of installed plugins data (V2 format)
static INSTALLED_PLUGINS_CACHE_V2: Lazy<Mutex<Option<InstalledPluginsFileV2>>> =
    Lazy::new(|| Mutex::new(None));

/// Session-level snapshot of installed plugins at startup
static IN_MEMORY_INSTALLED_PLUGINS: Lazy<Mutex<Option<InstalledPluginsFileV2>>> =
    Lazy::new(|| Mutex::new(None));

/// Get the path to the installed_plugins.json file
pub fn get_installed_plugins_file_path(env: &dyn InstalledPluginsEnv) -> PathBuf {
    env.get_plugins_directory().join("installed_plugins.json")
}

/// Get the path to the legacy installed_plugins_v2.json file.
/// Used only during migration to consolidate into single file.
pub fn get_installed_plugins_v2_file_path(env: &dyn InstalledPluginsEnv) -> PathBuf {
    env.get_plugins_directory()
        .join("installed_plugins_v2.json")
}

/// Clear the installed plugins cache.
/// Call this when the file is modified to force a reload.
///
/// Note: This also clears the in-memory session state (inMemoryInstalledPlugins).
pub fn clear_installed_plugins_cache() {
    *INSTALLED_PLUGINS_CACHE_V2.lock().unwrap() = None;
    *IN_MEMORY_INSTALLED_PLUGINS.lock().unwrap() = None;
    debug!("Cleared installed plugins cache");
}

/// Migrate to single plugin file format.
///
/// This consolidates the V1/V2 dual-file system into a single file:
/// 1. If installed_plugins_v2.json exists: rename to installed_plugins.json, delete V2 file
/// 2. If only installed_plugins.json exists with version=1: convert to version=2 in-place
/// 3. Clean up legacy non-versioned cache directories
///
/// This migration runs once per session at startup.
pub fn migrate_to_single_plugin_file(env: &dyn InstalledPluginsEnv) {
    let mut completed = MIGRATION_COMPLETED.lock().unwrap();
    if *completed {
        return;
    }

    let main_file_path = get_installed_plugins_file_path(env);
    let v2_file_path = get_installed_plugins_v2_file_path(env);

    // Case 1: Try renaming v2→main directly
    match env.rename_sync(&v2_file_path, &main_file_path) {
        Ok(()) => {
            debug!("Renamed installed_plugins_v2.json to installed_plugins.json");
            // Clean up legacy cache directories
            let v2_data = load_installed_plugins_v2(env);
            cleanup_legacy_cache(env, &v2_data);
            *completed = true;
            return;
        }
        Err(_) => {
            // v2 file doesn't exist, continue to case 2
        }
    }

    // Case 2: v2 absent — try reading main
    let main_content = match env.read_file_sync(&main_file_path) {
        Ok(content) => content,
        Err(_) => {
            // Case 3: No file exists - nothing to migrate
            *completed = true;
            return;
        }
    };

    match serde_json::from_str::<serde_json::Value>(&main_content) {
        Ok(main_data) => {
            let version = main_data
                .get("version")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);

            if version == 1 {
                // Convert V1 to V2 format in-place
                match serde_json::from_value::<InstalledPluginsFileV1>(main_data) {
                    Ok(v1_data) => {
                        let v2_data = migrate_v1_to_v2(env, &v1_data);
                        if let Ok(json_content) = serde_json::to_string_pretty(&v2_data) {
                            if let Err(e) = env.write_file_sync(&main_file_path, &json_content) {
                                debug!("Failed to write V2 data: {}", e);
                            } else {
                                debug!(
                                    "Converted installed_plugins.json from V1 to V2 format ({} plugins)",
                                    v1_data.plugins.len()
                                );
                                cleanup_legacy_cache(env, &v2_data);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Failed to parse V1 data: {}", e);
                    }
                }
            }
            // If version=2, already in correct format
        }
        Err(e) => {
            debug!("Failed to parse main file: {}", e);
        }
    }

    *completed = true;
}

/// Clean up legacy non-versioned cache directories.
///
/// Legacy cache structure: ~/.mossen/plugins/cache/{plugin-name}/
/// Versioned cache structure: ~/.mossen/plugins/cache/{marketplace}/{plugin}/{version}/
fn cleanup_legacy_cache(env: &dyn InstalledPluginsEnv, v2_data: &InstalledPluginsFileV2) {
    let cache_path = env.get_plugin_cache_path();

    // Collect all install paths that are referenced
    let mut referenced_paths = HashSet::new();
    for installations in v2_data.plugins.values() {
        for entry in installations {
            referenced_paths.insert(entry.install_path.clone());
        }
    }

    // List top-level directories in cache
    let entries = match env.readdir_sync(&cache_path) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for dirent in &entries {
        if !dirent.is_directory {
            continue;
        }

        let entry_path = cache_path.join(&dirent.name);

        // Check if this is a versioned cache or a legacy cache
        let sub_entries = match env.readdir_sync(&entry_path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        let has_versioned_structure = sub_entries.iter().any(|sub_dirent| {
            if !sub_dirent.is_directory {
                return false;
            }
            let sub_path = entry_path.join(&sub_dirent.name);
            // Check if subdir contains version directories
            match env.readdir_sync(&sub_path) {
                Ok(version_entries) => version_entries.iter().any(|v_dirent| v_dirent.is_directory),
                Err(_) => false,
            }
        });

        if has_versioned_structure {
            // This is a marketplace directory with versioned structure - skip
            continue;
        }

        // This is a legacy flat cache directory
        let entry_path_str = entry_path.to_string_lossy().to_string();
        if !referenced_paths.contains(&entry_path_str) {
            // Not referenced - safe to delete
            if let Err(e) = env.rm_sync(&entry_path) {
                debug!("Failed to remove legacy cache dir: {}", e);
            } else {
                debug!("Cleaned up legacy cache directory: {}", dirent.name);
            }
        }
    }
}

/// Reset migration state (for testing)
pub fn reset_migration_state() {
    *MIGRATION_COMPLETED.lock().unwrap() = false;
}

/// Read raw file data from installed_plugins.json.
/// Returns None if file doesn't exist.
fn read_installed_plugins_file_raw(
    env: &dyn InstalledPluginsEnv,
) -> Option<(u64, serde_json::Value)> {
    let file_path = get_installed_plugins_file_path(env);
    let file_content = env.read_file_sync(&file_path).ok()?;
    let data: serde_json::Value = serde_json::from_str(&file_content).ok()?;
    let version = data.get("version").and_then(|v| v.as_u64()).unwrap_or(1);
    Some((version, data))
}

/// Migrate V1 data to V2 format.
/// All V1 plugins are migrated to 'user' scope since V1 had no scope concept.
fn migrate_v1_to_v2(
    env: &dyn InstalledPluginsEnv,
    v1_data: &InstalledPluginsFileV1,
) -> InstalledPluginsFileV2 {
    let mut v2_plugins: HashMap<String, Vec<PluginInstallationEntry>> = HashMap::new();

    for (plugin_id, plugin) in &v1_data.plugins {
        let versioned_cache_path = env
            .get_versioned_cache_path(plugin_id, &plugin.version)
            .to_string_lossy()
            .to_string();

        v2_plugins.insert(
            plugin_id.clone(),
            vec![PluginInstallationEntry {
                scope: PluginScope::User,
                install_path: versioned_cache_path,
                version: Some(plugin.version.clone()),
                installed_at: Some(plugin.installed_at.clone()),
                last_updated: Some(plugin.last_updated.clone().unwrap_or_default()),
                git_commit_sha: plugin.git_commit_sha.clone(),
                project_path: None,
            }],
        );
    }

    InstalledPluginsFileV2 {
        version: 2,
        plugins: v2_plugins,
    }
}

/// Load installed plugins in V2 format.
///
/// Reads from installed_plugins.json. If file has version=1,
/// converts to V2 format in memory.
pub fn load_installed_plugins_v2(env: &dyn InstalledPluginsEnv) -> InstalledPluginsFileV2 {
    // Return cached V2 data if available
    let cache = INSTALLED_PLUGINS_CACHE_V2.lock().unwrap();
    if let Some(ref cached) = *cache {
        return cached.clone();
    }
    drop(cache);

    let file_path = get_installed_plugins_file_path(env);

    let result = match read_installed_plugins_file_raw(env) {
        Some((version, data)) => {
            if version == 2 {
                match serde_json::from_value::<InstalledPluginsFileV2>(data) {
                    Ok(validated) => {
                        debug!(
                            "Loaded {} installed plugins from {:?}",
                            validated.plugins.len(),
                            file_path
                        );
                        validated
                    }
                    Err(e) => {
                        debug!("Failed to parse V2 data: {}. Starting with empty state.", e);
                        InstalledPluginsFileV2 {
                            version: 2,
                            plugins: HashMap::new(),
                        }
                    }
                }
            } else {
                // V1 format - convert to V2
                match serde_json::from_value::<InstalledPluginsFileV1>(data) {
                    Ok(v1_validated) => {
                        let v2_data = migrate_v1_to_v2(env, &v1_validated);
                        debug!(
                            "Loaded and converted {} plugins from V1 format",
                            v1_validated.plugins.len()
                        );
                        v2_data
                    }
                    Err(e) => {
                        debug!("Failed to parse V1 data: {}. Starting with empty state.", e);
                        InstalledPluginsFileV2 {
                            version: 2,
                            plugins: HashMap::new(),
                        }
                    }
                }
            }
        }
        None => {
            debug!("installed_plugins.json doesn't exist, returning empty V2 object");
            InstalledPluginsFileV2 {
                version: 2,
                plugins: HashMap::new(),
            }
        }
    };

    *INSTALLED_PLUGINS_CACHE_V2.lock().unwrap() = Some(result.clone());
    result
}

/// Save installed plugins in V2 format to installed_plugins.json.
fn save_installed_plugins_v2(
    env: &dyn InstalledPluginsEnv,
    data: &InstalledPluginsFileV2,
) -> Result<()> {
    let file_path = get_installed_plugins_file_path(env);
    let _ = env.mkdir_sync(&env.get_plugins_directory());

    let json_content = serde_json::to_string_pretty(data)?;
    env.write_file_sync(&file_path, &json_content)?;

    // Update cache
    *INSTALLED_PLUGINS_CACHE_V2.lock().unwrap() = Some(data.clone());

    debug!(
        "Saved {} installed plugins to {:?}",
        data.plugins.len(),
        file_path
    );
    Ok(())
}

/// Add or update a plugin installation entry at a specific scope.
pub fn add_plugin_installation(
    env: &dyn InstalledPluginsEnv,
    plugin_id: &str,
    scope: PersistableScope,
    install_path: &str,
    metadata: &PartialInstallationEntry,
    project_path: Option<&str>,
) -> Result<()> {
    let mut data = load_installed_plugins_from_disk(env);

    let installations = data.plugins.entry(plugin_id.to_string()).or_default();

    // Find existing entry for this scope+projectPath
    let existing_index = installations
        .iter()
        .position(|entry| entry.scope == scope && entry.project_path.as_deref() == project_path);

    let now = chrono::Utc::now().to_rfc3339();
    let new_entry = PluginInstallationEntry {
        scope,
        install_path: install_path.to_string(),
        version: metadata.version.clone(),
        installed_at: Some(metadata.installed_at.clone().unwrap_or_else(|| now.clone())),
        last_updated: Some(now),
        git_commit_sha: metadata.git_commit_sha.clone(),
        project_path: project_path.map(|s| s.to_string()),
    };

    if let Some(idx) = existing_index {
        installations[idx] = new_entry;
        debug!(
            "Updated installation for {} at scope {:?}",
            plugin_id, scope
        );
    } else {
        installations.push(new_entry);
        debug!("Added installation for {} at scope {:?}", plugin_id, scope);
    }

    save_installed_plugins_v2(env, &data)
}

/// Partial metadata for installation entry
pub struct PartialInstallationEntry {
    pub version: Option<String>,
    pub installed_at: Option<String>,
    pub git_commit_sha: Option<String>,
}

/// Remove a plugin installation entry from a specific scope.
pub fn remove_plugin_installation(
    env: &dyn InstalledPluginsEnv,
    plugin_id: &str,
    scope: PersistableScope,
    project_path: Option<&str>,
) -> Result<()> {
    let mut data = load_installed_plugins_from_disk(env);
    let installations = match data.plugins.get_mut(plugin_id) {
        Some(i) => i,
        None => return Ok(()),
    };

    installations
        .retain(|entry| !(entry.scope == scope && entry.project_path.as_deref() == project_path));

    // Remove plugin entirely if no installations left
    if installations.is_empty() {
        data.plugins.remove(plugin_id);
    }

    save_installed_plugins_v2(env, &data)?;
    debug!(
        "Removed installation for {} at scope {:?}",
        plugin_id, scope
    );
    Ok(())
}

/// Get the in-memory installed plugins (session state).
pub fn get_in_memory_installed_plugins(env: &dyn InstalledPluginsEnv) -> InstalledPluginsFileV2 {
    let mut mem = IN_MEMORY_INSTALLED_PLUGINS.lock().unwrap();
    if mem.is_none() {
        *mem = Some(load_installed_plugins_v2(env));
    }
    mem.clone().unwrap()
}

/// Load installed plugins directly from disk, bypassing all caches.
pub fn load_installed_plugins_from_disk(env: &dyn InstalledPluginsEnv) -> InstalledPluginsFileV2 {
    match read_installed_plugins_file_raw(env) {
        Some((version, data)) => {
            if version == 2 {
                serde_json::from_value::<InstalledPluginsFileV2>(data).unwrap_or_else(|_| {
                    InstalledPluginsFileV2 {
                        version: 2,
                        plugins: HashMap::new(),
                    }
                })
            } else {
                match serde_json::from_value::<InstalledPluginsFileV1>(data) {
                    Ok(v1_data) => migrate_v1_to_v2(env, &v1_data),
                    Err(_) => InstalledPluginsFileV2 {
                        version: 2,
                        plugins: HashMap::new(),
                    },
                }
            }
        }
        None => InstalledPluginsFileV2 {
            version: 2,
            plugins: HashMap::new(),
        },
    }
}

/// Update a plugin's install path on disk only, without modifying in-memory state.
pub fn update_installation_path_on_disk(
    env: &dyn InstalledPluginsEnv,
    plugin_id: &str,
    scope: PersistableScope,
    project_path: Option<&str>,
    new_path: &str,
    new_version: &str,
    git_commit_sha: Option<&str>,
) {
    let mut disk_data = load_installed_plugins_from_disk(env);
    let installations = match disk_data.plugins.get_mut(plugin_id) {
        Some(i) => i,
        None => {
            debug!(
                "Cannot update {} on disk: plugin not found in installed plugins",
                plugin_id
            );
            return;
        }
    };

    let entry = installations
        .iter_mut()
        .find(|e| e.scope == scope && e.project_path.as_deref() == project_path);

    if let Some(entry) = entry {
        entry.install_path = new_path.to_string();
        entry.version = Some(new_version.to_string());
        entry.last_updated = Some(chrono::Utc::now().to_rfc3339());
        if let Some(sha) = git_commit_sha {
            entry.git_commit_sha = Some(sha.to_string());
        }

        let file_path = get_installed_plugins_file_path(env);
        if let Ok(json_content) = serde_json::to_string_pretty(&disk_data) {
            let _ = env.write_file_sync(&file_path, &json_content);
        }

        // Clear cache since disk changed, but do NOT update inMemoryInstalledPlugins
        *INSTALLED_PLUGINS_CACHE_V2.lock().unwrap() = None;

        debug!(
            "Updated {} on disk to version {} at {}",
            plugin_id, new_version, new_path
        );
    } else {
        debug!(
            "Cannot update {} on disk: no installation for scope {:?}",
            plugin_id, scope
        );
    }
}

/// Check if there are pending updates (disk differs from memory).
pub fn has_pending_updates(env: &dyn InstalledPluginsEnv) -> bool {
    let memory_state = get_in_memory_installed_plugins(env);
    let disk_state = load_installed_plugins_from_disk(env);

    for (plugin_id, disk_installations) in &disk_state.plugins {
        let memory_installations = match memory_state.plugins.get(plugin_id) {
            Some(i) => i,
            None => continue,
        };

        for disk_entry in disk_installations {
            let memory_entry = memory_installations
                .iter()
                .find(|m| m.scope == disk_entry.scope && m.project_path == disk_entry.project_path);
            if let Some(mem) = memory_entry {
                if mem.install_path != disk_entry.install_path {
                    return true;
                }
            }
        }
    }

    false
}

/// Get the count of pending updates.
pub fn get_pending_update_count(env: &dyn InstalledPluginsEnv) -> usize {
    let memory_state = get_in_memory_installed_plugins(env);
    let disk_state = load_installed_plugins_from_disk(env);
    let mut count = 0;

    for (plugin_id, disk_installations) in &disk_state.plugins {
        let memory_installations = match memory_state.plugins.get(plugin_id) {
            Some(i) => i,
            None => continue,
        };

        for disk_entry in disk_installations {
            let memory_entry = memory_installations
                .iter()
                .find(|m| m.scope == disk_entry.scope && m.project_path == disk_entry.project_path);
            if let Some(mem) = memory_entry {
                if mem.install_path != disk_entry.install_path {
                    count += 1;
                }
            }
        }
    }

    count
}

/// Pending update detail info
#[derive(Debug, Clone)]
pub struct PendingUpdateDetail {
    pub plugin_id: String,
    pub scope: String,
    pub old_version: String,
    pub new_version: String,
}

/// Get details about pending updates for display.
pub fn get_pending_updates_details(env: &dyn InstalledPluginsEnv) -> Vec<PendingUpdateDetail> {
    let memory_state = get_in_memory_installed_plugins(env);
    let disk_state = load_installed_plugins_from_disk(env);
    let mut updates = Vec::new();

    for (plugin_id, disk_installations) in &disk_state.plugins {
        let memory_installations = match memory_state.plugins.get(plugin_id) {
            Some(i) => i,
            None => continue,
        };

        for disk_entry in disk_installations {
            let memory_entry = memory_installations
                .iter()
                .find(|m| m.scope == disk_entry.scope && m.project_path == disk_entry.project_path);
            if let Some(mem) = memory_entry {
                if mem.install_path != disk_entry.install_path {
                    updates.push(PendingUpdateDetail {
                        plugin_id: plugin_id.clone(),
                        scope: format!("{:?}", disk_entry.scope),
                        old_version: mem.version.clone().unwrap_or_else(|| "unknown".to_string()),
                        new_version: disk_entry
                            .version
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                    });
                }
            }
        }
    }

    updates
}

/// Reset the in-memory session state.
pub fn reset_in_memory_state() {
    *IN_MEMORY_INSTALLED_PLUGINS.lock().unwrap() = None;
}

/// Initialize the versioned plugins system.
pub async fn initialize_versioned_plugins(env: &dyn InstalledPluginsEnv) -> Result<()> {
    // Step 1: Migrate to single file format
    migrate_to_single_plugin_file(env);

    // Step 2: Sync enabledPlugins from settings.json to installed_plugins.json
    if let Err(e) = migrate_from_enabled_plugins(env).await {
        debug!("Migration from enabled plugins failed: {}", e);
    }

    // Step 3: Initialize in-memory session state
    let data = get_in_memory_installed_plugins(env);
    debug!(
        "Initialized versioned plugins system with {} plugins",
        data.plugins.len()
    );
    Ok(())
}

/// Remove all plugin entries belonging to a specific marketplace.
pub fn remove_all_plugins_for_marketplace(
    env: &dyn InstalledPluginsEnv,
    marketplace_name: &str,
) -> (Vec<String>, Vec<String>) {
    if marketplace_name.is_empty() {
        return (vec![], vec![]);
    }

    let mut data = load_installed_plugins_from_disk(env);
    let suffix = format!("@{}", marketplace_name);
    let mut orphaned_paths: HashSet<String> = HashSet::new();
    let mut removed_plugin_ids: Vec<String> = Vec::new();

    let plugin_ids: Vec<String> = data.plugins.keys().cloned().collect();
    for plugin_id in plugin_ids {
        if !plugin_id.ends_with(&suffix) {
            continue;
        }

        if let Some(installations) = data.plugins.get(&plugin_id) {
            for entry in installations {
                if !entry.install_path.is_empty() {
                    orphaned_paths.insert(entry.install_path.clone());
                }
            }
        }

        data.plugins.remove(&plugin_id);
        removed_plugin_ids.push(plugin_id.clone());
        debug!(
            "Removed installed plugin for marketplace removal: {}",
            plugin_id
        );
    }

    if !removed_plugin_ids.is_empty() {
        let _ = save_installed_plugins_v2(env, &data);
    }

    (orphaned_paths.into_iter().collect(), removed_plugin_ids)
}

/// Check if an installation is relevant to the current project.
pub fn is_installation_relevant_to_current_project(
    env: &dyn InstalledPluginsEnv,
    inst: &PluginInstallationEntry,
) -> bool {
    inst.scope == PluginScope::User
        || inst.scope == PluginScope::Managed
        || inst.project_path.as_deref() == Some(env.get_original_cwd().to_string_lossy().as_ref())
}

/// Check if a plugin is installed in a way relevant to the current project.
pub fn is_plugin_installed(env: &dyn InstalledPluginsEnv, plugin_id: &str) -> bool {
    let v2_data = load_installed_plugins_v2(env);
    let installations = match v2_data.plugins.get(plugin_id) {
        Some(i) if !i.is_empty() => i,
        _ => return false,
    };

    if !installations
        .iter()
        .any(|inst| is_installation_relevant_to_current_project(env, inst))
    {
        return false;
    }

    // Check settings.enabledPlugins
    let enabled_plugins = env.get_settings_enabled_plugins();
    enabled_plugins.contains_key(plugin_id)
}

/// True only if the plugin has a USER or MANAGED scope installation.
pub fn is_plugin_globally_installed(env: &dyn InstalledPluginsEnv, plugin_id: &str) -> bool {
    let v2_data = load_installed_plugins_v2(env);
    let installations = match v2_data.plugins.get(plugin_id) {
        Some(i) if !i.is_empty() => i,
        _ => return false,
    };

    let has_global_entry = installations
        .iter()
        .any(|entry| entry.scope == PluginScope::User || entry.scope == PluginScope::Managed);

    if !has_global_entry {
        return false;
    }

    let enabled_plugins = env.get_settings_enabled_plugins();
    enabled_plugins.contains_key(plugin_id)
}

/// Add or update a plugin's installation metadata.
pub fn add_installed_plugin(
    env: &dyn InstalledPluginsEnv,
    plugin_id: &str,
    metadata: &InstalledPlugin,
    scope: PersistableScope,
    project_path: Option<&str>,
) -> Result<()> {
    let mut v2_data = load_installed_plugins_from_disk(env);
    let v2_entry = PluginInstallationEntry {
        scope,
        install_path: metadata.install_path.clone(),
        version: Some(metadata.version.clone()),
        installed_at: Some(metadata.installed_at.clone()),
        last_updated: metadata.last_updated.clone(),
        git_commit_sha: metadata.git_commit_sha.clone(),
        project_path: project_path.map(|s| s.to_string()),
    };

    let installations = v2_data.plugins.entry(plugin_id.to_string()).or_default();

    let existing_index = installations
        .iter()
        .position(|entry| entry.scope == scope && entry.project_path.as_deref() == project_path);

    let is_update = existing_index.is_some();
    if let Some(idx) = existing_index {
        installations[idx] = v2_entry;
    } else {
        installations.push(v2_entry);
    }

    save_installed_plugins_v2(env, &v2_data)?;

    debug!(
        "{} installed plugin: {} (scope: {:?})",
        if is_update { "Updated" } else { "Added" },
        plugin_id,
        scope
    );
    Ok(())
}

/// Remove a plugin from the installed plugins registry.
pub fn remove_installed_plugin(
    env: &dyn InstalledPluginsEnv,
    plugin_id: &str,
) -> Option<InstalledPlugin> {
    let mut v2_data = load_installed_plugins_from_disk(env);
    let installations = v2_data.plugins.get(plugin_id)?;

    if installations.is_empty() {
        return None;
    }

    let first_install = &installations[0];
    let metadata = InstalledPlugin {
        version: first_install
            .version
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        installed_at: first_install
            .installed_at
            .clone()
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        last_updated: first_install.last_updated.clone(),
        install_path: first_install.install_path.clone(),
        git_commit_sha: first_install.git_commit_sha.clone(),
    };

    v2_data.plugins.remove(plugin_id);
    let _ = save_installed_plugins_v2(env, &v2_data);

    debug!("Removed installed plugin: {}", plugin_id);
    Some(metadata)
}

/// Delete a plugin's cache directory.
pub fn delete_plugin_cache(env: &dyn InstalledPluginsEnv, install_path: &str) -> Result<()> {
    let path = Path::new(install_path);
    env.rm_sync(path)?;
    debug!("Deleted plugin cache at {}", install_path);

    // Clean up empty parent plugin directory
    let cache_path = env.get_plugin_cache_path();
    let cache_path_str = cache_path.to_string_lossy().to_string();
    if install_path.contains("/cache/") && install_path.starts_with(&cache_path_str) {
        if let Some(plugin_dir) = path.parent() {
            let plugin_dir_str = plugin_dir.to_string_lossy().to_string();
            if plugin_dir_str != cache_path_str && plugin_dir_str.starts_with(&cache_path_str) {
                if let Ok(contents) = env.readdir_sync(plugin_dir) {
                    if contents.is_empty() {
                        let _ = env.rmdir_sync(plugin_dir);
                        debug!("Deleted empty plugin directory at {:?}", plugin_dir);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Get the git commit SHA from a git repository directory.
pub fn get_git_commit_sha(env: &dyn InstalledPluginsEnv, dir_path: &Path) -> Option<String> {
    env.get_head_for_dir(dir_path)
}

/// Try to read version from plugin manifest.
fn get_plugin_version_from_manifest(
    env: &dyn InstalledPluginsEnv,
    plugin_cache_path: &Path,
    plugin_id: &str,
) -> String {
    let manifest_path = plugin_cache_path.join(".mossen-plugin").join("plugin.json");

    match env.read_file_sync(&manifest_path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(manifest) => manifest
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            Err(_) => {
                debug!("Could not read version from manifest for {}", plugin_id);
                "unknown".to_string()
            }
        },
        Err(_) => {
            debug!("Could not read version from manifest for {}", plugin_id);
            "unknown".to_string()
        }
    }
}

/// Sync installed_plugins.json with enabledPlugins from settings.
pub async fn migrate_from_enabled_plugins(env: &dyn InstalledPluginsEnv) -> Result<()> {
    let enabled_plugins = env.get_settings_enabled_plugins();

    if enabled_plugins.is_empty() {
        return Ok(());
    }

    // Check if main file exists and has V2 format
    let raw_file_data = read_installed_plugins_file_raw(env);
    let file_exists = raw_file_data.is_some();
    let is_v2_format = raw_file_data
        .as_ref()
        .map(|(v, _)| *v == 2)
        .unwrap_or(false);

    // If file exists with V2 format, check if we can skip the expensive migration
    if is_v2_format {
        if let Some((_, ref data)) = raw_file_data {
            if let Ok(existing_data) =
                serde_json::from_value::<InstalledPluginsFileV2>(data.clone())
            {
                let all_plugins_exist =
                    enabled_plugins
                        .keys()
                        .filter(|id| id.contains('@'))
                        .all(|id| {
                            existing_data
                                .plugins
                                .get(id)
                                .map(|i| !i.is_empty())
                                .unwrap_or(false)
                        });

                if all_plugins_exist {
                    debug!("All plugins already exist, skipping migration");
                    return Ok(());
                }
            }
        }
    }

    debug!(
        "{}",
        if file_exists {
            "Syncing installed_plugins.json with enabledPlugins from all settings.json files"
        } else {
            "Creating installed_plugins.json from settings.json files"
        }
    );

    let now = chrono::Utc::now().to_rfc3339();
    let project_path = env.get_cwd();

    // Step 1: Build a map of pluginId -> scope from all settings.json files
    let mut plugin_scope_from_settings: HashMap<String, (PluginScope, Option<String>)> =
        HashMap::new();

    let setting_sources = ["userSettings", "projectSettings", "localSettings"];
    for source in &setting_sources {
        if let Some(source_enabled_plugins) = env.get_settings_for_source(source) {
            for plugin_id in source_enabled_plugins.keys() {
                if !plugin_id.contains('@') {
                    continue;
                }
                let scope = env.setting_source_to_scope(source);
                let pp = if scope == PluginScope::User {
                    None
                } else {
                    Some(project_path.to_string_lossy().to_string())
                };
                plugin_scope_from_settings.insert(plugin_id.clone(), (scope, pp));
            }
        }
    }

    // Step 2: Start with existing data
    let mut v2_plugins: HashMap<String, Vec<PluginInstallationEntry>> = if file_exists {
        let existing_data = load_installed_plugins_v2(env);
        existing_data.plugins
    } else {
        HashMap::new()
    };

    // Step 3: Update V2 scopes based on settings.json
    let mut updated_count = 0;
    let mut added_count = 0;

    for (plugin_id, (scope, scope_project_path)) in &plugin_scope_from_settings {
        let existing_installations = v2_plugins.get_mut(plugin_id);

        if let Some(installations) = existing_installations {
            if !installations.is_empty() {
                let existing_entry = &mut installations[0];
                if existing_entry.scope != *scope
                    || existing_entry.project_path.as_deref() != scope_project_path.as_deref()
                {
                    existing_entry.scope = *scope;
                    existing_entry.project_path = scope_project_path.clone();
                    existing_entry.last_updated = Some(now.clone());
                    updated_count += 1;
                    debug!(
                        "Updated {} scope to {:?} (settings.json is source of truth)",
                        plugin_id, scope
                    );
                }
                continue;
            }
        }

        // Plugin not in V2 - try to add it by looking up in marketplace
        let (plugin_name, marketplace) = env.parse_plugin_identifier(plugin_id);

        let (plugin_name, _marketplace) = match (plugin_name, marketplace) {
            (Some(n), Some(m)) => (n, m),
            _ => continue,
        };

        if let Some(plugin_info) = env.get_plugin_by_id(plugin_id) {
            let (_install_path, mut version, git_commit_sha) = match plugin_info.source {
                MarketplaceSource::Path(ref source_path) => {
                    let ip = plugin_info.marketplace_install_location.join(source_path);
                    let v = get_plugin_version_from_manifest(env, &ip, plugin_id);
                    let sha = get_git_commit_sha(env, &ip);
                    (ip.to_string_lossy().to_string(), v, sha)
                }
                MarketplaceSource::External => {
                    let cache_path = env.get_plugin_cache_path();
                    let sanitized_name = plugin_name
                        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-");
                    let plugin_cache_path = cache_path.join(&sanitized_name);

                    let dir_entries = match env.readdir_sync(&plugin_cache_path) {
                        Ok(entries) => entries,
                        Err(_) => {
                            debug!("External plugin {} not in cache, skipping", plugin_id);
                            continue;
                        }
                    };

                    let mut v = "unknown".to_string();
                    if dir_entries.iter().any(|e| e.name == ".mossen-plugin") {
                        v = get_plugin_version_from_manifest(env, &plugin_cache_path, plugin_id);
                    }

                    let sha = get_git_commit_sha(env, &plugin_cache_path);
                    (plugin_cache_path.to_string_lossy().to_string(), v, sha)
                }
            };

            if version == "unknown" {
                if let Some(ref pv) = plugin_info.version {
                    version = pv.clone();
                }
            }
            if version == "unknown" {
                if let Some(ref sha) = git_commit_sha {
                    version = sha.chars().take(12).collect();
                }
            }

            let versioned_path = env
                .get_versioned_cache_path(plugin_id, &version)
                .to_string_lossy()
                .to_string();

            v2_plugins.insert(
                plugin_id.clone(),
                vec![PluginInstallationEntry {
                    scope: *scope,
                    install_path: versioned_path,
                    version: Some(version),
                    installed_at: Some(now.clone()),
                    last_updated: Some(now.clone()),
                    git_commit_sha,
                    project_path: scope_project_path.clone(),
                }],
            );

            added_count += 1;
            debug!("Added {} with scope {:?}", plugin_id, scope);
        } else {
            debug!(
                "Plugin {} not found in any marketplace, skipping",
                plugin_id
            );
        }
    }

    // Step 4: Save to single file (V2 format)
    if !file_exists || updated_count > 0 || added_count > 0 {
        let v2_data = InstalledPluginsFileV2 {
            version: 2,
            plugins: v2_plugins,
        };
        save_installed_plugins_v2(env, &v2_data)?;
        debug!(
            "Sync completed: {} added, {} updated in installed_plugins.json",
            added_count, updated_count
        );
    }

    Ok(())
}
