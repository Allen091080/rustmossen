//! DeprecationWarning notification (deprecation_warning.ts).
//! Shows deprecation warnings for deprecated features.

#[derive(Debug, Clone)]
pub struct DeprecationWarningNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl DeprecationWarningNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "deprecation-warning".to_string(),
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

impl Default for DeprecationWarningNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// `useDeprecationWarningNotification` — pure-logic translation.
/// Returns a tuple `(maybe_event, new_last_warning)` where `new_last_warning`
/// is the value the caller should store back into the equivalent of the
/// TS `lastWarningRef`.
///
/// TS source: `useDeprecationWarningNotification(model)`.
pub fn use_deprecation_warning_notification(
    is_remote_mode: bool,
    deprecation_warning: Option<&str>,
    last_warning: Option<&str>,
) -> (Option<(String, String)>, Option<String>) {
    if is_remote_mode {
        return (None, last_warning.map(String::from));
    }
    match deprecation_warning {
        Some(w) if Some(w) != last_warning => (
            Some((
                "model-deprecation-warning".to_string(),
                w.to_string(),
            )),
            Some(w.to_string()),
        ),
        Some(_) => (None, last_warning.map(String::from)),
        None => (None, None),
    }
}
