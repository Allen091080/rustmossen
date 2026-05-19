//! Graceful shutdown — terminal cleanup and process exit handling.
//!
//! Translated from utils/gracefulShutdown.ts

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use anyhow::anyhow;

/// Error type for cleanup timeout.
#[derive(Debug)]
pub struct CleanupTimeoutError;

impl std::fmt::Display for CleanupTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cleanup timeout")
    }
}

impl std::error::Error for CleanupTimeoutError {}

static SHUTDOWN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static RESUME_HINT_PRINTED: AtomicBool = AtomicBool::new(false);

/// Terminal escape sequences for cleanup.
pub mod terminal_sequences {
    pub const DISABLE_MOUSE_TRACKING: &str = "\x1b[?1000l\x1b[?1002l\x1b[?1003l";
    pub const EXIT_ALT_SCREEN: &str = "\x1b[?1049l";
    pub const DISABLE_MODIFY_OTHER_KEYS: &str = "\x1b[>4;0m";
    pub const DISABLE_KITTY_KEYBOARD: &str = "\x1b[>u";
    pub const DISABLE_FOCUS_EVENTS: &str = "\x1b[?1004l";
    pub const DISABLE_BRACKETED_PASTE: &str = "\x1b[?2004l";
    pub const SHOW_CURSOR: &str = "\x1b[?25h";
    pub const CLEAR_TERMINAL_TITLE: &str = "\x1b]0;\x07";
}

/// Clean up terminal modes synchronously before process exit.
pub fn cleanup_terminal_modes() {
    use std::io::Write;

    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return;
    }

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    let _ = handle.write_all(terminal_sequences::DISABLE_MOUSE_TRACKING.as_bytes());
    let _ = handle.write_all(terminal_sequences::EXIT_ALT_SCREEN.as_bytes());
    let _ = handle.write_all(terminal_sequences::DISABLE_MODIFY_OTHER_KEYS.as_bytes());
    let _ = handle.write_all(terminal_sequences::DISABLE_KITTY_KEYBOARD.as_bytes());
    let _ = handle.write_all(terminal_sequences::DISABLE_FOCUS_EVENTS.as_bytes());
    let _ = handle.write_all(terminal_sequences::DISABLE_BRACKETED_PASTE.as_bytes());
    let _ = handle.write_all(terminal_sequences::SHOW_CURSOR.as_bytes());
    let _ = handle.write_all(terminal_sequences::CLEAR_TERMINAL_TITLE.as_bytes());
    let _ = handle.flush();
}

/// Print a hint about how to resume the session.
pub fn print_resume_hint(session_id: &str, custom_title: Option<&str>, is_interactive: bool) {
    if RESUME_HINT_PRINTED.swap(true, Ordering::SeqCst) {
        return;
    }

    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) || !is_interactive {
        return;
    }

    use std::io::Write;
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    let resume_arg = if let Some(title) = custom_title {
        let escaped = title.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    } else {
        session_id.to_string()
    };

    let _ = writeln!(handle, "\nResume this session with:\nmossen --resume {}", resume_arg);
}

/// Force process exit.
pub fn force_exit(exit_code: i32) -> ! {
    std::process::exit(exit_code)
}

/// Set up global signal handlers for graceful shutdown.
pub fn setup_graceful_shutdown<F>(shutdown_handler: F)
where
    F: Fn(i32) + Send + Sync + 'static,
{
    let handler = std::sync::Arc::new(shutdown_handler);

    // SIGINT handler - callers should use tokio::signal or signal-hook
    // to register the actual handler at the application level.
    let _ = handler;

    // Note: SIGTERM and SIGHUP handlers would require platform-specific code
    // In production, use signal-hook or tokio::signal
}

/// Sync version of graceful shutdown.
pub fn graceful_shutdown_sync(exit_code: i32) {
    if SHUTDOWN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }
    cleanup_terminal_modes();
}

/// Check if graceful shutdown is in progress.
pub fn is_shutting_down() -> bool {
    SHUTDOWN_IN_PROGRESS.load(Ordering::SeqCst)
}

/// Reset shutdown state — only for use in tests.
pub fn reset_shutdown_state() {
    SHUTDOWN_IN_PROGRESS.store(false, Ordering::SeqCst);
    RESUME_HINT_PRINTED.store(false, Ordering::SeqCst);
}

/// Exit reason enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    UserExit,
    Signal,
    Error,
    Other,
}

/// Graceful shutdown function that performs async cleanup.
pub async fn graceful_shutdown(
    exit_code: i32,
    _reason: ExitReason,
    cleanup_fns: Vec<Box<dyn FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send>>,
    session_id: &str,
    custom_title: Option<&str>,
    is_interactive: bool,
    final_message: Option<&str>,
) {
    if SHUTDOWN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }

    // Exit alt screen and print resume hint FIRST
    cleanup_terminal_modes();
    print_resume_hint(session_id, custom_title, is_interactive);

    // Run cleanup functions with timeout
    let cleanup_future = async {
        for cleanup_fn in cleanup_fns {
            let fut = cleanup_fn();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), fut).await;
        }
    };

    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), cleanup_future).await;

    // Print final message if provided
    if let Some(msg) = final_message {
        eprintln!("{}", msg);
    }

    force_exit(exit_code)
}

/// Get the pending shutdown for testing.
pub fn get_pending_shutdown_for_testing() -> bool {
    SHUTDOWN_IN_PROGRESS.load(Ordering::SeqCst)
}
