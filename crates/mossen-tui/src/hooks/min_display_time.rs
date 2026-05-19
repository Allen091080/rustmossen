//! Min display time hook (useMinDisplayTime.ts).
//! Ensures a UI element is shown for at least a minimum duration.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct MinDisplayTimeState {
    pub min_duration: Duration,
    pub shown_at: Option<Instant>,
    pub content_ready: bool,
    pub force_visible: bool,
}

impl MinDisplayTimeState {
    pub fn new(min_ms: u64) -> Self {
        Self { min_duration: Duration::from_millis(min_ms), shown_at: None, content_ready: false, force_visible: false }
    }
    pub fn show(&mut self) { self.shown_at = Some(Instant::now()); self.force_visible = true; }
    pub fn mark_content_ready(&mut self) { self.content_ready = true; }
    pub fn should_remain_visible(&self) -> bool {
        if !self.force_visible { return false; }
        match self.shown_at {
            Some(t) => t.elapsed() < self.min_duration || !self.content_ready,
            None => false,
        }
    }
    pub fn can_hide(&self) -> bool {
        self.content_ready && self.shown_at.map_or(true, |t| t.elapsed() >= self.min_duration)
    }
    pub fn reset(&mut self) { self.shown_at = None; self.content_ready = false; self.force_visible = false; }
}
impl Default for MinDisplayTimeState { fn default() -> Self { Self::new(500) } }
