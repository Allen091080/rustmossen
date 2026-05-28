//! InstallMessages notification (install_messages.ts).
//! Shows messages during plugin/extension installation.

#[derive(Debug, Clone)]
pub struct InstallMessagesNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl InstallMessagesNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "install-messages".to_string(),
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

impl Default for InstallMessagesNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMessageKind {
    Info,
    Warning,
    Error,
    Path,
    Alias,
}

#[derive(Debug, Clone)]
pub struct InstallMessage {
    pub kind: InstallMessageKind,
    pub message: String,
    pub user_action_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallMessageEvent {
    pub key: String,
    pub text: String,
    pub color: String,
    pub priority: String,
}

/// `useInstallMessages` — pure-logic translation. Maps install-message
/// results to notification events ready for the notification store.
///
/// TS source: `useInstallMessages()`.
pub fn use_install_messages(messages: &[InstallMessage]) -> Vec<InstallMessageEvent> {
    messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let priority = if m.kind == InstallMessageKind::Error || m.user_action_required {
                "high"
            } else if m.kind == InstallMessageKind::Path || m.kind == InstallMessageKind::Alias {
                "medium"
            } else {
                "low"
            };
            let color = if m.kind == InstallMessageKind::Error {
                "error"
            } else {
                "warning"
            };
            let kind_name = match m.kind {
                InstallMessageKind::Info => "info",
                InstallMessageKind::Warning => "warning",
                InstallMessageKind::Error => "error",
                InstallMessageKind::Path => "path",
                InstallMessageKind::Alias => "alias",
            };
            InstallMessageEvent {
                key: format!("install-message-{}-{}", i, kind_name),
                text: m.message.clone(),
                color: color.to_string(),
                priority: priority.to_string(),
            }
        })
        .collect()
}
