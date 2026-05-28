//! CanSwitchSubscription notification (can_switch_subscription.ts).
//! Notifies when user can switch to an existing subscription.

#[derive(Debug, Clone)]
pub struct CanSwitchSubscriptionNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl CanSwitchSubscriptionNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "can-switch-subscription".to_string(),
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

impl Default for CanSwitchSubscriptionNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// `useCanSwitchToExistingSubscription` — hard-cut Mossen builds never
/// advertise legacy hosted subscriptions, so this hook is a no-op.
/// Returns `None` so callers can drop the notification entirely.
///
/// TS source: `useCanSwitchToExistingSubscription()`.
pub fn use_can_switch_to_existing_subscription() -> Option<(String, String)> {
    None
}
