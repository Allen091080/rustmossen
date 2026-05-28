//! Startup notification (startup.ts).
//! One-shot startup notifications (welcome, changelog, etc.).

#[derive(Debug, Clone)]
pub struct StartupNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl StartupNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "startup".to_string(),
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

impl Default for StartupNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupNotificationEvent {
    pub key: String,
    pub text: String,
    pub color: String,
    pub priority: String,
    pub timeout_ms: u64,
}

/// `useStartupNotification` — pure-logic translation. Fires the
/// notification(s) returned by the `compute` callback once per session.
/// Returns the events to surface on this invocation, or `None` if the
/// caller has already fired the startup notification.
///
/// TS source: `useStartupNotification(compute)`. The TS body gates on
/// `hasRunRef` and `getIsRemoteMode()`; the Rust port asks the caller
/// to thread the `has_run` flag in.
pub fn use_startup_notification<F>(
    is_remote_mode: bool,
    has_run: &mut bool,
    compute: F,
) -> Vec<StartupNotificationEvent>
where
    F: FnOnce() -> Vec<StartupNotificationEvent>,
{
    if is_remote_mode || *has_run {
        return Vec::new();
    }
    *has_run = true;
    compute()
}
