//! Clipboard image hint hook (useClipboardImageHint.ts).
//!
//! Shows a notification when the terminal regains focus and the
//! clipboard contains an image.

use std::time::{Duration, Instant};

const FOCUS_CHECK_DEBOUNCE_MS: u64 = 1000;
const HINT_COOLDOWN_MS: u64 = 30000;

/// State for clipboard image hint notification.
#[derive(Debug, Clone)]
pub struct ClipboardImageHintState {
    pub enabled: bool,
    pub last_focused: bool,
    pub last_hint_time: Option<Instant>,
    pub pending_check: bool,
    pub check_scheduled_at: Option<Instant>,
}

impl ClipboardImageHintState {
    pub fn new() -> Self {
        Self {
            enabled: false,
            last_focused: false,
            last_hint_time: None,
            pending_check: false,
            check_scheduled_at: None,
        }
    }

    /// Update focus state. Returns true if a clipboard check should be triggered.
    pub fn on_focus_change(&mut self, is_focused: bool) -> bool {
        let was_focused = self.last_focused;
        self.last_focused = is_focused;

        if !self.enabled || !is_focused || was_focused {
            return false;
        }

        // Check cooldown
        if let Some(last_hint) = self.last_hint_time {
            if last_hint.elapsed() < Duration::from_millis(HINT_COOLDOWN_MS) {
                return false;
            }
        }

        // Schedule a debounced check
        self.pending_check = true;
        self.check_scheduled_at = Some(Instant::now());
        true
    }

    /// Check if the debounce period has passed.
    pub fn should_check_clipboard(&self) -> bool {
        if !self.pending_check {
            return false;
        }
        if let Some(scheduled) = self.check_scheduled_at {
            scheduled.elapsed() >= Duration::from_millis(FOCUS_CHECK_DEBOUNCE_MS)
        } else {
            false
        }
    }

    /// Mark that the hint was shown.
    pub fn mark_hint_shown(&mut self) {
        self.last_hint_time = Some(Instant::now());
        self.pending_check = false;
        self.check_scheduled_at = None;
    }

    /// Cancel pending check.
    pub fn cancel_check(&mut self) {
        self.pending_check = false;
        self.check_scheduled_at = None;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Default for ClipboardImageHintState {
    fn default() -> Self {
        Self::new()
    }
}
