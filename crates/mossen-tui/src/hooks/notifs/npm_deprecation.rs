//! NpmDeprecation notification (npm_deprecation.ts).
//! Warns about deprecated npm installation method.

#[derive(Debug, Clone)]
pub struct NpmDeprecationNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl NpmDeprecationNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "npm-deprecation".to_string(),
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

impl Default for NpmDeprecationNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct NpmDeprecationInputs {
    pub product_display_name: String,
    pub product_cli_name: String,
    pub installer_docs_url: Option<String>,
    pub custom_backend_enabled: bool,
    pub is_bundled_mode: bool,
    pub disable_installation_checks: bool,
    pub installation_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmDeprecationEvent {
    pub key: String,
    pub text: String,
    pub color: String,
    pub priority: String,
    pub timeout_ms: u64,
}

/// Build the npm deprecation message text. Branches on whether a docs URL
/// is provided.
///
/// TS source: `getNpmDeprecationMessage()`.
pub fn get_npm_deprecation_message(
    product_display_name: &str,
    product_cli_name: &str,
    installer_docs_url: Option<&str>,
) -> String {
    match installer_docs_url {
        None => format!(
            "{} now uses the installer flow instead of npm. Run `{} install` or use your build's installer documentation for more options.",
            product_display_name, product_cli_name
        ),
        Some(url) => format!(
            "{} now uses the installer flow instead of npm. Run `{} install` or see {} for more options.",
            product_display_name, product_cli_name, url
        ),
    }
}

/// Async-equivalent of `getNpmDeprecationNotification()`. Returns the
/// event to fire on startup, or `None` if it shouldn't fire.
///
/// TS source: `getNpmDeprecationNotification()`.
pub fn get_npm_deprecation_notification(inputs: &NpmDeprecationInputs) -> Option<NpmDeprecationEvent> {
    if inputs.custom_backend_enabled || inputs.is_bundled_mode || inputs.disable_installation_checks {
        return None;
    }
    if inputs.installation_type == "development" {
        return None;
    }
    Some(NpmDeprecationEvent {
        key: "npm-deprecation-warning".to_string(),
        text: get_npm_deprecation_message(
            &inputs.product_display_name,
            &inputs.product_cli_name,
            inputs.installer_docs_url.as_deref(),
        ),
        color: "warning".to_string(),
        priority: "high".to_string(),
        timeout_ms: 15_000,
    })
}

/// `useNpmDeprecationNotification` — thin wrapper around
/// `get_npm_deprecation_notification` matching the TS export name.
///
/// TS source: `useNpmDeprecationNotification()`.
pub fn use_npm_deprecation_notification(inputs: &NpmDeprecationInputs) -> Option<NpmDeprecationEvent> {
    get_npm_deprecation_notification(inputs)
}
