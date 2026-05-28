//! Chrome extension notification.
//!
//! Shows notifications about Chrome extension status: not installed,
//! integration unavailable, or default-enabled.

/// Chrome extension notification level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeNotificationLevel {
    Info,
    Warning,
    Error,
}

/// Chrome extension notification surface.
#[derive(Debug, Clone)]
pub struct ChromeExtensionNotice {
    pub key: String,
    pub level: ChromeNotificationLevel,
    pub message: String,
}

/// State for chrome extension notification.
#[derive(Debug, Clone)]
pub struct ChromeExtensionNotificationState {
    pub notice: Option<ChromeExtensionNotice>,
    pub checked: bool,
    pub extension_installed: bool,
}

impl ChromeExtensionNotificationState {
    pub fn new() -> Self {
        Self {
            notice: None,
            checked: false,
            extension_installed: false,
        }
    }

    /// Determine the notification to show based on chrome integration state.
    pub fn check(
        &mut self,
        chrome_flag: Option<bool>,
        can_use_chrome: bool,
        is_custom_backend: bool,
        has_configured_urls: bool,
        extension_installed: bool,
        is_running_on_homespace: bool,
    ) {
        self.checked = true;
        self.extension_installed = extension_installed;

        // Check if chrome integration should be enabled
        let should_enable = chrome_flag.unwrap_or(true);
        if !should_enable {
            self.notice = None;
            return;
        }

        if !can_use_chrome {
            let message = if is_custom_backend && !has_configured_urls {
                "Chrome integration is not configured. Set MOSSEN_CODE_PLATFORM_BASE_URL or the MOSSEN_CODE_CHROME_* URLs first.".to_string()
            } else {
                "Chrome integration is not enabled for the current provider or backend configuration.".to_string()
            };
            self.notice = Some(ChromeExtensionNotice {
                key: "chrome-integration-unavailable".to_string(),
                level: ChromeNotificationLevel::Error,
                message,
            });
            return;
        }

        if !extension_installed && !is_running_on_homespace {
            self.notice = Some(ChromeExtensionNotice {
                key: "chrome-extension-not-detected".to_string(),
                level: ChromeNotificationLevel::Warning,
                message: "Chrome extension not detected".to_string(),
            });
            return;
        }

        if chrome_flag.is_none() {
            self.notice = Some(ChromeExtensionNotice {
                key: "mossen-in-chrome-default-enabled".to_string(),
                level: ChromeNotificationLevel::Info,
                message: "Chrome integration enabled · /chrome".to_string(),
            });
            return;
        }

        self.notice = None;
    }
}

impl Default for ChromeExtensionNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Inputs needed to determine the chrome-extension notification surface.
#[derive(Debug, Clone)]
pub struct ChromeExtensionSurfaceInputs<'a> {
    pub chrome_flag: Option<bool>,
    pub should_enable_in_chrome: bool,
    pub can_use_chrome_integration: bool,
    pub custom_backend_enabled: bool,
    pub has_configured_chrome_urls: bool,
    pub extension_installed: bool,
    pub running_on_homespace: bool,
    pub chrome_extension_url: &'a str,
}

/// Determine the chrome-extension notification surface for the current
/// session. Returns `None` if there's nothing to show.
///
/// TS source: `getChromeExtensionNotificationSurface()`.
pub fn get_chrome_extension_notification_surface(
    inputs: &ChromeExtensionSurfaceInputs<'_>,
) -> Option<ChromeExtensionNotice> {
    if !inputs.should_enable_in_chrome {
        return None;
    }
    if !inputs.can_use_chrome_integration {
        let message = if inputs.custom_backend_enabled && !inputs.has_configured_chrome_urls {
            "Chrome integration is not configured. Set MOSSEN_CODE_PLATFORM_BASE_URL or the MOSSEN_CODE_CHROME_* URLs first.".to_string()
        } else {
            "Chrome integration is not enabled for the current provider or backend configuration."
                .to_string()
        };
        return Some(ChromeExtensionNotice {
            key: "chrome-integration-unavailable".to_string(),
            level: ChromeNotificationLevel::Error,
            message,
        });
    }
    if !inputs.extension_installed && !inputs.running_on_homespace {
        return Some(ChromeExtensionNotice {
            key: "chrome-extension-not-detected".to_string(),
            level: ChromeNotificationLevel::Warning,
            message: format!(
                "Chrome extension not detected · {} to install",
                inputs.chrome_extension_url
            ),
        });
    }
    if inputs.chrome_flag.is_none() {
        return Some(ChromeExtensionNotice {
            key: "mossen-in-chrome-default-enabled".to_string(),
            level: ChromeNotificationLevel::Info,
            message: "Chrome integration enabled · /chrome".to_string(),
        });
    }
    None
}
