//! PluginAutoupdate notification (plugin_autoupdate.ts).
//! Notification when plugins are auto-updated.

#[derive(Debug, Clone)]
pub struct PluginAutoupdateNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl PluginAutoupdateNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "plugin-autoupdate".to_string(),
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

impl Default for PluginAutoupdateNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// `usePluginAutoupdateNotification` — pure-logic translation. Returns
/// the notification to surface when plugins have been auto-updated.
///
/// TS source: `usePluginAutoupdateNotification()`. The plugin IDs may
/// contain an `@version` suffix; only the bare name is shown.
pub fn use_plugin_autoupdate_notification(
    is_remote_mode: bool,
    updated_plugins: &[String],
) -> Option<(String, String)> {
    if is_remote_mode || updated_plugins.is_empty() {
        return None;
    }
    let plugin_names: Vec<String> = updated_plugins
        .iter()
        .map(|id| match id.find('@') {
            Some(idx) if idx > 0 => id[..idx].to_string(),
            _ => id.clone(),
        })
        .collect();
    let count = plugin_names.len();
    let display_names = if count <= 2 {
        plugin_names.join(" and ")
    } else {
        format!("{} plugins", count)
    };
    let heading = if count == 1 { "Plugin" } else { "Plugins" };
    Some((
        "plugin-autoupdate-restart".to_string(),
        format!(
            "{} updated: {} · Run /reload-plugins to apply",
            heading, display_names
        ),
    ))
}
