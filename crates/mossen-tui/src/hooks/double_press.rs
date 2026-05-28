//! Double press detection state (useDoublePress.ts).
//!
//! Creates a handler that fires one callback on first press, and another on
//! second press within a timeout window.

use std::time::{Duration, Instant};

/// Timeout for double-press detection (800ms).
pub const DOUBLE_PRESS_TIMEOUT_MS: u64 = 800;

/// State machine for double-press detection.
#[derive(Debug, Clone)]
pub struct DoublePressState {
    last_press: Option<Instant>,
    timeout_active: bool,
    pending: bool,
    timeout_duration: Duration,
}

impl DoublePressState {
    pub fn new() -> Self {
        Self {
            last_press: None,
            timeout_active: false,
            pending: false,
            timeout_duration: Duration::from_millis(DOUBLE_PRESS_TIMEOUT_MS),
        }
    }

    /// Process a press event. Returns the action to take.
    pub fn press(&mut self) -> DoublePressAction {
        let now = Instant::now();
        let is_double_press = if let Some(last) = self.last_press {
            now.duration_since(last) <= self.timeout_duration && self.timeout_active
        } else {
            false
        };

        self.last_press = Some(now);

        if is_double_press {
            self.timeout_active = false;
            self.pending = false;
            DoublePressAction::DoublePress
        } else {
            self.timeout_active = true;
            self.pending = true;
            DoublePressAction::FirstPress
        }
    }

    /// Check if the timeout has expired (call periodically).
    pub fn tick(&mut self) -> bool {
        if !self.timeout_active {
            return false;
        }
        if let Some(last) = self.last_press {
            if last.elapsed() > self.timeout_duration {
                self.timeout_active = false;
                self.pending = false;
                return true; // timeout expired
            }
        }
        false
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    pub fn reset(&mut self) {
        self.last_press = None;
        self.timeout_active = false;
        self.pending = false;
    }
}

impl Default for DoublePressState {
    fn default() -> Self {
        Self::new()
    }
}

/// Action resulting from a press event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoublePressAction {
    /// First press detected — show pending indicator.
    FirstPress,
    /// Second press within timeout — execute double-press action.
    DoublePress,
}
