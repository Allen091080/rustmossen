//! SettingsErrors notification (settings_errors.ts).
//! Shows notification for invalid settings configuration.

#[derive(Debug, Clone)]
pub struct SettingsErrorsNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl SettingsErrorsNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "settings-errors".to_string(),
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

impl Default for SettingsErrorsNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// One settings validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsValidationError {
    pub path: String,
    pub message: String,
}

/// `useSettingsErrors` — pure-logic translation. Returns the notification
/// to surface (and a flag indicating whether the prior notification
/// should be removed).
///
/// TS source: `useSettingsErrors()`.
pub fn use_settings_errors(
    is_remote_mode: bool,
    errors: &[SettingsValidationError],
) -> Option<(String, String)> {
    if is_remote_mode {
        return None;
    }
    if errors.is_empty() {
        return None;
    }
    let n = errors.len();
    let word = if n == 1 { "issue" } else { "issues" };
    Some((
        "settings-errors".to_string(),
        format!("Found {} settings {} · /doctor for details", n, word),
    ))
}
