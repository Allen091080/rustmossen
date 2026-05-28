//! Compact warning hook — subscription to compact warning suppression state.
//!
//! Translates: services/compact/compactWarningHook.ts
//! Original was a React hook using useSyncExternalStore. In Rust this
//! becomes a simple state query function on the existing compact_warning_state module.

use super::compact_warning_state::CompactWarningStore;

/// Check whether compact warnings are currently suppressed.
///
/// In the TS version this was a React hook (`useCompactWarningSuppression`).
/// In Rust, this is a synchronous query on the store.
pub fn is_compact_warning_suppressed(store: &CompactWarningStore) -> bool {
    store.get_state()
}

/// TS `useCompactWarningSuppression()` — returns whether the compact warning
/// is currently suppressed for the active session.
pub fn use_compact_warning_suppression(store: &CompactWarningStore) -> bool {
    is_compact_warning_suppressed(store)
}
