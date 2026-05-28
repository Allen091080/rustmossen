//! Blink animation hook (useBlink.ts).
//!
//! Synchronized blinking animation that pauses when offscreen or
//! when the terminal is blurred.

use std::time::{Duration, Instant};

/// Default blink interval in milliseconds.
pub const BLINK_INTERVAL_MS: u64 = 600;

/// State for blink animation.
#[derive(Debug, Clone)]
pub struct BlinkState {
    pub enabled: bool,
    pub interval: Duration,
    pub start_time: Instant,
    pub terminal_focused: bool,
    pub element_visible: bool,
}

impl BlinkState {
    pub fn new() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_millis(BLINK_INTERVAL_MS),
            start_time: Instant::now(),
            terminal_focused: true,
            element_visible: true,
        }
    }

    pub fn with_interval(mut self, ms: u64) -> Self {
        self.interval = Duration::from_millis(ms);
        self
    }

    /// Compute whether the blink is currently visible.
    pub fn is_visible(&self) -> bool {
        if !self.enabled || !self.terminal_focused {
            return true;
        }
        if !self.element_visible {
            return true;
        }
        let elapsed = self.start_time.elapsed();
        let cycle = elapsed.as_millis() / self.interval.as_millis().max(1);
        cycle % 2 == 0
    }

    /// Set whether the animation is enabled.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Update terminal focus state.
    pub fn set_terminal_focused(&mut self, focused: bool) {
        self.terminal_focused = focused;
    }

    /// Update element visibility (in viewport).
    pub fn set_element_visible(&mut self, visible: bool) {
        self.element_visible = visible;
    }

    /// Reset the blink clock.
    pub fn reset(&mut self) {
        self.start_time = Instant::now();
    }
}

impl Default for BlinkState {
    fn default() -> Self {
        Self::new()
    }
}
