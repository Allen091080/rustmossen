//! Error logging and log management utilities.
//!
//! Provides structured error logging with in-memory buffering, queued events
//! before sink attachment, and file-based log loading/sorting.

use chrono::{DateTime, NaiveDateTime, Utc};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

use crate::string_utils::truncate_chars;

/// Maximum number of errors kept in memory.
const MAX_IN_MEMORY_ERRORS: usize = 100;

/// An error entry with timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub error: String,
    pub timestamp: String,
}

/// A log option representing a session/error log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogOption {
    pub date: String,
    pub full_path: PathBuf,
    pub messages: Vec<SerializedMessage>,
    pub value: usize,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub first_prompt: String,
    pub message_count: usize,
    pub is_sidechain: bool,
    #[serde(default)]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub custom_title: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// A serialized message from a log file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedMessage {
    #[serde(rename = "type")]
    pub msg_type: Option<String>,
    pub message: Option<MessageContent>,
    pub timestamp: Option<String>,
}

/// Message content (simplified).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContent {
    pub content: Option<serde_json::Value>,
}

/// Tick tag constant for autonomous mode detection.
const TICK_TAG: &str = "tick";

/// Queued error event types.
#[derive(Debug, Clone)]
enum QueuedErrorEvent {
    Error {
        error: String,
        stack: Option<String>,
    },
    McpError {
        server_name: String,
        error: String,
    },
    McpDebug {
        server_name: String,
        message: String,
    },
}

/// Error log sink trait.
pub trait ErrorLogSink: Send + Sync {
    fn log_error(&self, error: &str, stack: Option<&str>);
    fn log_mcp_error(&self, server_name: &str, error: &str);
    fn log_mcp_debug(&self, server_name: &str, message: &str);
    fn get_errors_path(&self) -> PathBuf;
    fn get_mcp_logs_path(&self, server_name: &str) -> PathBuf;
}

/// Global error log state.
struct ErrorLogState {
    in_memory_errors: Vec<ErrorInfo>,
    error_queue: Vec<QueuedErrorEvent>,
    sink: Option<Arc<dyn ErrorLogSink>>,
}

static ERROR_LOG_STATE: Lazy<Mutex<ErrorLogState>> = Lazy::new(|| {
    Mutex::new(ErrorLogState {
        in_memory_errors: Vec::new(),
        error_queue: Vec::new(),
        sink: None,
    })
});

static HARD_FAIL_MODE: Lazy<bool> = Lazy::new(|| std::env::args().any(|a| a == "--hard-fail"));

/// Gets the display title for a log/session with fallback logic.
pub fn get_log_display_title(log: &LogOption, default_title: Option<&str>) -> String {
    let is_autonomous_prompt = log.first_prompt.starts_with(&format!("<{}>", TICK_TAG));

    let stripped_first_prompt = strip_display_tags_allow_empty(&log.first_prompt);
    let use_first_prompt = !stripped_first_prompt.is_empty() && !is_autonomous_prompt;

    let title = log
        .agent_name
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(log.custom_title.as_deref().filter(|s| !s.is_empty()))
        .or(log.summary.as_deref().filter(|s| !s.is_empty()))
        .or(if use_first_prompt {
            Some(stripped_first_prompt.as_str())
        } else {
            None
        })
        .or(default_title)
        .or(if is_autonomous_prompt {
            Some("Autonomous session")
        } else {
            None
        })
        .or(log.session_id.as_deref().map(|s| &s[..s.len().min(8)]))
        .unwrap_or("");

    strip_display_tags(title).trim().to_string()
}

/// Strip display-unfriendly tags from text.
fn strip_display_tags(text: &str) -> String {
    let re = regex::Regex::new(r"</?(?:command-name|ide_opened_file|context)[^>]*>").unwrap();
    let result = re.replace_all(text, "");
    if result.trim().is_empty() {
        text.to_string()
    } else {
        result.to_string()
    }
}

/// Strip display tags, allowing empty result.
fn strip_display_tags_allow_empty(text: &str) -> String {
    let re = regex::Regex::new(r"</?(?:command-name|ide_opened_file|context)[^>]*>").unwrap();
    re.replace_all(text, "").to_string()
}

/// Convert a date to a filename-safe string.
pub fn date_to_filename(date: &DateTime<Utc>) -> String {
    date.format("%Y-%m-%dT%H-%M-%S%.3fZ").to_string()
}

/// Attach the error log sink.
pub fn attach_error_log_sink(new_sink: Arc<dyn ErrorLogSink>) {
    let mut state = ERROR_LOG_STATE.lock();
    if state.sink.is_some() {
        return;
    }
    state.sink = Some(new_sink.clone());

    // Drain the queue
    let queued: Vec<_> = state.error_queue.drain(..).collect();
    drop(state);

    for event in queued {
        match event {
            QueuedErrorEvent::Error { error, stack } => {
                new_sink.log_error(&error, stack.as_deref());
            }
            QueuedErrorEvent::McpError { server_name, error } => {
                new_sink.log_mcp_error(&server_name, &error);
            }
            QueuedErrorEvent::McpDebug {
                server_name,
                message,
            } => {
                new_sink.log_mcp_debug(&server_name, &message);
            }
        }
    }
}

/// Log an error.
pub fn log_error(error: &dyn std::error::Error) {
    if *HARD_FAIL_MODE {
        eprintln!("[HARD FAIL] logError called with: {}", error);
        std::process::exit(1);
    }

    // Check if error reporting should be disabled
    if is_error_reporting_disabled() {
        return;
    }

    let error_str = format!("{:?}", error);
    let error_info = ErrorInfo {
        error: error_str.clone(),
        timestamp: Utc::now().to_rfc3339(),
    };

    let mut state = ERROR_LOG_STATE.lock();
    // Add to in-memory log
    if state.in_memory_errors.len() >= MAX_IN_MEMORY_ERRORS {
        state.in_memory_errors.remove(0);
    }
    state.in_memory_errors.push(error_info);

    if let Some(ref sink) = state.sink {
        let sink = sink.clone();
        drop(state);
        sink.log_error(&error_str, None);
    } else {
        state.error_queue.push(QueuedErrorEvent::Error {
            error: error_str,
            stack: None,
        });
    }
}

/// Log an error from a string message.
pub fn log_error_str(message: &str) {
    if *HARD_FAIL_MODE {
        eprintln!("[HARD FAIL] logError called with: {}", message);
        std::process::exit(1);
    }

    if is_error_reporting_disabled() {
        return;
    }

    let error_info = ErrorInfo {
        error: message.to_string(),
        timestamp: Utc::now().to_rfc3339(),
    };

    let mut state = ERROR_LOG_STATE.lock();
    if state.in_memory_errors.len() >= MAX_IN_MEMORY_ERRORS {
        state.in_memory_errors.remove(0);
    }
    state.in_memory_errors.push(error_info);

    if let Some(ref sink) = state.sink {
        let sink = sink.clone();
        drop(state);
        sink.log_error(message, None);
    } else {
        state.error_queue.push(QueuedErrorEvent::Error {
            error: message.to_string(),
            stack: None,
        });
    }
}

/// Check if error reporting is disabled.
fn is_error_reporting_disabled() -> bool {
    std::env::var("MOSSEN_CODE_USE_BEDROCK").is_ok_and(|v| is_env_truthy(&v))
        || std::env::var("MOSSEN_CODE_USE_VERTEX").is_ok_and(|v| is_env_truthy(&v))
        || std::env::var("MOSSEN_CODE_USE_FOUNDRY").is_ok_and(|v| is_env_truthy(&v))
        || std::env::var("DISABLE_ERROR_REPORTING").is_ok()
}

/// Check if an env var value is truthy.
fn is_env_truthy(val: &str) -> bool {
    matches!(val.to_lowercase().as_str(), "1" | "true" | "yes")
}

/// Get in-memory errors.
pub fn get_in_memory_errors() -> Vec<ErrorInfo> {
    ERROR_LOG_STATE.lock().in_memory_errors.clone()
}

/// Load error logs from the errors directory.
pub async fn load_error_logs(errors_path: &Path) -> Vec<LogOption> {
    load_log_list(errors_path).await
}

/// Get an error log by index.
pub async fn get_error_log_by_index(errors_path: &Path, index: usize) -> Option<LogOption> {
    let logs = load_error_logs(errors_path).await;
    logs.into_iter().nth(index)
}

/// Load and process logs from a directory.
async fn load_log_list(path: &Path) -> Vec<LogOption> {
    let entries = match fs::read_dir(path).await {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut files = Vec::new();
    let mut dir = entries;
    while let Ok(Some(entry)) = dir.next_entry().await {
        files.push(entry);
    }

    let mut log_data: Vec<LogOption> = Vec::new();

    for (i, file) in files.iter().enumerate() {
        let full_path = file.path();
        let content = match fs::read_to_string(&full_path).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        let messages: Vec<SerializedMessage> = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let first_message = messages.first();
        let last_message = messages.last();

        let first_prompt = first_message
            .and_then(|m| {
                if m.msg_type.as_deref() == Some("user") {
                    m.message.as_ref().and_then(|msg| {
                        msg.content
                            .as_ref()
                            .and_then(|c| c.as_str().map(|s| s.to_string()))
                    })
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "No prompt".to_string());

        let file_stats = match fs::metadata(&full_path).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let is_sidechain = full_path.to_string_lossy().contains("sidechain");
        let mtime: DateTime<Utc> = file_stats
            .modified()
            .map(DateTime::from)
            .unwrap_or_else(|_| Utc::now());
        let date = date_to_filename(&mtime);

        let created = first_message
            .and_then(|m| m.timestamp.as_deref())
            .and_then(parse_iso_string)
            .unwrap_or(mtime);

        let modified = last_message
            .and_then(|m| m.timestamp.as_deref())
            .and_then(parse_iso_string)
            .unwrap_or(mtime);

        let truncated_prompt = {
            let first_line = first_prompt.lines().next().unwrap_or(&first_prompt);
            truncate_chars(first_line, 50)
        };

        log_data.push(LogOption {
            date,
            full_path,
            message_count: messages.len(),
            messages,
            value: i,
            created,
            modified,
            first_prompt: truncated_prompt,
            is_sidechain,
            agent_name: None,
            custom_title: None,
            summary: None,
            session_id: None,
        });
    }

    // Sort by modified date descending
    log_data.sort_by(|a, b| b.modified.cmp(&a.modified));

    // Re-assign value indices
    for (i, log) in log_data.iter_mut().enumerate() {
        log.value = i;
    }

    log_data
}

/// Parse an ISO date string into a DateTime.
fn parse_iso_string(s: &str) -> Option<DateTime<Utc>> {
    // Split on non-digit characters
    let parts: Vec<&str> = s.split(|c: char| !c.is_ascii_digit()).collect();
    if parts.len() < 7 {
        return None;
    }
    let year = parts[0].parse::<i32>().ok()?;
    let month = parts[1].parse::<u32>().ok()?;
    let day = parts[2].parse::<u32>().ok()?;
    let hour = parts[3].parse::<u32>().ok()?;
    let min = parts[4].parse::<u32>().ok()?;
    let sec = parts[5].parse::<u32>().ok()?;
    let milli = parts[6].parse::<u32>().ok()?;

    NaiveDateTime::new(
        chrono::NaiveDate::from_ymd_opt(year, month, day)?,
        chrono::NaiveTime::from_hms_milli_opt(hour, min, sec, milli)?,
    )
    .and_local_timezone(Utc)
    .single()
}

/// Log an MCP error.
pub fn log_mcp_error(server_name: &str, error: &str) {
    let mut state = ERROR_LOG_STATE.lock();
    if let Some(ref sink) = state.sink {
        let sink = sink.clone();
        drop(state);
        sink.log_mcp_error(server_name, error);
    } else {
        state.error_queue.push(QueuedErrorEvent::McpError {
            server_name: server_name.to_string(),
            error: error.to_string(),
        });
    }
}

/// Log an MCP debug message.
pub fn log_mcp_debug(server_name: &str, message: &str) {
    let mut state = ERROR_LOG_STATE.lock();
    if let Some(ref sink) = state.sink {
        let sink = sink.clone();
        drop(state);
        sink.log_mcp_debug(server_name, message);
    } else {
        state.error_queue.push(QueuedErrorEvent::McpDebug {
            server_name: server_name.to_string(),
            message: message.to_string(),
        });
    }
}

/// Capture API request for bug reports (stores params without messages).
pub fn capture_api_request(params: &serde_json::Value, query_source: Option<&str>) {
    if let Some(source) = query_source {
        if !source.starts_with("repl_main_thread") {
            return;
        }
    } else {
        return;
    }

    // Store params without messages
    if let Some(obj) = params.as_object() {
        let mut without_messages = obj.clone();
        without_messages.remove("messages");
        // In a real implementation, this would call setLastAPIRequest
        let _ = without_messages;
    }
}

/// Reset error log state for testing.
pub fn reset_error_log_for_testing() {
    let mut state = ERROR_LOG_STATE.lock();
    state.sink = None;
    state.error_queue.clear();
    state.in_memory_errors.clear();
}
