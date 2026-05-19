//! Sync cache state — in-memory state for remote managed settings sync.

use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;

/// Whether the user is eligible for remote managed settings.
static ELIGIBLE: AtomicBool = AtomicBool::new(false);

/// In-memory session state for sync.
static SESSION_STATE: once_cell::sync::Lazy<Mutex<SessionSyncState>> =
    once_cell::sync::Lazy::new(|| Mutex::new(SessionSyncState::default()));

#[derive(Debug, Default)]
struct SessionSyncState {
    checksum: Option<String>,
    fetch_in_progress: bool,
    last_error: Option<String>,
    cached_settings: Option<serde_json::Value>,
}

/// Check if remote managed settings are eligible for this user.
pub fn is_eligible() -> bool {
    ELIGIBLE.load(Ordering::SeqCst)
}

/// Set eligibility for remote managed settings.
pub fn set_eligible(eligible: bool) {
    ELIGIBLE.store(eligible, Ordering::SeqCst);
}

/// Get the current cached checksum.
pub fn get_cached_checksum() -> Option<String> {
    SESSION_STATE.lock().checksum.clone()
}

/// Update the cached checksum.
pub fn set_cached_checksum(checksum: Option<String>) {
    SESSION_STATE.lock().checksum = checksum;
}

/// Check if a fetch is currently in progress.
pub fn is_fetch_in_progress() -> bool {
    SESSION_STATE.lock().fetch_in_progress
}

/// Set fetch-in-progress flag.
pub fn set_fetch_in_progress(in_progress: bool) {
    SESSION_STATE.lock().fetch_in_progress = in_progress;
}

/// Get the last fetch error.
pub fn get_last_error() -> Option<String> {
    SESSION_STATE.lock().last_error.clone()
}

/// Set the last fetch error.
pub fn set_last_error(error: Option<String>) {
    SESSION_STATE.lock().last_error = error;
}

/// Reset all session state.
pub fn reset() {
    let mut state = SESSION_STATE.lock();
    state.checksum = None;
    state.fetch_in_progress = false;
    state.last_error = None;
    state.cached_settings = None;
    ELIGIBLE.store(false, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/remoteManagedSettings/syncCacheState.ts` exports.
// ---------------------------------------------------------------------------

/// `syncCacheState.ts` `setSessionCache`.
pub fn set_session_cache(settings: serde_json::Value, checksum: String) {
    let mut state = SESSION_STATE.lock();
    state.cached_settings = Some(settings);
    state.checksum = Some(checksum);
}

/// `syncCacheState.ts` `resetSyncCache`.
pub fn reset_sync_cache() {
    reset();
}

/// `syncCacheState.ts` `setEligibility`.
pub fn set_eligibility(eligible: bool) {
    ELIGIBLE.store(eligible, Ordering::SeqCst);
}

/// `syncCacheState.ts` `getSettingsPath`.
pub fn get_settings_path() -> std::path::PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".mossen")
        .join("remote-managed-settings.json")
}

/// `syncCacheState.ts` `getCachedSettings`.
pub fn get_cached_settings() -> Option<serde_json::Value> {
    SESSION_STATE.lock().cached_settings.clone()
}

/// TS `getRemoteManagedSettingsSyncFromCache` — returns the cached settings
/// payload (or `None` when no cache exists). Mirrors the TS export name.
pub fn get_remote_managed_settings_sync_from_cache() -> Option<serde_json::Value> {
    crate::services::remote_managed_settings::sync_cache_state::get_cached_settings()
}
