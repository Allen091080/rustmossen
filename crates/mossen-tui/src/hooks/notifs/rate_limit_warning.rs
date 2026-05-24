//! RateLimitWarning notification (rate_limit_warning.ts).
//! Warns when approaching or hitting rate limits.

#[derive(Debug, Clone)]
pub struct RateLimitWarningNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl RateLimitWarningNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "rate-limit-warning".to_string(),
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

impl Default for RateLimitWarningNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionType {
    Free,
    Pro,
    Team,
    Enterprise,
}

#[derive(Debug, Clone)]
pub struct RateLimitWarningInputs {
    pub is_remote_mode: bool,
    pub is_using_overage: bool,
    pub rate_limit_warning: Option<String>,
    pub using_overage_text: String,
    pub subscription_type: SubscriptionType,
    pub has_billing_access: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitNotification {
    pub key: String,
    pub text: String,
    pub priority: String,
}

/// `useRateLimitWarningNotification` — pure-logic translation. Returns
/// notifications to emit and the new value for the
/// `hasShownOverageNotification` flag.
///
/// TS source: `useRateLimitWarningNotification(model)`.
pub fn use_rate_limit_warning_notification(
    inputs: &RateLimitWarningInputs,
    has_shown_overage: bool,
    last_shown_warning: Option<&str>,
) -> (Vec<RateLimitNotification>, bool, Option<String>) {
    let mut notifications = Vec::new();
    if inputs.is_remote_mode {
        return (
            notifications,
            has_shown_overage,
            last_shown_warning.map(String::from),
        );
    }
    let is_team_or_ent = matches!(
        inputs.subscription_type,
        SubscriptionType::Team | SubscriptionType::Enterprise
    );

    let mut new_has_shown = has_shown_overage;
    if inputs.is_using_overage
        && !has_shown_overage
        && (!is_team_or_ent || inputs.has_billing_access)
    {
        notifications.push(RateLimitNotification {
            key: "limit-reached".to_string(),
            text: inputs.using_overage_text.clone(),
            priority: "immediate".to_string(),
        });
        new_has_shown = true;
    } else if !inputs.is_using_overage && has_shown_overage {
        new_has_shown = false;
    }

    let mut new_last_shown = last_shown_warning.map(String::from);
    if let Some(warning) = &inputs.rate_limit_warning {
        if last_shown_warning != Some(warning.as_str()) {
            new_last_shown = Some(warning.clone());
            notifications.push(RateLimitNotification {
                key: "rate-limit-warning".to_string(),
                text: warning.clone(),
                priority: "high".to_string(),
            });
        }
    }
    (notifications, new_has_shown, new_last_shown)
}
