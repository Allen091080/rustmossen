//! Inbox poller hook (useInboxPoller.ts).
//!
//! Periodically polls for new messages/notifications from the server.

use std::time::{Duration, Instant};

/// State for inbox polling.
#[derive(Debug, Clone)]
pub struct InboxPollerState {
    pub is_polling: bool,
    pub last_poll: Option<Instant>,
    pub poll_interval: Duration,
    pub unread_count: u32,
    pub error_count: u32,
    pub max_errors: u32,
    pub disabled: bool,
}

impl InboxPollerState {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            is_polling: false,
            last_poll: None,
            poll_interval: Duration::from_millis(interval_ms),
            unread_count: 0,
            error_count: 0,
            max_errors: 5,
            disabled: false,
        }
    }

    /// Check if it's time to poll.
    pub fn should_poll(&self) -> bool {
        if self.disabled || self.is_polling {
            return false;
        }
        match self.last_poll {
            Some(last) => last.elapsed() >= self.poll_interval,
            None => true,
        }
    }

    /// Start a poll.
    pub fn start_poll(&mut self) {
        self.is_polling = true;
    }

    /// Complete a successful poll.
    pub fn complete_poll(&mut self, unread: u32) {
        self.is_polling = false;
        self.last_poll = Some(Instant::now());
        self.unread_count = unread;
        self.error_count = 0;
    }

    /// Record a poll error.
    pub fn poll_error(&mut self) {
        self.is_polling = false;
        self.error_count += 1;
        if self.error_count >= self.max_errors {
            self.disabled = true;
        }
    }

    /// Reset error state and re-enable polling.
    pub fn reset_errors(&mut self) {
        self.error_count = 0;
        self.disabled = false;
    }
}

impl Default for InboxPollerState {
    fn default() -> Self {
        Self::new(30_000)
    }
}
