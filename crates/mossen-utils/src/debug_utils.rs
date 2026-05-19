//! Debug logging utilities.
//!
//! Provides debug mode detection, filtering, log level control, and buffered
//! debug log writing to files with symlink management.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

/// Debug log levels ordered by severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DebugLogLevel {
    Verbose = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl DebugLogLevel {
    /// Parse a string into a log level, case-insensitive.
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "verbose" => Some(Self::Verbose),
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// Convert to uppercase string for log output.
    pub fn as_str_upper(&self) -> &'static str {
        match self {
            Self::Verbose => "VERBOSE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

/// A parsed debug filter pattern for selective debug output.
#[derive(Debug, Clone)]
pub struct DebugFilter {
    /// Patterns to include (empty = include all).
    pub include: Vec<String>,
    /// Patterns to exclude.
    pub exclude: Vec<String>,
}

/// Parse a debug filter string into include/exclude patterns.
///
/// Format: comma-separated patterns. Prefix with `-` to exclude.
/// Example: `"api,network,-verbose"` includes "api" and "network", excludes "verbose".
pub fn parse_debug_filter(pattern: &str) -> DebugFilter {
    let mut include = Vec::new();
    let mut exclude = Vec::new();

    for part in pattern.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(stripped) = trimmed.strip_prefix('-') {
            if !stripped.is_empty() {
                exclude.push(stripped.to_string());
            }
        } else {
            include.push(trimmed.to_string());
        }
    }

    DebugFilter { include, exclude }
}

/// Check if a message should be shown based on a debug filter.
pub fn should_show_debug_message(message: &str, filter: Option<&DebugFilter>) -> bool {
    let Some(filter) = filter else {
        return true;
    };

    // Check excludes first
    for pattern in &filter.exclude {
        if message.contains(pattern.as_str()) {
            return false;
        }
    }

    // If no include patterns, show everything (that wasn't excluded)
    if filter.include.is_empty() {
        return true;
    }

    // Check if message matches any include pattern
    for pattern in &filter.include {
        if message.contains(pattern.as_str()) {
            return true;
        }
    }

    false
}

/// Runtime flag for debug mode enabled via /debug command.
static RUNTIME_DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether formatted output is being used (affects multiline log handling).
static HAS_FORMATTED_OUTPUT: AtomicBool = AtomicBool::new(false);

/// Get the minimum debug log level from environment variable.
///
/// Defaults to `Debug`, which filters out `Verbose` messages.
/// Set `MOSSEN_CODE_DEBUG_LOG_LEVEL=verbose` to include all.
pub fn get_min_debug_log_level() -> DebugLogLevel {
    static LEVEL: Lazy<DebugLogLevel> = Lazy::new(|| {
        std::env::var("MOSSEN_CODE_DEBUG_LOG_LEVEL")
            .ok()
            .and_then(|v| DebugLogLevel::from_str_loose(&v))
            .unwrap_or(DebugLogLevel::Debug)
    });
    *LEVEL
}

/// Check if debug mode is currently enabled.
///
/// Debug mode is active if any of:
/// - Runtime debug was enabled via `enable_debug_logging()`
/// - `DEBUG` env var is truthy
/// - `DEBUG_SDK` env var is truthy
/// - `--debug` or `-d` was passed on command line
/// - `--debug-to-stderr` or `-d2e` was passed
/// - `--debug=<pattern>` was passed
/// - `--debug-file` was specified
pub fn is_debug_mode() -> bool {
    static IS_DEBUG: Lazy<AtomicBool> = Lazy::new(|| {
        let enabled = is_env_truthy("DEBUG")
            || is_env_truthy("DEBUG_SDK")
            || std::env::args().any(|a| a == "--debug" || a == "-d")
            || is_debug_to_stderr()
            || std::env::args().any(|a| a.starts_with("--debug="))
            || get_debug_file_path().is_some();
        AtomicBool::new(enabled)
    });

    RUNTIME_DEBUG_ENABLED.load(Ordering::Relaxed) || IS_DEBUG.load(Ordering::Relaxed)
}

/// Enable debug logging mid-session (e.g., via /debug command).
/// Returns true if logging was already active.
pub fn enable_debug_logging() -> bool {
    let was_active = is_debug_mode()
        || std::env::var("USER_TYPE").ok().as_deref() == Some("ant");
    RUNTIME_DEBUG_ENABLED.store(true, Ordering::Relaxed);
    was_active
}

/// Get the debug filter from command line arguments.
pub fn get_debug_filter() -> Option<DebugFilter> {
    static FILTER: Lazy<Option<DebugFilter>> = Lazy::new(|| {
        for arg in std::env::args() {
            if let Some(pattern) = arg.strip_prefix("--debug=") {
                return Some(parse_debug_filter(pattern));
            }
        }
        None
    });
    FILTER.clone()
}

/// Check if debug output should go to stderr.
pub fn is_debug_to_stderr() -> bool {
    static IS_STDERR: Lazy<bool> = Lazy::new(|| {
        std::env::args().any(|a| a == "--debug-to-stderr" || a == "-d2e")
    });
    *IS_STDERR
}

/// Get the debug file path from command line arguments.
pub fn get_debug_file_path() -> Option<String> {
    static PATH: Lazy<Option<String>> = Lazy::new(|| {
        let args: Vec<String> = std::env::args().collect();
        for (i, arg) in args.iter().enumerate() {
            if let Some(path) = arg.strip_prefix("--debug-file=") {
                return Some(path.to_string());
            }
            if arg == "--debug-file" && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
        None
    });
    PATH.clone()
}

/// Set whether formatted output mode is active.
pub fn set_has_formatted_output(value: bool) {
    HAS_FORMATTED_OUTPUT.store(value, Ordering::Relaxed);
}

/// Get whether formatted output mode is active.
pub fn get_has_formatted_output() -> bool {
    HAS_FORMATTED_OUTPUT.load(Ordering::Relaxed)
}

/// Check whether a debug message should be logged.
fn should_log_debug_message(message: &str) -> bool {
    // In test mode without stderr output, skip
    if std::env::var("NODE_ENV").ok().as_deref() == Some("test") && !is_debug_to_stderr() {
        return false;
    }

    // Non-ants only write debug logs when debug mode is active
    if std::env::var("USER_TYPE").ok().as_deref() != Some("ant") && !is_debug_mode() {
        return false;
    }

    let filter = get_debug_filter();
    should_show_debug_message(message, filter.as_ref())
}

/// A buffered debug log writer.
pub struct DebugWriter {
    buffer: Arc<Mutex<Vec<String>>>,
    immediate_mode: bool,
}

impl DebugWriter {
    /// Create a new debug writer.
    pub fn new(immediate_mode: bool) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            immediate_mode,
        }
    }

    /// Write a message to the debug log.
    pub fn write(&self, content: &str, path: &Path) {
        if self.immediate_mode {
            // Sync write for immediate mode
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = file.write_all(content.as_bytes());
            }
        } else {
            let mut buffer = self.buffer.lock();
            buffer.push(content.to_string());
        }
    }

    /// Flush any buffered content to disk.
    pub fn flush(&self, path: &Path) {
        let mut buffer = self.buffer.lock();
        if buffer.is_empty() {
            return;
        }
        let content: String = buffer.drain(..).collect();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = file.write_all(content.as_bytes());
        }
    }

    /// Dispose (flush remaining content).
    pub fn dispose(&self, path: &Path) {
        self.flush(path);
    }
}

/// Global debug writer instance.
static DEBUG_WRITER: Lazy<Mutex<Option<DebugWriter>>> = Lazy::new(|| Mutex::new(None));

fn get_debug_writer() -> &'static Lazy<Mutex<Option<DebugWriter>>> {
    // Initialize on first use
    {
        let mut writer = DEBUG_WRITER.lock();
        if writer.is_none() {
            *writer = Some(DebugWriter::new(is_debug_mode()));
        }
    }
    &DEBUG_WRITER
}

/// Flush all pending debug log writes.
pub fn flush_debug_logs(path: &Path) {
    let writer_guard = get_debug_writer().lock();
    if let Some(writer) = writer_guard.as_ref() {
        writer.flush(path);
    }
}

/// Get the debug log path based on configuration.
///
/// Priority:
/// 1. `--debug-file` argument
/// 2. `MOSSEN_CODE_DEBUG_LOGS_DIR` environment variable
/// 3. Default: `~/.mossen/debug/<session_id>.txt`
pub fn get_debug_log_path(session_id: &str, config_home: &Path) -> PathBuf {
    if let Some(path) = get_debug_file_path() {
        return PathBuf::from(path);
    }
    if let Ok(dir) = std::env::var("MOSSEN_CODE_DEBUG_LOGS_DIR") {
        return PathBuf::from(dir);
    }
    config_home
        .join("debug")
        .join(format!("{}.txt", session_id))
}

/// Log a message for debugging purposes.
///
/// Respects the minimum log level, debug mode state, and debug filter.
/// Formats messages with ISO timestamp and level prefix.
pub fn log_for_debugging(message: &str, level: DebugLogLevel, session_id: &str, config_home: &Path) {
    if level < get_min_debug_log_level() {
        return;
    }
    if !should_log_debug_message(message) {
        return;
    }

    let mut output_message = message.to_string();
    // Multiline messages break jsonl format, so serialize them
    if get_has_formatted_output() && message.contains('\n') {
        output_message = serde_json::to_string(&message).unwrap_or_else(|_| message.to_string());
    }

    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let output = format!(
        "{} [{}] {}\n",
        timestamp,
        level.as_str_upper(),
        output_message.trim()
    );

    if is_debug_to_stderr() {
        eprint!("{}", output);
        return;
    }

    let path = get_debug_log_path(session_id, config_home);
    let writer_guard = get_debug_writer().lock();
    if let Some(writer) = writer_guard.as_ref() {
        writer.write(&output, &path);
    }
}

/// Update the latest debug log symlink to point to the current debug log file.
///
/// Creates or updates a symlink at `<debug_dir>/latest`.
pub fn update_latest_debug_log_symlink(debug_log_path: &Path) {
    if let Some(debug_dir) = debug_log_path.parent() {
        let latest_symlink = debug_dir.join("latest");
        // Remove existing symlink
        let _ = std::fs::remove_file(&latest_symlink);
        // Create new symlink
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(debug_log_path, &latest_symlink);
        }
        #[cfg(windows)]
        {
            let _ = std::os::windows::fs::symlink_file(debug_log_path, &latest_symlink);
        }
    }
}

/// Log errors for Ants only, always visible in production.
pub fn log_ant_error(context: &str, error: &dyn std::error::Error, session_id: &str, config_home: &Path) {
    if std::env::var("USER_TYPE").ok().as_deref() != Some("ant") {
        return;
    }

    let stack_info = format!("[Internal] {} stack trace:\n{:?}", context, error);
    log_for_debugging(&stack_info, DebugLogLevel::Error, session_id, config_home);
}

/// Helper: check if an environment variable value is truthy.
fn is_env_truthy(key: &str) -> bool {
    match std::env::var(key) {
        Ok(val) => {
            let v = val.trim().to_lowercase();
            !v.is_empty() && v != "0" && v != "false" && v != "no"
        }
        Err(_) => false,
    }
}
