//! Session backgrounding hook (useSessionBackgrounding.ts).
//! Manages the session state when the terminal goes to background.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionForegroundState {
    Foreground,
    Background,
    Returning,
}

#[derive(Debug, Clone)]
pub struct SessionBackgroundingState {
    pub state: SessionForegroundState,
    pub background_since: Option<Instant>,
    pub total_background_time: std::time::Duration,
    pub background_count: u32,
}

impl SessionBackgroundingState {
    pub fn new() -> Self {
        Self {
            state: SessionForegroundState::Foreground,
            background_since: None,
            total_background_time: std::time::Duration::ZERO,
            background_count: 0,
        }
    }
    pub fn go_background(&mut self) {
        self.state = SessionForegroundState::Background;
        self.background_since = Some(Instant::now());
        self.background_count += 1;
    }
    pub fn go_foreground(&mut self) -> std::time::Duration {
        let elapsed = self
            .background_since
            .map_or(std::time::Duration::ZERO, |t| t.elapsed());
        self.total_background_time += elapsed;
        self.state = SessionForegroundState::Returning;
        self.background_since = None;
        elapsed
    }
    pub fn finish_return(&mut self) {
        self.state = SessionForegroundState::Foreground;
    }
    pub fn is_background(&self) -> bool {
        self.state == SessionForegroundState::Background
    }
    pub fn background_elapsed(&self) -> std::time::Duration {
        self.background_since
            .map_or(std::time::Duration::ZERO, |t| t.elapsed())
    }
}
impl Default for SessionBackgroundingState {
    fn default() -> Self {
        Self::new()
    }
}
