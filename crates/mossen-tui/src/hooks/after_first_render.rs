//! After first render hook (useAfterFirstRender.ts).
//!
//! Fires a one-shot callback after the first render cycle completes.
//! In the TS version this checked env vars and called process.exit for
//! startup-time measurement.

use std::sync::atomic::{AtomicBool, Ordering};

/// State for the after-first-render hook.
#[derive(Debug)]
pub struct AfterFirstRenderState {
    fired: AtomicBool,
}

impl AfterFirstRenderState {
    pub fn new() -> Self {
        Self {
            fired: AtomicBool::new(false),
        }
    }

    /// Call once after the first render. Returns true if this was the first call.
    pub fn mark_rendered(&self) -> bool {
        !self.fired.swap(true, Ordering::SeqCst)
    }

    /// Check if the first render has occurred.
    pub fn has_rendered(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }

    /// Check if we should exit after first render (internal user type + env flag).
    pub fn should_exit_after_render() -> bool {
        let user_type = std::env::var("USER_TYPE").unwrap_or_default();
        let exit_flag = std::env::var("MOSSEN_CODE_EXIT_AFTER_FIRST_RENDER").unwrap_or_default();
        user_type == "internal" && (exit_flag == "1" || exit_flag.eq_ignore_ascii_case("true"))
    }

    /// Get startup time in milliseconds (from process start).
    pub fn startup_time_ms() -> u64 {
        // Use std::time::SystemTime as approximation
        // In production this would use process uptime
        0
    }
}

impl Default for AfterFirstRenderState {
    fn default() -> Self {
        Self::new()
    }
}
