//! Post-compact cleanup — frees caches and tracking state invalidated by compaction.

use super::compact_warning_state::suppress_compact_warning;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostCompactCleanupOutcome {
    pub main_thread_compact: bool,
    pub microcompact_state_reset: bool,
    pub compact_warning_suppressed: bool,
}

/// Run cleanup of caches and tracking state after compaction.
/// Call this after both auto-compact and manual /compact to free memory
/// held by tracking structures that are invalidated by compaction.
pub fn run_post_compact_cleanup(query_source: Option<&str>) -> PostCompactCleanupOutcome {
    let is_main_thread_compact = match query_source {
        None => true,
        Some(s) => s.starts_with("repl_main_thread") || s == "sdk",
    };

    super::micro_compact::reset_microcompact_state();
    suppress_compact_warning();

    if is_main_thread_compact {
        // In production, clear getUserContext cache, resetGetMemoryFilesCache, etc.
        // These are application-level caches that don't exist in this Rust layer yet.
    }

    // Clear system prompt sections cache
    // clearSystemPromptSections()
    // clearClassifierApprovals()
    // clearSpeculativeChecks()
    // clearSessionMessagesCache()
    PostCompactCleanupOutcome {
        main_thread_compact: is_main_thread_compact,
        microcompact_state_reset: true,
        compact_warning_suppressed: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_compact_cleanup_reports_lifecycle_effects() {
        let outcome = run_post_compact_cleanup(Some("repl_main_thread"));

        assert_eq!(
            outcome,
            PostCompactCleanupOutcome {
                main_thread_compact: true,
                microcompact_state_reset: true,
                compact_warning_suppressed: true,
            }
        );
    }
}
