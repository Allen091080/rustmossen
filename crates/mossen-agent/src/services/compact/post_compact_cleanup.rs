//! Post-compact cleanup — frees caches and tracking state invalidated by compaction.

/// Run cleanup of caches and tracking state after compaction.
/// Call this after both auto-compact and manual /compact to free memory
/// held by tracking structures that are invalidated by compaction.
pub fn run_post_compact_cleanup(query_source: Option<&str>) {
    let is_main_thread_compact = match query_source {
        None => true,
        Some(s) => s.starts_with("repl_main_thread") || s == "sdk",
    };

    super::micro_compact::reset_microcompact_state();

    if is_main_thread_compact {
        // In production, clear getUserContext cache, resetGetMemoryFilesCache, etc.
        // These are application-level caches that don't exist in this Rust layer yet.
    }

    // Clear system prompt sections cache
    // clearSystemPromptSections()
    // clearClassifierApprovals()
    // clearSpeculativeChecks()
    // clearSessionMessagesCache()
}
