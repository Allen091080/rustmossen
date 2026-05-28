//! Elapsed time hook (useElapsedTime.ts).
//!
//! Returns formatted elapsed time since a start time, with
//! interval-based updates.

use std::time::{Duration, Instant};

/// State for elapsed time display.
#[derive(Debug, Clone)]
pub struct ElapsedTimeState {
    pub start_time: Instant,
    pub is_running: bool,
    pub update_interval: Duration,
    pub paused_duration: Duration,
    pub end_time: Option<Instant>,
    cached_display: String,
    last_update: Instant,
}

impl ElapsedTimeState {
    pub fn new(start_time: Instant) -> Self {
        Self {
            start_time,
            is_running: true,
            update_interval: Duration::from_secs(1),
            paused_duration: Duration::ZERO,
            end_time: None,
            cached_display: String::new(),
            last_update: Instant::now(),
        }
    }

    /// Set the update interval.
    pub fn with_interval(mut self, ms: u64) -> Self {
        self.update_interval = Duration::from_millis(ms);
        self
    }

    /// Set paused duration to subtract.
    pub fn with_paused(mut self, paused: Duration) -> Self {
        self.paused_duration = paused;
        self
    }

    /// Freeze the timer at a specific end time.
    pub fn with_end_time(mut self, end: Instant) -> Self {
        self.end_time = Some(end);
        self
    }

    /// Get the current elapsed duration.
    pub fn elapsed(&self) -> Duration {
        let end = self.end_time.unwrap_or_else(Instant::now);
        end.saturating_duration_since(self.start_time)
            .saturating_sub(self.paused_duration)
    }

    /// Get formatted duration string (e.g., "1m 23s").
    pub fn formatted(&mut self) -> &str {
        let now = Instant::now();
        if now.duration_since(self.last_update) >= self.update_interval
            || self.cached_display.is_empty()
        {
            self.cached_display = format_duration(self.elapsed());
            self.last_update = now;
        }
        &self.cached_display
    }

    /// Check if the timer should tick (update display).
    pub fn should_tick(&self) -> bool {
        self.is_running && self.last_update.elapsed() >= self.update_interval
    }

    /// Stop the timer.
    pub fn stop(&mut self) {
        self.is_running = false;
        self.end_time = Some(Instant::now());
    }

    /// Resume the timer.
    pub fn resume(&mut self) {
        self.is_running = true;
        self.end_time = None;
    }
}

/// Format a duration as a human-readable string.
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{}m", m)
        } else {
            format!("{}m {}s", m, s)
        }
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{}h", h)
        } else {
            format!("{}h {}m", h, m)
        }
    }
}
