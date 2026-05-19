//! LspInitialization notification (lsp_initialization.ts).
//! Notification for LSP server initialization progress.

#[derive(Debug, Clone)]
pub struct LspInitializationNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl LspInitializationNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "lsp-initialization".to_string(),
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

impl Default for LspInitializationNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Should this LSP error message be suppressed from user-visible
/// notifications? ENOENT / "command not found" / "not found in path" /
/// "spawn ... not found" errors are treated as soft failures.
///
/// TS source: `shouldSuppressLspNotification(errorMessage)`.
pub fn should_suppress_lsp_notification(error_message: &str) -> bool {
    let normalized = error_message.to_lowercase();
    normalized.contains("enoent")
        || normalized.contains("command not found")
        || normalized.contains("not found in path")
        || (normalized.contains("spawn ") && normalized.contains(" not found"))
}

/// Input snapshot for the LSP initialization notification hook.
#[derive(Debug, Clone)]
pub struct LspInitializationInputs {
    pub is_remote_mode: bool,
    pub is_scroll_draining: bool,
    pub should_poll: bool,
    pub initialization_status: LspInitializationStatus,
    pub server_errors: Vec<(String, String)>,
}

/// Status of the LSP manager itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspInitializationStatus {
    NotStarted,
    Pending,
    Ready,
    Failed { error: String },
}

/// One LSP-related event to surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspNotificationEvent {
    pub source: String,
    pub error: String,
    pub display_name: String,
    pub suppressed: bool,
}

/// `useLspInitializationNotification` — translated as a pure function
/// that returns the events the caller should surface (and whether
/// polling should stop).
///
/// TS source: `useLspInitializationNotification()`.
pub fn use_lsp_initialization_notification(inputs: &LspInitializationInputs) -> (Vec<LspNotificationEvent>, bool) {
    let mut out = Vec::new();
    let mut stop_polling = false;
    if inputs.is_remote_mode || inputs.is_scroll_draining || !inputs.should_poll {
        return (out, false);
    }
    match &inputs.initialization_status {
        LspInitializationStatus::Failed { error } => {
            let suppressed = should_suppress_lsp_notification(error);
            out.push(LspNotificationEvent {
                source: "lsp-manager".to_string(),
                error: error.clone(),
                display_name: "lsp-manager".to_string(),
                suppressed,
            });
            stop_polling = true;
        }
        LspInitializationStatus::Pending | LspInitializationStatus::NotStarted => {
            return (out, false);
        }
        LspInitializationStatus::Ready => {}
    }
    for (server, err) in &inputs.server_errors {
        let display = if let Some(rest) = server.strip_prefix("plugin:") {
            rest.to_string()
        } else {
            server.clone()
        };
        out.push(LspNotificationEvent {
            source: server.clone(),
            error: err.clone(),
            display_name: display,
            suppressed: should_suppress_lsp_notification(err),
        });
    }
    (out, stop_polling)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppresses_enoent() {
        assert!(should_suppress_lsp_notification("Error: spawn enoent foo"));
        assert!(should_suppress_lsp_notification("command not found"));
        assert!(should_suppress_lsp_notification("spawn typescript-language-server not found"));
        assert!(!should_suppress_lsp_notification("unexpected response"));
    }
}
