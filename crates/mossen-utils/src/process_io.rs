//! Compatibility shim — historical alias for `utils/process.ts`. All symbols
//! re-export the canonical implementations in [`crate::process`] so EPIPE
//! latching and SIGPIPE setup live in one place.

pub use crate::process::{
    exit_with_error, is_stderr_destroyed, is_stdout_destroyed,
    register_process_output_error_handlers, write_to_stderr, write_to_stdout,
};

/// Adapter that hides the `stream` argument carried by the canonical helper —
/// historical callers passed only the timeout and relied on `tokio::io::stdin`
/// being used implicitly.
pub async fn peek_for_stdin_data(timeout_ms: u64) -> bool {
    crate::process::peek_for_stdin_data(tokio::io::stdin(), timeout_ms).await
}
