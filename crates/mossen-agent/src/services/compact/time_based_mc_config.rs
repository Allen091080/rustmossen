//! GrowthBook config for time-based microcompact.
//!
//! Triggers content-clearing microcompact when the gap since the last main-loop
//! assistant message exceeds a threshold — the server-side prompt cache has
//! almost certainly expired, so the full prefix will be rewritten anyway.

use serde::{Deserialize, Serialize};

/// Configuration for time-based microcompact trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBasedMCConfig {
    /// Master switch. When false, time-based microcompact is a no-op.
    pub enabled: bool,
    /// Trigger when (now - last assistant timestamp) exceeds this many minutes.
    /// 60 is the safe choice: the server's 1h cache TTL is guaranteed expired.
    pub gap_threshold_minutes: u64,
    /// Keep this many most-recent compactable tool results.
    pub keep_recent: usize,
}

impl Default for TimeBasedMCConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            gap_threshold_minutes: 60,
            keep_recent: 5,
        }
    }
}

/// Get the time-based MC config. In the TS version this reads from GrowthBook.
/// Here we provide the default with ability to override via feature flags.
pub fn get_time_based_mc_config() -> TimeBasedMCConfig {
    // In production, this would read from a feature flag service.
    // For now, return defaults.
    TimeBasedMCConfig::default()
}
