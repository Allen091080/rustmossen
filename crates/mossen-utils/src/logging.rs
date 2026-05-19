//! Logging infrastructure.
//!
//! Provides structured logging via `tracing`, error log sinks, and
//! debug-level logging utilities.

use std::sync::OnceLock;

use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Error logging
// ---------------------------------------------------------------------------

/// Maximum number of errors kept in the in-memory ring buffer.
const MAX_IN_MEMORY_ERRORS: usize = 100;

/// In-memory error log entry.
#[derive(Debug, Clone)]
pub struct ErrorLogEntry {
    pub error: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Thread-safe in-memory error ring buffer.
static ERROR_LOG: OnceLock<parking_lot::Mutex<Vec<ErrorLogEntry>>> = OnceLock::new();

fn error_log() -> &'static parking_lot::Mutex<Vec<ErrorLogEntry>> {
    ERROR_LOG.get_or_init(|| parking_lot::Mutex::new(Vec::with_capacity(MAX_IN_MEMORY_ERRORS)))
}

/// Log an error to both tracing and the in-memory ring buffer.
pub fn log_error(err: &dyn std::error::Error) {
    let msg = err.to_string();
    error!("{}", msg);
    add_to_in_memory_error_log(&msg);
}

/// Log an error message string.
pub fn log_error_msg(msg: &str) {
    error!("{}", msg);
    add_to_in_memory_error_log(msg);
}

fn add_to_in_memory_error_log(msg: &str) {
    let mut log = error_log().lock();
    if log.len() >= MAX_IN_MEMORY_ERRORS {
        log.remove(0);
    }
    log.push(ErrorLogEntry {
        error: msg.to_string(),
        timestamp: chrono::Utc::now(),
    });
}

/// Get a snapshot of the in-memory error log.
pub fn get_error_log_snapshot() -> Vec<ErrorLogEntry> {
    error_log().lock().clone()
}

/// Clear the in-memory error log.
pub fn clear_error_log() {
    error_log().lock().clear();
}

// ---------------------------------------------------------------------------
// Debug logging
// ---------------------------------------------------------------------------

/// Log a debug message (conditional on MOSSEN_DEBUG or tracing filter).
pub fn log_for_debugging(msg: &str) {
    debug!("{}", msg);
}

/// Log an info-level message.
pub fn log_info(msg: &str) {
    info!("{}", msg);
}

/// Log a warning.
pub fn log_warn(msg: &str) {
    warn!("{}", msg);
}

// ---------------------------------------------------------------------------
// Tracing initialization
// ---------------------------------------------------------------------------

/// Initialize the default tracing subscriber with env-filter support.
/// Call once at application startup.
///
/// Respects `RUST_LOG` env var for filtering.
pub fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

/// Initialize tracing with JSON output (for structured log ingestion).
pub fn init_tracing_json() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt().with_env_filter(filter).json().init();
}
