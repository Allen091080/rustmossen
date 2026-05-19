//! Auto dream service — periodic background memory consolidation.
//!
//! Runs a consolidation agent during idle periods to merge, deduplicate,
//! and organize accumulated memories.

pub mod config;
pub mod consolidation_lock;
pub mod consolidation_prompt;

use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

pub use config::AutoDreamConfig;

/// Auto dream scheduler state.
pub struct AutoDreamScheduler {
    config: AutoDreamConfig,
    last_run: Mutex<Option<Instant>>,
    running: Mutex<bool>,
}

impl AutoDreamScheduler {
    pub fn new(config: AutoDreamConfig) -> Self {
        Self {
            config,
            last_run: Mutex::new(None),
            running: Mutex::new(false),
        }
    }

    /// Check if consolidation should run now.
    pub async fn should_run(&self) -> bool {
        if !self.config.enabled {
            return false;
        }

        let running = self.running.lock().await;
        if *running {
            return false;
        }

        let last = self.last_run.lock().await;
        match *last {
            Some(t) => t.elapsed() >= self.config.interval,
            None => true,
        }
    }

    /// Run the consolidation agent.
    pub async fn run_consolidation(&self) -> Result<ConsolidationResult, String> {
        let mut running = self.running.lock().await;
        if *running {
            return Err("Consolidation already running".to_string());
        }
        *running = true;
        drop(running);

        let result = self.execute_consolidation().await;

        let mut running = self.running.lock().await;
        *running = false;
        let mut last = self.last_run.lock().await;
        *last = Some(Instant::now());

        result
    }

    async fn execute_consolidation(&self) -> Result<ConsolidationResult, String> {
        debug!("auto-dream: starting consolidation");

        // In production, this would:
        // 1. Acquire a consolidation lock
        // 2. Read all memory files
        // 3. Run the consolidation agent to merge/deduplicate
        // 4. Write updated files
        // 5. Release the lock

        Ok(ConsolidationResult {
            files_merged: 0,
            files_deleted: 0,
            files_updated: 0,
            duration: Duration::from_secs(0),
        })
    }
}

/// Result of a consolidation run.
#[derive(Debug, Clone)]
pub struct ConsolidationResult {
    pub files_merged: usize,
    pub files_deleted: usize,
    pub files_updated: usize,
    pub duration: Duration,
}
