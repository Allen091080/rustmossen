//! Process output and error handling — canonical port of `utils/process.ts`.
//!
//! TS attaches an `error` listener on `process.stdout` / `process.stderr` so a
//! broken pipe (e.g. `mossen -p | head -1`) destroys the stream and the writer
//! stops queuing data. The Rust equivalent is twofold:
//!   * install `SIGPIPE → SIG_IGN` on Unix so the kernel no longer kills the
//!     process when a downstream reader closes its end of the pipe
//!   * latch a per-stream `destroyed` flag the first time `write_all` returns
//!     `ErrorKind::BrokenPipe`, then short-circuit subsequent writes
//!
//! `process_io.rs` and `process_utils.rs` are kept as thin re-export shims for
//! call sites that already imported them under their historical names.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncRead as TokioAsyncRead, AsyncReadExt};
use tokio::time::sleep;

/// `true` after stdout has produced a `BrokenPipe` error. Subsequent writes are
/// dropped silently, matching the `stream.destroyed` short-circuit in TS.
static STDOUT_DESTROYED: AtomicBool = AtomicBool::new(false);

/// `true` after stderr has produced a `BrokenPipe` error.
static STDERR_DESTROYED: AtomicBool = AtomicBool::new(false);

/// Returns whether the given error is a broken-pipe (EPIPE) error.
fn handle_epipe(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::BrokenPipe
}

/// Register process-output error handlers — installs `SIGPIPE → SIG_IGN` so a
/// closed downstream pipe surfaces as `BrokenPipe` on `write_all` instead of
/// terminating the process, and clears the per-stream destroyed flags.
///
/// Mirrors `registerProcessOutputErrorHandlers` in `utils/process.ts`.
pub fn register_process_output_error_handlers() {
    #[cfg(unix)]
    unsafe {
        // SAFETY: libc::signal is async-signal-safe; SIG_IGN is a valid handler.
        // Doing this exactly once at startup is the canonical Unix pattern.
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    STDOUT_DESTROYED.store(false, Ordering::SeqCst);
    STDERR_DESTROYED.store(false, Ordering::SeqCst);
}

/// Returns whether stdout has been latched as destroyed by an earlier EPIPE.
pub fn is_stdout_destroyed() -> bool {
    STDOUT_DESTROYED.load(Ordering::SeqCst)
}

/// Returns whether stderr has been latched as destroyed by an earlier EPIPE.
pub fn is_stderr_destroyed() -> bool {
    STDERR_DESTROYED.load(Ordering::SeqCst)
}

/// Write `data` to an arbitrary `Write` stream. Returns the underlying `io`
/// error (including `BrokenPipe`) unchanged — callers decide how to react.
pub fn write_out(stream: &mut dyn Write, data: &str) -> io::Result<()> {
    stream.write_all(data.as_bytes())
}

/// Write `data` to stdout. Drops the write silently once stdout has been
/// destroyed by an earlier EPIPE; otherwise latches the destroyed flag on the
/// first `BrokenPipe` it sees.
pub fn write_to_stdout(data: &str) {
    if STDOUT_DESTROYED.load(Ordering::SeqCst) {
        return;
    }
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    if let Err(e) = handle.write_all(data.as_bytes()) {
        if handle_epipe(&e) {
            STDOUT_DESTROYED.store(true, Ordering::SeqCst);
        }
    }
}

/// Write `data` to stderr. Same EPIPE-latching semantics as `write_to_stdout`.
pub fn write_to_stderr(data: &str) {
    if STDERR_DESTROYED.load(Ordering::SeqCst) {
        return;
    }
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    if let Err(e) = handle.write_all(data.as_bytes()) {
        if handle_epipe(&e) {
            STDERR_DESTROYED.store(true, Ordering::SeqCst);
        }
    }
}

/// Print `message` to stderr (or drop it silently if stderr is destroyed) and
/// exit with code 1. Used by entry-point fast paths.
pub fn exit_with_error(message: &str) -> ! {
    write_to_stderr(message);
    write_to_stderr("\n");
    std::process::exit(1)
}

/// Wait for `stream` to either deliver a first chunk of data or close. Returns
/// `true` on timeout (no data arrived within `ms`), `false` once the stream
/// ends. After the first data chunk the timeout is cancelled and we drain to
/// EOF unconditionally — matching the TS `-p` mode behaviour where the caller
/// accumulator needs every chunk, not just the first.
pub async fn peek_for_stdin_data(stream: impl TokioAsyncRead + Unpin, ms: u64) -> bool {
    let mut stream = stream;
    let mut buf = [0u8; 1];

    let first = tokio::select! {
        biased;
        result = stream.read(&mut buf) => match result {
            Ok(0) => Some(false), // EOF before any data — not a timeout
            Ok(_) => None,        // got a byte — drain rest below
            Err(_) => Some(false),
        },
        _ = sleep(Duration::from_millis(ms)) => Some(true),
    };
    if let Some(outcome) = first {
        return outcome;
    }

    let mut discard = [0u8; 4096];
    loop {
        match stream.read(&mut discard).await {
            Ok(0) => return false,
            Ok(_) => continue,
            Err(_) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_epipe_classifies_broken_pipe() {
        let broken_pipe = io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe");
        assert!(handle_epipe(&broken_pipe));

        let other_error = io::Error::new(io::ErrorKind::Other, "other");
        assert!(!handle_epipe(&other_error));
    }

    #[test]
    fn write_out_appends_to_buffer() {
        let mut buffer = Vec::new();
        write_out(&mut buffer, "hello").unwrap();
        assert_eq!(buffer, b"hello");
    }

    #[test]
    fn register_clears_destroyed_flags() {
        STDOUT_DESTROYED.store(true, Ordering::SeqCst);
        STDERR_DESTROYED.store(true, Ordering::SeqCst);
        register_process_output_error_handlers();
        assert!(!is_stdout_destroyed());
        assert!(!is_stderr_destroyed());
    }
}
