//! Compact warning state — tracks whether the "context left until autocompact" warning
//! should be suppressed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;

/// Store for compact warning suppression state.
#[derive(Debug, Clone)]
pub struct CompactWarningStore {
    state: Arc<AtomicBool>,
    sender: Arc<watch::Sender<bool>>,
    receiver: watch::Receiver<bool>,
}

impl CompactWarningStore {
    /// Create a new compact warning store with initial value false.
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self {
            state: Arc::new(AtomicBool::new(false)),
            sender: Arc::new(sender),
            receiver,
        }
    }

    /// Get the current suppression state.
    pub fn get_state(&self) -> bool {
        self.state.load(Ordering::Relaxed)
    }

    /// Set the state and notify subscribers.
    pub fn set_state(&self, value: bool) {
        self.state.store(value, Ordering::Relaxed);
        let _ = self.sender.send(value);
    }

    /// Subscribe to state changes.
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.receiver.clone()
    }
}

impl Default for CompactWarningStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Global compact warning store instance.
static COMPACT_WARNING_SUPPRESSED: AtomicBool = AtomicBool::new(false);

/// Suppress the compact warning. Call after successful compaction.
pub fn suppress_compact_warning() {
    COMPACT_WARNING_SUPPRESSED.store(true, Ordering::Relaxed);
}

/// Clear the compact warning suppression. Called at start of new compact attempt.
pub fn clear_compact_warning_suppression() {
    COMPACT_WARNING_SUPPRESSED.store(false, Ordering::Relaxed);
}

/// Check if compact warning is currently suppressed.
pub fn is_compact_warning_suppressed() -> bool {
    COMPACT_WARNING_SUPPRESSED.load(Ordering::Relaxed)
}
