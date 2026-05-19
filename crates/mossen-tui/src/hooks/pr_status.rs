//! PR status hook (usePrStatus.ts).
//! Polls PR review status periodically while the session is active.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrReviewState { Pending, Approved, ChangesRequested, Commented, Dismissed }

#[derive(Debug, Clone)]
pub struct PrStatusState {
    pub number: Option<u32>,
    pub url: Option<String>,
    pub review_state: Option<PrReviewState>,
    pub last_updated: Option<Instant>,
    pub is_polling: bool,
    pub disabled: bool,
    pub poll_interval: Duration,
    pub last_fetch: Option<Instant>,
}

const POLL_INTERVAL_MS: u64 = 60_000;
const SLOW_GH_THRESHOLD_MS: u64 = 4_000;
const IDLE_STOP_MS: u64 = 3_600_000;

impl PrStatusState {
    pub fn new() -> Self {
        Self {
            number: None, url: None, review_state: None, last_updated: None,
            is_polling: false, disabled: false,
            poll_interval: Duration::from_millis(POLL_INTERVAL_MS), last_fetch: None,
        }
    }
    pub fn should_poll(&self) -> bool {
        if self.disabled || self.is_polling { return false; }
        self.last_fetch.map_or(true, |t| t.elapsed() >= self.poll_interval)
    }
    pub fn start_poll(&mut self) { self.is_polling = true; }
    pub fn complete_poll(&mut self, number: Option<u32>, url: Option<String>, state: Option<PrReviewState>, duration_ms: u64) {
        self.is_polling = false;
        self.last_fetch = Some(Instant::now());
        if duration_ms > SLOW_GH_THRESHOLD_MS { self.disabled = true; return; }
        self.number = number; self.url = url; self.review_state = state;
        self.last_updated = Some(Instant::now());
    }
    pub fn poll_error(&mut self) { self.is_polling = false; }
    pub fn should_stop_idle(&self, last_interaction: Instant) -> bool {
        last_interaction.elapsed() >= Duration::from_millis(IDLE_STOP_MS)
    }
}
impl Default for PrStatusState { fn default() -> Self { Self::new() } }
