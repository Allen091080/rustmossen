//! Voice Enabled hook (useVoiceEnabled.ts).
//! Checks if voice input is enabled and available.

#[derive(Debug, Clone)]
pub struct VoiceEnabledState {
    pub enabled: bool,
    pub available: bool,
    pub reason_disabled: Option<String>,
}

impl VoiceEnabledState {
    pub fn new() -> Self { Self { enabled: false, available: false, reason_disabled: None } }
    pub fn check_availability(&mut self, has_microphone: bool, feature_flag: bool, settings_enabled: bool) {
        self.available = has_microphone && feature_flag;
        self.enabled = self.available && settings_enabled;
        if !has_microphone { self.reason_disabled = Some("No microphone detected".to_string()); }
        else if !feature_flag { self.reason_disabled = Some("Voice feature not available".to_string()); }
        else if !settings_enabled { self.reason_disabled = Some("Voice disabled in settings".to_string()); }
        else { self.reason_disabled = None; }
    }
    pub fn is_usable(&self) -> bool { self.enabled && self.available }
}
impl Default for VoiceEnabledState { fn default() -> Self { Self::new() } }
