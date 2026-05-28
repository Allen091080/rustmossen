//! PluginInstallationStatus notification (plugin_installation_status.ts).
//! Shows plugin installation progress and status.

#[derive(Debug, Clone)]
pub struct PluginInstallationStatusNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl PluginInstallationStatusNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "plugin-installation-status".to_string(),
            message: None,
        }
    }

    /// Check conditions and set notification message if needed.
    pub fn check(&mut self, should_show: bool, message: &str) {
        if should_show && !self.shown && !self.dismissed {
            self.shown = true;
            self.message = Some(message.to_string());
        }
    }

    /// Dismiss the notification.
    pub fn dismiss(&mut self) {
        self.dismissed = true;
        self.message = None;
    }

    /// Get the notification message if active.
    pub fn active_message(&self) -> Option<&str> {
        if self.shown && !self.dismissed {
            self.message.as_deref()
        } else {
            None
        }
    }

    /// Reset state for re-evaluation.
    pub fn reset(&mut self) {
        self.shown = false;
        self.message = None;
    }
}

impl Default for PluginInstallationStatusNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginInstallStatus {
    Pending,
    Installed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct PluginInstallationSnapshot {
    pub marketplace_statuses: Vec<PluginInstallStatus>,
    pub plugin_statuses: Vec<PluginInstallStatus>,
}

/// `usePluginInstallationStatus` — pure-logic translation. Returns a
/// notification when any plugin or marketplace failed.
///
/// TS source: `usePluginInstallationStatus()`.
pub fn use_plugin_installation_status(
    is_remote_mode: bool,
    status: Option<&PluginInstallationSnapshot>,
) -> Option<(String, String)> {
    if is_remote_mode {
        return None;
    }
    let Some(s) = status else { return None };
    let failed_marketplaces = s
        .marketplace_statuses
        .iter()
        .filter(|&&st| st == PluginInstallStatus::Failed)
        .count();
    let failed_plugins = s
        .plugin_statuses
        .iter()
        .filter(|&&st| st == PluginInstallStatus::Failed)
        .count();
    let total = failed_marketplaces + failed_plugins;
    if total == 0 {
        return None;
    }
    let word = if total == 1 { "plugin" } else { "plugins" };
    Some((
        "plugin-install-failed".to_string(),
        format!("{} {} failed to install · /plugin for details", total, word),
    ))
}
