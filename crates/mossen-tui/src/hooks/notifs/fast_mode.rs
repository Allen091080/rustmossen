//! FastMode notification (fast_mode.ts).
//! Notification for fast mode activation/status.

#[derive(Debug, Clone)]
pub struct FastModeNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl FastModeNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "fast-mode".to_string(),
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

impl Default for FastModeNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Reason a fast-mode cooldown was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CooldownReason {
    Overloaded,
    RateLimit,
}

/// One fast-mode notification event the caller should surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastModeNotificationEvent {
    /// Stable notification key.
    pub key: String,
    /// User-visible text.
    pub text: String,
    /// Color name from the design system ("fastMode" / "warning").
    pub color: String,
    /// Priority bucket.
    pub priority: String,
    /// Other keys that this notification supersedes (TS uses `invalidates`).
    pub invalidates: Vec<String>,
}

/// Build the fast-mode cooldown message for a given reason and a
/// human-formatted reset duration. Translated from `getCooldownMessage`
/// in TS.
pub fn get_cooldown_message(reason: CooldownReason, reset_in: &str) -> String {
    match reason {
        CooldownReason::Overloaded => format!(
            "Fast mode overloaded and is temporarily unavailable · resets in {}",
            reset_in
        ),
        CooldownReason::RateLimit => format!(
            "Fast limit reached and temporarily disabled · resets in {}",
            reset_in
        ),
    }
}

/// `useFastModeNotification` — pure-logic translation. Returns the
/// notifications the caller should display given the inputs.
///
/// TS source: `useFastModeNotification()`. The TS version uses
/// `useEffect` to subscribe to org-change / overage / cooldown events;
/// the Rust port is called by the event source when one of those
/// happens.
#[derive(Debug, Clone)]
pub struct FastModeNotificationInputs {
    pub is_remote_mode: bool,
    pub fast_mode_enabled: bool,
    pub is_fast_mode_on: bool,
}

/// Outcome of an org-change event: whether to surface a notification and
/// whether the app should turn fast mode off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgChangeOutcome {
    pub notification: Option<FastModeNotificationEvent>,
    pub disable_fast_mode: bool,
}

pub fn use_fast_mode_notification_on_org_change(
    inputs: &FastModeNotificationInputs,
    org_enabled: bool,
) -> OrgChangeOutcome {
    if inputs.is_remote_mode || !inputs.fast_mode_enabled {
        return OrgChangeOutcome { notification: None, disable_fast_mode: false };
    }
    if org_enabled {
        OrgChangeOutcome {
            notification: Some(FastModeNotificationEvent {
                key: "fast-mode-org-changed".to_string(),
                text: "Fast mode is now available · /fast to turn on".to_string(),
                color: "fastMode".to_string(),
                priority: "immediate".to_string(),
                invalidates: Vec::new(),
            }),
            disable_fast_mode: false,
        }
    } else if inputs.is_fast_mode_on {
        OrgChangeOutcome {
            notification: Some(FastModeNotificationEvent {
                key: "fast-mode-org-changed".to_string(),
                text: "Fast mode has been disabled by your organization".to_string(),
                color: "warning".to_string(),
                priority: "immediate".to_string(),
                invalidates: Vec::new(),
            }),
            disable_fast_mode: true,
        }
    } else {
        OrgChangeOutcome { notification: None, disable_fast_mode: false }
    }
}

pub fn use_fast_mode_notification_on_cooldown_triggered(
    inputs: &FastModeNotificationInputs,
    reason: CooldownReason,
    reset_in: &str,
) -> Option<FastModeNotificationEvent> {
    if inputs.is_remote_mode || !inputs.is_fast_mode_on {
        return None;
    }
    Some(FastModeNotificationEvent {
        key: "fast-mode-cooldown-started".to_string(),
        text: get_cooldown_message(reason, reset_in),
        color: "warning".to_string(),
        priority: "immediate".to_string(),
        invalidates: vec!["fast-mode-cooldown-expired".to_string()],
    })
}

pub fn use_fast_mode_notification_on_cooldown_expired(
    inputs: &FastModeNotificationInputs,
) -> Option<FastModeNotificationEvent> {
    if inputs.is_remote_mode || !inputs.is_fast_mode_on {
        return None;
    }
    Some(FastModeNotificationEvent {
        key: "fast-mode-cooldown-expired".to_string(),
        text: "Fast limit reset · now using fast mode".to_string(),
        color: "fastMode".to_string(),
        priority: "immediate".to_string(),
        invalidates: vec!["fast-mode-cooldown-started".to_string()],
    })
}

/// Entry point matching the TS symbol name — translates the React
/// hook's overall purpose into a single dispatching function. It accepts
/// the current inputs plus an event variant and returns the resulting
/// notification (if any).
pub enum FastModeEvent {
    OrgChange { org_enabled: bool },
    CooldownTriggered { reason: CooldownReason, reset_in: String },
    CooldownExpired,
}

pub fn use_fast_mode_notification(inputs: &FastModeNotificationInputs, event: &FastModeEvent) -> Option<FastModeNotificationEvent> {
    match event {
        FastModeEvent::OrgChange { org_enabled } => use_fast_mode_notification_on_org_change(inputs, *org_enabled).notification,
        FastModeEvent::CooldownTriggered { reason, reset_in } => use_fast_mode_notification_on_cooldown_triggered(inputs, *reason, reset_in),
        FastModeEvent::CooldownExpired => use_fast_mode_notification_on_cooldown_expired(inputs),
    }
}
