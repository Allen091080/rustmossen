//! Notify after timeout hook (useNotifyAfterTimeout.ts).
//! Shows a notification after a specified delay.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct NotifyAfterTimeoutState {
    pub delay: Duration,
    pub started_at: Option<Instant>,
    pub fired: bool,
    pub notification_key: String,
    pub notification_text: String,
}

impl NotifyAfterTimeoutState {
    pub fn new(delay_ms: u64, key: &str, text: &str) -> Self {
        Self {
            delay: Duration::from_millis(delay_ms),
            started_at: None, fired: false,
            notification_key: key.to_string(), notification_text: text.to_string(),
        }
    }
    pub fn start(&mut self) { self.started_at = Some(Instant::now()); self.fired = false; }
    pub fn should_fire(&self) -> bool {
        !self.fired && self.started_at.map_or(false, |t| t.elapsed() >= self.delay)
    }
    pub fn fire(&mut self) -> Option<(&str, &str)> {
        if self.should_fire() { self.fired = true; Some((&self.notification_key, &self.notification_text)) }
        else { None }
    }
    pub fn reset(&mut self) { self.started_at = None; self.fired = false; }
    pub fn cancel(&mut self) { self.started_at = None; }
}

/// Threshold (ms) below which an interaction is considered recent enough
/// that we shouldn't fire a "you've been idle" notification. Translated
/// from `DEFAULT_INTERACTION_THRESHOLD_MS` in TS.
pub const DEFAULT_INTERACTION_THRESHOLD_MS: u64 = 6000;
