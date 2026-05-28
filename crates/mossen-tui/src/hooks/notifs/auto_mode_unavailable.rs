//! AutoModeUnavailable notification (auto_mode_unavailable.ts).
//! Shows notification when auto mode is unavailable (settings, circuit-breaker, org-allowlist).

#[derive(Debug, Clone)]
pub struct AutoModeUnavailableNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl AutoModeUnavailableNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "auto-mode-unavailable".to_string(),
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

impl Default for AutoModeUnavailableNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    Plan,
    AcceptEdits,
    Auto,
    BypassPermissions,
}

#[derive(Debug, Clone)]
pub struct AutoModeUnavailableInputs {
    pub feature_enabled: bool,
    pub is_remote_mode: bool,
    pub current_mode: PermissionMode,
    pub prev_mode: PermissionMode,
    pub is_auto_mode_available: bool,
    pub has_auto_mode_opt_in: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoModeUnavailableEvent {
    pub key: String,
    pub text: String,
    pub color: String,
    pub priority: String,
}

/// `useAutoModeUnavailableNotification` — pure-logic translation. Returns
/// the notification event when the shift-tab carousel wraps past where
/// auto mode would have been.
///
/// TS source: `useAutoModeUnavailableNotification()`.
pub fn use_auto_mode_unavailable_notification(
    inputs: &AutoModeUnavailableInputs,
    already_shown: bool,
) -> Option<AutoModeUnavailableEvent> {
    if !inputs.feature_enabled || inputs.is_remote_mode || already_shown {
        return None;
    }
    let wrapped_past_auto_slot = inputs.current_mode == PermissionMode::Default
        && inputs.prev_mode != PermissionMode::Default
        && inputs.prev_mode != PermissionMode::Auto
        && !inputs.is_auto_mode_available
        && inputs.has_auto_mode_opt_in;
    if !wrapped_past_auto_slot {
        return None;
    }
    let reason = inputs.unavailable_reason.as_deref()?;
    Some(AutoModeUnavailableEvent {
        key: "auto-mode-unavailable".to_string(),
        text: format!("Auto mode unavailable · {}", reason),
        color: "warning".to_string(),
        priority: "medium".to_string(),
    })
}
