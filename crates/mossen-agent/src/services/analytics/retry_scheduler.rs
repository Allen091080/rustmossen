//! Retry scheduler — exponential backoff for failed event deliveries.

use std::time::Duration;

/// Calculate retry delay with exponential backoff and jitter.
pub fn get_retry_delay(attempt: u32, base_delay_ms: u64) -> Duration {
    let exponential = base_delay_ms * (1u64 << attempt.min(10));
    let jitter = (rand::random::<f64>() * exponential as f64 * 0.1) as u64;
    Duration::from_millis(exponential + jitter)
}

/// Maximum number of retry attempts before giving up.
pub const MAX_RETRY_ATTEMPTS: u32 = 5;

/// Check if an event should be retried based on attempt count.
pub fn should_retry(attempt: u32) -> bool {
    attempt < MAX_RETRY_ATTEMPTS
}

/// Retry scheduler that manages backoff for a set of pending retries.
pub struct RetryScheduler {
    base_delay_ms: u64,
    max_attempts: u32,
}

impl RetryScheduler {
    pub fn new(base_delay_ms: u64, max_attempts: u32) -> Self {
        Self {
            base_delay_ms,
            max_attempts,
        }
    }

    pub fn next_delay(&self, attempt: u32) -> Option<Duration> {
        if attempt >= self.max_attempts {
            return None;
        }
        Some(get_retry_delay(attempt, self.base_delay_ms))
    }
}

impl Default for RetryScheduler {
    fn default() -> Self {
        Self::new(1000, MAX_RETRY_ATTEMPTS)
    }
}
