//! Reconciler (reconciler.ts).
//!
//! In TS this binds a React reconciler to the Ink DOM. The Rust port keeps
//! the public helper surface (profiling counters + owner chain accessors)
//! while leaving the actual render driver inside the renderer module.

#![allow(dead_code)]

use std::sync::Mutex;
use std::time::Instant;

use once_cell::sync::Lazy;

/// Capture the displayName chain of a React-like fiber. In Rust we have no
/// fiber, so callers pass an explicit Vec — we just deduplicate and clip.
pub fn get_owner_chain(input: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in input.iter().take(50) {
        if name.is_empty() {
            continue;
        }
        if out.last().map_or(true, |last| last != *name) {
            out.push((*name).to_string());
        }
    }
    out
}

/// Returns true when `MOSSEN_CODE_DEBUG_REPAINTS` env var is truthy.
pub fn is_debug_repaints_enabled() -> bool {
    match std::env::var("MOSSEN_CODE_DEBUG_REPAINTS") {
        Ok(v) => {
            let v = v.to_ascii_lowercase();
            !matches!(v.as_str(), "" | "0" | "false" | "no")
        }
        Err(_) => false,
    }
}

#[derive(Debug, Default)]
struct Profile {
    last_yoga_ms: f64,
    last_commit_ms: f64,
    commit_start: Option<Instant>,
}

static PROFILE: Lazy<Mutex<Profile>> = Lazy::new(|| Mutex::new(Profile::default()));

/// Store the most recent yoga layout duration (milliseconds).
pub fn record_yoga_ms(ms: f64) {
    if let Ok(mut p) = PROFILE.lock() {
        p.last_yoga_ms = ms;
    }
}

/// Read the most recent yoga layout duration.
pub fn get_last_yoga_ms() -> f64 {
    PROFILE.lock().ok().map(|p| p.last_yoga_ms).unwrap_or(0.0)
}

/// Mark the start of a commit phase.
pub fn mark_commit_start() {
    if let Ok(mut p) = PROFILE.lock() {
        p.commit_start = Some(Instant::now());
    }
}

/// Mark the end of a commit phase, storing the elapsed time.
pub fn mark_commit_end() {
    if let Ok(mut p) = PROFILE.lock() {
        if let Some(start) = p.commit_start.take() {
            p.last_commit_ms = start.elapsed().as_secs_f64() * 1_000.0;
        }
    }
}

/// Read the last commit duration in milliseconds.
pub fn get_last_commit_ms() -> f64 {
    PROFILE.lock().ok().map(|p| p.last_commit_ms).unwrap_or(0.0)
}

/// Reset all profiling counters between conversation turns.
pub fn reset_profile_counters() {
    if let Ok(mut p) = PROFILE.lock() {
        *p = Profile::default();
    }
}
