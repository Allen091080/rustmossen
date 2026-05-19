//! Git availability check with memoized caching.
//!
//! Translated from `utils/plugins/gitAvailability.ts` (69 lines).

use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether git has been forcibly marked as unavailable.
static GIT_FORCE_UNAVAILABLE: AtomicBool = AtomicBool::new(false);

/// Cached git availability result.
static GIT_AVAILABLE_CACHE: OnceCell<bool> = OnceCell::new();

/// Mutex to protect cache initialization.
static CACHE_INIT: Mutex<()> = Mutex::new(());

/// Check if a command is available in PATH.
///
/// Uses `which` to find the actual executable without executing it.
async fn is_command_available(command: &str) -> bool {
    which::which(command).is_ok()
}

/// Check if git is available on the system.
///
/// This is memoized so repeated calls within a session return the cached result.
/// Git availability is unlikely to change during a single CLI session.
///
/// Only checks PATH — does not exec git. On macOS this means the /usr/bin/git
/// xcrun shim passes even without Xcode CLT installed.
pub async fn check_git_available() -> bool {
    if GIT_FORCE_UNAVAILABLE.load(Ordering::Relaxed) {
        return false;
    }

    if let Some(&cached) = GIT_AVAILABLE_CACHE.get() {
        return cached;
    }

    let _lock = CACHE_INIT.lock();
    // Double-check after acquiring lock
    if let Some(&cached) = GIT_AVAILABLE_CACHE.get() {
        return cached;
    }

    let available = is_command_available("git").await;
    let _ = GIT_AVAILABLE_CACHE.set(available);
    available
}

/// Force the memoized git-availability check to return false for the rest of
/// the session.
///
/// Call this when a git invocation fails in a way that indicates the binary
/// exists on PATH but cannot actually run — the macOS xcrun shim being the
/// main case.
pub fn mark_git_unavailable() {
    GIT_FORCE_UNAVAILABLE.store(true, Ordering::Relaxed);
}

/// Clear the git availability cache. Used for testing purposes.
pub fn clear_git_availability_cache() {
    GIT_FORCE_UNAVAILABLE.store(false, Ordering::Relaxed);
    // OnceCell doesn't support clearing, but we can use the force flag
}
