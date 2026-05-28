use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use chrono::Utc;
use once_cell::sync::Lazy;
use tokio::fs;

use crate::debug_filter::{parse_debug_filter, should_show_debug_message, DebugFilter};

/// Debug log levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DebugLogLevel {
    Verbose = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl DebugLogLevel {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "verbose" => Some(Self::Verbose),
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verbose => "VERBOSE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

static RUNTIME_DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);
static HAS_FORMATTED_OUTPUT: AtomicBool = AtomicBool::new(false);

static DEBUG_FILTER: Lazy<Option<DebugFilter>> = Lazy::new(|| {
    // Look for --debug=pattern in args
    let args: Vec<String> = std::env::args().collect();
    let debug_arg = args.iter().find(|a| a.starts_with("--debug="));
    debug_arg.and_then(|arg| {
        let pattern = &arg["--debug=".len()..];
        parse_debug_filter(Some(pattern))
    })
});

static MIN_DEBUG_LOG_LEVEL: Lazy<DebugLogLevel> = Lazy::new(|| {
    std::env::var("MOSSEN_CODE_DEBUG_LOG_LEVEL")
        .ok()
        .and_then(|v| DebugLogLevel::from_str_opt(&v))
        .unwrap_or(DebugLogLevel::Debug)
});

/// Returns the minimum debug log level.
pub fn get_min_debug_log_level() -> DebugLogLevel {
    *MIN_DEBUG_LOG_LEVEL
}

/// Check if debug mode is enabled.
pub fn is_debug_mode() -> bool {
    if RUNTIME_DEBUG_ENABLED.load(Ordering::Relaxed) {
        return true;
    }
    if is_env_truthy("DEBUG") || is_env_truthy("DEBUG_SDK") {
        return true;
    }
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--debug" || a == "-d") {
        return true;
    }
    if is_debug_to_stderr() {
        return true;
    }
    if args.iter().any(|a| a.starts_with("--debug=")) {
        return true;
    }
    if get_debug_file_path().is_some() {
        return true;
    }
    false
}

/// Enables debug logging mid-session.
pub fn enable_debug_logging() -> bool {
    let was_active = is_debug_mode()
        || std::env::var("USER_TYPE")
            .map(|v| v == "internal")
            .unwrap_or(false);
    RUNTIME_DEBUG_ENABLED.store(true, Ordering::Relaxed);
    was_active
}

/// Check if debug output should go to stderr.
pub fn is_debug_to_stderr() -> bool {
    let args: Vec<String> = std::env::args().collect();
    args.iter().any(|a| a == "--debug-to-stderr" || a == "-d2e")
}

/// Get the debug file path from command line arguments.
pub fn get_debug_file_path() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    for (i, arg) in args.iter().enumerate() {
        if let Some(path) = arg.strip_prefix("--debug-file=") {
            return Some(path.to_string());
        }
        if arg == "--debug-file" {
            if let Some(next) = args.get(i + 1) {
                return Some(next.clone());
            }
        }
    }
    None
}

fn should_log_debug_message(message: &str) -> bool {
    if std::env::var("NODE_ENV")
        .map(|v| v == "test")
        .unwrap_or(false)
        && !is_debug_to_stderr()
    {
        return false;
    }

    let is_internal = std::env::var("USER_TYPE")
        .map(|v| v == "internal")
        .unwrap_or(false);
    if !is_internal && !is_debug_mode() {
        return false;
    }

    should_show_debug_message(message, DEBUG_FILTER.as_ref())
}

pub fn set_has_formatted_output(value: bool) {
    HAS_FORMATTED_OUTPUT.store(value, Ordering::Relaxed);
}

pub fn get_has_formatted_output() -> bool {
    HAS_FORMATTED_OUTPUT.load(Ordering::Relaxed)
}

static DEBUG_LOG_BUFFER: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Log a debug message.
pub fn log_for_debugging(message: &str, level: DebugLogLevel) {
    if (level as u8) < (get_min_debug_log_level() as u8) {
        return;
    }
    if !should_log_debug_message(message) {
        return;
    }

    let mut msg = message.to_string();
    if get_has_formatted_output() && msg.contains('\n') {
        msg = serde_json::to_string(&msg).unwrap_or(msg);
    }

    let timestamp = Utc::now().to_rfc3339();
    let output = format!("{} [{}] {}\n", timestamp, level.as_str(), msg.trim());

    if is_debug_to_stderr() {
        eprint!("{}", output);
        return;
    }

    // Buffer the output
    if let Ok(mut buffer) = DEBUG_LOG_BUFFER.lock() {
        buffer.push(output);
    }
}

/// Flush debug logs to disk.
pub async fn flush_debug_logs() {
    let entries: Vec<String> = {
        let mut buffer = DEBUG_LOG_BUFFER.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *buffer)
    };

    if entries.is_empty() {
        return;
    }

    let path = get_debug_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }

    let content = entries.join("");
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .map(|_| ());

    // Use tokio append
    if let Ok(mut file) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
    {
        use tokio::io::AsyncWriteExt;
        let _ = file.write_all(content.as_bytes()).await;
    }

    // Update the latest symlink
    let _ = update_latest_debug_log_symlink(&path).await;
}

/// Get the debug log path.
pub fn get_debug_log_path() -> PathBuf {
    if let Some(path) = get_debug_file_path() {
        return PathBuf::from(path);
    }
    if let Ok(dir) = std::env::var("MOSSEN_CODE_DEBUG_LOGS_DIR") {
        return PathBuf::from(dir);
    }
    let config_dir = get_mossen_config_home_dir();
    let session_id = get_session_id_for_debug();
    config_dir.join("debug").join(format!("{}.txt", session_id))
}

/// Updates the latest debug log symlink.
async fn update_latest_debug_log_symlink(debug_log_path: &std::path::Path) -> std::io::Result<()> {
    if let Some(debug_logs_dir) = debug_log_path.parent() {
        let latest_symlink = debug_logs_dir.join("latest");
        let _ = fs::remove_file(&latest_symlink).await;
        #[cfg(unix)]
        {
            let _ = fs::symlink(debug_log_path, &latest_symlink).await;
        }
    }
    Ok(())
}

/// Log errors for Ants only.
pub fn log_ant_error(context: &str, error: &dyn std::error::Error) {
    if std::env::var("USER_TYPE")
        .map(|v| v == "internal")
        .unwrap_or(false)
    {
        return;
    }

    log_for_debugging(
        &format!("[Internal] {} error: {}", context, error),
        DebugLogLevel::Error,
    );
}

// Helper utilities
fn is_env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|v| {
            let lower = v.to_lowercase();
            lower == "1" || lower == "true" || lower == "yes"
        })
        .unwrap_or(false)
}

fn get_mossen_config_home_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MOSSEN_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".mossen")
}

fn get_session_id_for_debug() -> String {
    std::env::var("MOSSEN_SESSION_ID").unwrap_or_else(|_| "unknown".to_string())
}
