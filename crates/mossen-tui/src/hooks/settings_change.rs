//! Settings Change hook (useSettingsChange.ts).
//! Detects settings file changes and triggers callbacks.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSource {
    User,
    Project,
    Default,
}

#[derive(Debug, Clone)]
pub struct SettingsChangeState {
    pub last_change: Option<Instant>,
    pub last_source: Option<SettingSource>,
    pub change_count: u64,
    pub watching: bool,
}

impl SettingsChangeState {
    pub fn new() -> Self {
        Self {
            last_change: None,
            last_source: None,
            change_count: 0,
            watching: false,
        }
    }
    pub fn start_watching(&mut self) {
        self.watching = true;
    }
    pub fn stop_watching(&mut self) {
        self.watching = false;
    }
    pub fn on_change(&mut self, source: SettingSource) {
        self.last_change = Some(Instant::now());
        self.last_source = Some(source);
        self.change_count += 1;
    }
}
impl Default for SettingsChangeState {
    fn default() -> Self {
        Self::new()
    }
}
