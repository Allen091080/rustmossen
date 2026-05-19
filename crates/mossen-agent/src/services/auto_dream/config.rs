//! Auto dream configuration.

use std::time::Duration;

/// Configuration for the auto dream (consolidation) service.
#[derive(Debug, Clone)]
pub struct AutoDreamConfig {
    pub enabled: bool,
    pub interval: Duration,
    pub max_turns: usize,
    pub timeout: Duration,
}

impl Default for AutoDreamConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(3600), // 1 hour
            max_turns: 6,
            timeout: Duration::from_secs(120),
        }
    }
}

/// TS `isAutoDreamEnabled` — gate for the AutoDream subsystem. Reads the env
/// flag `MOSSEN_AUTO_DREAM` (truthy values: `1`/`true`).
pub fn is_auto_dream_enabled() -> bool {
    matches!(
        std::env::var("MOSSEN_AUTO_DREAM").as_deref(),
        Ok("1" | "true" | "TRUE")
    )
}
