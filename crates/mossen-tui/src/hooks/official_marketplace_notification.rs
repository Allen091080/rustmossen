//! Official marketplace notification.
//! Shows a notification about the official plugin marketplace.

#[derive(Debug, Clone)]
pub struct OfficialMarketplaceNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub marketplace_url: String,
}

impl OfficialMarketplaceNotificationState {
    pub fn new(url: &str) -> Self {
        Self {
            shown: false,
            dismissed: false,
            marketplace_url: url.to_string(),
        }
    }
    pub fn should_show(&self, has_plugins: bool, seen_before: bool) -> bool {
        !self.shown && !self.dismissed && has_plugins && !seen_before
    }
    pub fn show(&mut self) {
        self.shown = true;
    }
    pub fn dismiss(&mut self) {
        self.dismissed = true;
    }
}
impl Default for OfficialMarketplaceNotificationState {
    fn default() -> Self {
        Self::new("https://marketplace.mossen.dev")
    }
}
