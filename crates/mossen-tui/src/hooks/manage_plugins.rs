//! Manage plugins hook (useManagePlugins.ts).
//!
//! Manages the installation, update, and removal of plugins.

use std::collections::HashMap;

/// Plugin installation status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginStatus {
    NotInstalled,
    Installing,
    Installed,
    Updating,
    Removing,
    Error(String),
}

/// Information about an installed plugin.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub status: PluginStatus,
    pub enabled: bool,
    pub auto_update: bool,
}

/// State for plugin management.
#[derive(Debug, Clone)]
pub struct ManagePluginsState {
    pub plugins: HashMap<String, PluginEntry>,
    pub pending_operations: Vec<String>,
}

impl ManagePluginsState {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            pending_operations: Vec::new(),
        }
    }

    /// Register a plugin.
    pub fn register(&mut self, plugin: PluginEntry) {
        self.plugins.insert(plugin.id.clone(), plugin);
    }

    /// Start installing a plugin.
    pub fn install(&mut self, id: &str) {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.status = PluginStatus::Installing;
            self.pending_operations.push(id.to_string());
        }
    }

    /// Mark installation as complete.
    pub fn install_complete(&mut self, id: &str, version: String) {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.status = PluginStatus::Installed;
            plugin.version = version;
        }
        self.pending_operations.retain(|i| i != id);
    }

    /// Start removing a plugin.
    pub fn remove(&mut self, id: &str) {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.status = PluginStatus::Removing;
            self.pending_operations.push(id.to_string());
        }
    }

    /// Mark removal as complete.
    pub fn remove_complete(&mut self, id: &str) {
        self.plugins.remove(id);
        self.pending_operations.retain(|i| i != id);
    }

    /// Toggle plugin enabled state.
    pub fn toggle_enabled(&mut self, id: &str) -> bool {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.enabled = !plugin.enabled;
            plugin.enabled
        } else {
            false
        }
    }

    /// Mark a plugin operation as failed.
    pub fn mark_error(&mut self, id: &str, error: String) {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.status = PluginStatus::Error(error);
        }
        self.pending_operations.retain(|i| i != id);
    }

    /// Get all installed plugins.
    pub fn installed(&self) -> Vec<&PluginEntry> {
        self.plugins
            .values()
            .filter(|p| p.status == PluginStatus::Installed)
            .collect()
    }

    /// Check if any operations are pending.
    pub fn has_pending(&self) -> bool {
        !self.pending_operations.is_empty()
    }
}

impl Default for ManagePluginsState {
    fn default() -> Self {
        Self::new()
    }
}
