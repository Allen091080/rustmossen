//! IdeStatusIndicator notification (ide_status_indicator.ts).
//! Shows IDE connection status in the notification area.

#[derive(Debug, Clone)]
pub struct IdeStatusIndicatorNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl IdeStatusIndicatorNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "ide-status-indicator".to_string(),
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

impl Default for IdeStatusIndicatorNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeStatus {
    Connected,
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct IdeStatusIndicatorInputs {
    pub is_remote_mode: bool,
    pub is_supported_terminal: bool,
    pub ide_status: Option<IdeStatus>,
    pub ide_name: Option<String>,
    pub is_jet_brains: bool,
    pub install_error_present: bool,
    pub selection_present: bool,
    pub ide_hint_shown_count: u32,
    pub max_ide_hint_show_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdeStatusIndicatorEvent {
    pub key: String,
    pub text: String,
    pub color: Option<String>,
    pub priority: String,
}

/// `useIDEStatusIndicator` — pure-logic translation. Returns the events
/// the caller should fire and the keys to clear.
///
/// TS source: `useIDEStatusIndicator({ ideInstallationStatus, ideSelection,
/// mcpClients })`. The TS body conditionally `addNotification`s /
/// `removeNotification`s based on the combination of IDE status, install
/// error, and JetBrains hint. We compute the four notifications it can
/// emit and return them.
pub fn use_ide_status_indicator(inputs: &IdeStatusIndicatorInputs) -> (Vec<IdeStatusIndicatorEvent>, Vec<String>) {
    let mut events = Vec::new();
    let mut clear_keys = Vec::new();
    if inputs.is_remote_mode {
        return (events, clear_keys);
    }

    let should_show_selection = inputs.ide_status == Some(IdeStatus::Connected) && inputs.selection_present;
    let should_show_connected = inputs.ide_status == Some(IdeStatus::Connected) && !should_show_selection;
    let show_install_error = (inputs.install_error_present || inputs.is_jet_brains)
        && !inputs.is_jet_brains
        && !should_show_connected
        && !should_show_selection;
    let show_jet_brains_info = (inputs.install_error_present || inputs.is_jet_brains)
        && inputs.is_jet_brains
        && !should_show_connected
        && !should_show_selection;

    // ide-status-hint (default integration suggestion).
    if inputs.is_supported_terminal || inputs.ide_status.is_some() || show_jet_brains_info {
        clear_keys.push("ide-status-hint".to_string());
    } else if inputs.ide_hint_shown_count >= inputs.max_ide_hint_show_count {
        // throttle reached
    }

    // ide-status-disconnected
    if show_install_error || show_jet_brains_info
        || inputs.ide_status != Some(IdeStatus::Disconnected)
        || inputs.ide_name.is_none()
    {
        clear_keys.push("ide-status-disconnected".to_string());
    } else if let Some(name) = &inputs.ide_name {
        events.push(IdeStatusIndicatorEvent {
            key: "ide-status-disconnected".to_string(),
            text: format!("{} disconnected", name),
            color: Some("error".to_string()),
            priority: "medium".to_string(),
        });
    }

    // ide-status-jetbrains-disconnected
    if !show_jet_brains_info {
        clear_keys.push("ide-status-jetbrains-disconnected".to_string());
    } else {
        events.push(IdeStatusIndicatorEvent {
            key: "ide-status-jetbrains-disconnected".to_string(),
            text: "IDE plugin not connected · /status for info".to_string(),
            color: None,
            priority: "medium".to_string(),
        });
    }

    // ide-status-install-error
    if !show_install_error {
        clear_keys.push("ide-status-install-error".to_string());
    } else {
        events.push(IdeStatusIndicatorEvent {
            key: "ide-status-install-error".to_string(),
            text: "IDE extension install failed (see /status for info)".to_string(),
            color: Some("error".to_string()),
            priority: "medium".to_string(),
        });
    }

    (events, clear_keys)
}
