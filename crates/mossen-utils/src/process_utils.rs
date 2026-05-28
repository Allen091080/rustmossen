//! Compatibility shim — historical alias for `utils/process.ts`. Same
//! contract as [`crate::process_io`] — re-exports the canonical helpers from
//! [`crate::process`] so all EPIPE / SIGPIPE handling shares one source of
//! truth.

pub use crate::process::{
    exit_with_error, is_stderr_destroyed, is_stdout_destroyed,
    register_process_output_error_handlers, write_to_stderr, write_to_stdout,
};

/// Adapter that omits the explicit `stream` argument, defaulting to
/// `tokio::io::stdin()` — keeps the older two-arg-less call site shape working.
pub async fn peek_for_stdin_data(ms: u64) -> bool {
    crate::process::peek_for_stdin_data(tokio::io::stdin(), ms).await
}
