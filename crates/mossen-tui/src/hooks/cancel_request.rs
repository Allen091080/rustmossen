//! Cancel request hook (useCancelRequest.ts).
//!
//! Manages the state for cancelling an in-flight API request.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Cancellation token that can be shared across async boundaries.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// State for the cancel request hook.
#[derive(Debug, Clone)]
pub struct CancelRequestState {
    pub token: CancellationToken,
    pub is_cancelling: bool,
    pub cancel_count: u32,
}

impl CancelRequestState {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            is_cancelling: false,
            cancel_count: 0,
        }
    }

    /// Initiate cancellation of the current request.
    pub fn cancel(&mut self) {
        self.token.cancel();
        self.is_cancelling = true;
        self.cancel_count += 1;
    }

    /// Reset for a new request.
    pub fn reset(&mut self) {
        self.token = CancellationToken::new();
        self.is_cancelling = false;
    }

    /// Check if currently in cancelling state.
    pub fn is_pending_cancel(&self) -> bool {
        self.is_cancelling
    }
}

impl Default for CancelRequestState {
    fn default() -> Self {
        Self::new()
    }
}
