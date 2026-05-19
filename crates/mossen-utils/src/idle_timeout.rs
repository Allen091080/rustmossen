use std::env;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::debug;

/// Idle timeout manager for SDK mode.
/// Automatically triggers shutdown after the specified idle duration.
pub struct IdleTimeoutManager {
    delay_ms: Option<u64>,
    is_idle: Arc<dyn Fn() -> bool + Send + Sync>,
    state: Arc<Mutex<IdleState>>,
}

struct IdleState {
    abort_handle: Option<tokio::task::AbortHandle>,
    last_idle_time: Option<Instant>,
}

impl IdleTimeoutManager {
    /// Creates an idle timeout manager for SDK mode.
    /// Automatically exits the process after the specified idle duration.
    ///
    /// `is_idle` - Function that returns true if the system is currently idle.
    pub fn new(is_idle: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        // Parse MOSSEN_CODE_EXIT_AFTER_STOP_DELAY environment variable
        let delay_ms = env::var("MOSSEN_CODE_EXIT_AFTER_STOP_DELAY")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&d| d > 0);

        Self {
            delay_ms,
            is_idle,
            state: Arc::new(Mutex::new(IdleState {
                abort_handle: None,
                last_idle_time: None,
            })),
        }
    }

    /// Start the idle timer. If already running, resets it.
    pub async fn start(&self) {
        let mut state = self.state.lock().await;

        // Clear any existing timer
        if let Some(handle) = state.abort_handle.take() {
            handle.abort();
        }

        // Only start timer if delay is configured and valid
        if let Some(delay_ms) = self.delay_ms {
            let now = Instant::now();
            state.last_idle_time = Some(now);

            let is_idle = self.is_idle.clone();
            let delay = Duration::from_millis(delay_ms);
            let state_ref = self.state.clone();

            let handle = tokio::spawn(async move {
                tokio::time::sleep(delay).await;

                let state = state_ref.lock().await;
                if let Some(last_idle) = state.last_idle_time {
                    let idle_duration = last_idle.elapsed();
                    if (is_idle)() && idle_duration >= delay {
                        debug!("Exiting after {}ms of idle time", delay_ms);
                        // In Rust, we signal graceful shutdown rather than calling process::exit
                        // The caller should set up a shutdown signal handler
                        std::process::exit(0);
                    }
                }
            });

            state.abort_handle = Some(handle.abort_handle());
        }
    }

    /// Stop the idle timer.
    pub async fn stop(&self) {
        let mut state = self.state.lock().await;
        if let Some(handle) = state.abort_handle.take() {
            handle.abort();
        }
        state.last_idle_time = None;
    }
}

/// Creates an idle timeout manager for SDK mode.
pub fn create_idle_timeout_manager(
    is_idle: Arc<dyn Fn() -> bool + Send + Sync>,
) -> IdleTimeoutManager {
    IdleTimeoutManager::new(is_idle)
}
