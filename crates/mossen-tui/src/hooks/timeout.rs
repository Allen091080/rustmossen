//! Timeout hook (useTimeout.ts).
//! Simple timeout hook that returns true after a delay.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TimeoutState {
    pub delay: Duration,
    pub started_at: Option<Instant>,
    pub is_elapsed: bool,
}

impl TimeoutState {
    pub fn new(delay_ms: u64) -> Self {
        Self {
            delay: Duration::from_millis(delay_ms),
            started_at: None,
            is_elapsed: false,
        }
    }
    pub fn start(&mut self) {
        self.started_at = Some(Instant::now());
        self.is_elapsed = false;
    }
    pub fn reset(&mut self) {
        self.started_at = Some(Instant::now());
        self.is_elapsed = false;
    }
    pub fn tick(&mut self) -> bool {
        if self.is_elapsed {
            return true;
        }
        if let Some(start) = self.started_at {
            if start.elapsed() >= self.delay {
                self.is_elapsed = true;
                return true;
            }
        }
        false
    }
    pub fn is_elapsed(&self) -> bool {
        self.is_elapsed
    }
}
impl Default for TimeoutState {
    fn default() -> Self {
        Self::new(1000)
    }
}
