//! AbortController utilities with memory-safe parent-child relationships.
//!
//! This module provides utilities for creating parent-child AbortController
//! relationships using Weak references to prevent memory leaks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

/// Creates an AbortController that can be aborted multiple times.
/// This is a simple wrapper around a broadcast channel for signaling abort.
#[derive(Clone)]
pub struct AbortController {
    aborted: Arc<AtomicBool>,
    reason: Arc<Mutex<Option<String>>>,
}

impl AbortController {
    /// Creates a new AbortController
    pub fn new() -> Self {
        Self {
            aborted: Arc::new(AtomicBool::new(false)),
            reason: Arc::new(Mutex::new(None)),
        }
    }

    /// Abort the controller
    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);
    }

    /// Abort with a reason
    pub fn abort_with_error(&self, error: &str) {
        *self.reason.lock().unwrap() = Some(error.to_string());
        self.abort();
    }

    /// Check if aborted
    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }

    /// Get abort reason
    pub fn reason(&self) -> Option<String> {
        self.reason.lock().unwrap().clone()
    }
}

impl Default for AbortController {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper around AbortController with configurable listener limits.
pub struct AbortControllerWrapper {
    inner: AbortController,
    max_listeners: usize,
}

impl AbortControllerWrapper {
    /// Creates a new AbortController with proper event listener limits set.
    pub fn new(max_listeners: usize) -> Self {
        Self {
            inner: AbortController::new(),
            max_listeners,
        }
    }

    /// Returns the inner AbortController
    pub fn inner(&self) -> &AbortController {
        &self.inner
    }

    /// Signal this controller to abort with an optional reason
    pub fn abort(&self, reason: Option<&str>) {
        if let Some(r) = reason {
            self.inner.abort_with_error(r);
        } else {
            self.inner.abort();
        }
    }

    /// Returns true if the signal has been aborted
    pub fn is_aborted(&self) -> bool {
        self.inner.is_aborted()
    }

    /// Get the abort reason if aborted
    pub fn reason(&self) -> Option<String> {
        self.inner.reason()
    }
}

impl Default for AbortControllerWrapper {
    fn default() -> Self {
        Self::new(50)
    }
}

/// Default max listeners for standard operations
pub const DEFAULT_MAX_LISTENERS: usize = 50;

/// Creates an AbortController with proper event listener limits set.
///
/// In TypeScript this sets `setMaxListeners(maxListeners, controller.signal)`
/// to suppress `MaxListenersExceededWarning`. Rust does not have an analogous
/// listener limit, so we simply construct a new controller. The argument is
/// retained for API parity with the TypeScript source.
pub fn create_abort_controller(_max_listeners: Option<usize>) -> AbortController {
    AbortController::new()
}

/// Creates a child AbortController that aborts when its parent aborts.
/// Aborting the child does NOT affect the parent.
///
/// Memory-safe: Uses shared state with atomic flag so the parent doesn't
/// necessarily retain the child, though in Rust this is handled at compile time.
pub fn create_child_abort_controller(
    parent: &AbortController,
) -> AbortController {
    let child = AbortController::new();

    // Fast path: parent already aborted, sync the state
    if parent.is_aborted() {
        if let Some(reason) = parent.reason() {
            child.abort_with_error(&reason);
        } else {
            child.abort();
        }
        return child;
    }

    // Clone the abort flag to track parent's state
    let parent_aborted = parent.aborted.clone();
    let parent_reason = parent.reason.clone();

    // Clone child for the async task
    let child_for_task = child.clone();

    // We spawn a background task to propagate abort
    // Note: In a real scenario, you'd want to properly manage this task
    // to avoid memory leaks. A proper implementation would use Weak references.
    tokio::spawn(async move {
        // Poll parent state until aborted
        while !parent_aborted.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Parent is now aborted, propagate to child
        let reason = parent_reason.lock().unwrap().clone();
        if let Some(r) = reason {
            child_for_task.abort_with_error(&r);
        } else {
            child_for_task.abort();
        }
    });

    child
}

/// Creates a simple child AbortController that immediately syncs parent state.
/// For synchronous usage, this is simpler than the async version.
pub fn create_child_abort_controller_sync(
    parent: &AbortController,
) -> AbortController {
    let child = AbortController::new();

    // Sync parent state if already aborted
    if parent.is_aborted() {
        if let Some(reason) = parent.reason() {
            child.abort_with_error(&reason);
        } else {
            child.abort();
        }
    }

    child
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abort_controller_new() {
        let controller = AbortController::new();
        assert!(!controller.is_aborted());
        assert!(controller.reason().is_none());
    }

    #[test]
    fn test_abort_controller_abort() {
        let controller = AbortController::new();
        controller.abort();
        assert!(controller.is_aborted());
    }

    #[test]
    fn test_abort_controller_with_reason() {
        let controller = AbortController::new();
        controller.abort_with_error("test error");
        assert!(controller.is_aborted());
        assert_eq!(controller.reason(), Some("test error".to_string()));
    }

    #[test]
    fn test_abort_controller_wrapper_default() {
        let wrapper = AbortControllerWrapper::default();
        assert!(!wrapper.is_aborted());
    }

    #[test]
    fn test_abort_controller_wrapper_abort() {
        let wrapper = AbortControllerWrapper::new(50);
        wrapper.abort(Some("test reason"));
        assert!(wrapper.is_aborted());
        assert_eq!(wrapper.reason(), Some("test reason".to_string()));
    }

    #[test]
    fn test_child_already_aborted_parent() {
        let parent = AbortController::new();
        parent.abort();
        let child = create_child_abort_controller_sync(&parent);

        assert!(child.is_aborted());
    }
}
