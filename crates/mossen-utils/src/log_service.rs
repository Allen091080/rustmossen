//! Log Service
//!
//! Error logging with in-memory buffer, MCP logging, and error log file management.

use chrono::Utc;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

const MAX_IN_MEMORY_ERRORS: usize = 100;

/// An error log entry.
#[derive(Debug, Clone)]
pub struct ErrorLogEntry {
    pub error: String,
    pub timestamp: String,
}

/// In-memory error log.
static IN_MEMORY_ERROR_LOG: Lazy<Mutex<Vec<ErrorLogEntry>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Error log sink trait.
pub trait ErrorLogSink: Send + Sync {
    fn log_error(&self, error: &anyhow::Error);
    fn log_mcp_error(&self, server_name: &str, error: &str);
    fn log_mcp_debug(&self, server_name: &str, message: &str);
    fn get_errors_path(&self) -> String;
    fn get_mcp_logs_path(&self, server_name: &str) -> String;
}

/// Queued error event types.
#[derive(Debug, Clone)]
enum QueuedErrorEvent {
    Error(String),
    McpError {
        server_name: String,
        error: String,
    },
    McpDebug {
        server_name: String,
        message: String,
    },
}

static ERROR_QUEUE: Lazy<Mutex<Vec<QueuedErrorEvent>>> = Lazy::new(|| Mutex::new(Vec::new()));
static ERROR_LOG_SINK: Lazy<Mutex<Option<Box<dyn ErrorLogSink>>>> = Lazy::new(|| Mutex::new(None));

fn add_to_in_memory_error_log(entry: ErrorLogEntry) {
    let mut log = IN_MEMORY_ERROR_LOG.lock();
    if log.len() >= MAX_IN_MEMORY_ERRORS {
        log.remove(0);
    }
    log.push(entry);
}

/// Attach the error log sink.
pub fn attach_error_log_sink(sink: Box<dyn ErrorLogSink>) {
    let mut sink_holder = ERROR_LOG_SINK.lock();
    if sink_holder.is_some() {
        return;
    }
    *sink_holder = Some(sink);

    // Drain the queue
    let mut queue = ERROR_QUEUE.lock();
    let queued_events: Vec<QueuedErrorEvent> = queue.drain(..).collect();
    drop(queue);

    let sink_ref = sink_holder.as_ref().unwrap();
    for event in queued_events {
        match event {
            QueuedErrorEvent::Error(msg) => {
                sink_ref.log_error(&anyhow::anyhow!("{}", msg));
            }
            QueuedErrorEvent::McpError { server_name, error } => {
                sink_ref.log_mcp_error(&server_name, &error);
            }
            QueuedErrorEvent::McpDebug {
                server_name,
                message,
            } => {
                sink_ref.log_mcp_debug(&server_name, &message);
            }
        }
    }
}

/// Log an error to multiple destinations.
pub fn log_error(error: &dyn std::fmt::Display) {
    let error_str = error.to_string();
    let entry = ErrorLogEntry {
        error: error_str.clone(),
        timestamp: Utc::now().to_rfc3339(),
    };
    add_to_in_memory_error_log(entry);

    let sink = ERROR_LOG_SINK.lock();
    if let Some(ref s) = *sink {
        s.log_error(&anyhow::anyhow!("{}", error_str));
    } else {
        ERROR_QUEUE.lock().push(QueuedErrorEvent::Error(error_str));
    }
}

/// Get in-memory errors.
pub fn get_in_memory_errors() -> Vec<ErrorLogEntry> {
    IN_MEMORY_ERROR_LOG.lock().clone()
}

/// Log an MCP error.
pub fn log_mcp_error(server_name: &str, error: &str) {
    let sink = ERROR_LOG_SINK.lock();
    if let Some(ref s) = *sink {
        s.log_mcp_error(server_name, error);
    } else {
        ERROR_QUEUE.lock().push(QueuedErrorEvent::McpError {
            server_name: server_name.to_string(),
            error: error.to_string(),
        });
    }
}

/// Log an MCP debug message.
pub fn log_mcp_debug(server_name: &str, message: &str) {
    let sink = ERROR_LOG_SINK.lock();
    if let Some(ref s) = *sink {
        s.log_mcp_debug(server_name, message);
    } else {
        ERROR_QUEUE.lock().push(QueuedErrorEvent::McpDebug {
            server_name: server_name.to_string(),
            message: message.to_string(),
        });
    }
}

/// Gets the display title for a log/session with fallback logic.
pub fn get_log_display_title(
    agent_name: Option<&str>,
    custom_title: Option<&str>,
    summary: Option<&str>,
    first_prompt: Option<&str>,
    session_id: Option<&str>,
    default_title: Option<&str>,
) -> String {
    let is_autonomous_prompt = first_prompt
        .map(|p| p.starts_with("<tick>"))
        .unwrap_or(false);

    let title = agent_name
        .filter(|s| !s.is_empty())
        .or(custom_title.filter(|s| !s.is_empty()))
        .or(summary.filter(|s| !s.is_empty()))
        .or_else(|| {
            if !is_autonomous_prompt {
                first_prompt.filter(|s| !s.is_empty())
            } else {
                None
            }
        })
        .or(default_title)
        .or_else(|| {
            if is_autonomous_prompt {
                Some("Autonomous session")
            } else {
                None
            }
        })
        .or_else(|| session_id.map(|id| &id[..8.min(id.len())]))
        .unwrap_or("");

    title.trim().to_string()
}

/// Convert a date to filename-safe format.
pub fn date_to_filename(date: &chrono::DateTime<Utc>) -> String {
    date.format("%Y-%m-%dT%H-%M-%S-%3fZ").to_string()
}

/// Parse an ISO string to a DateTime.
pub fn parse_iso_string(s: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            // Fallback: parse manually
            let parts: Vec<&str> = s.split(|c: char| !c.is_ascii_digit()).collect();
            if parts.len() >= 7 {
                let year: i32 = parts[0].parse().ok()?;
                let month: u32 = parts[1].parse().ok()?;
                let day: u32 = parts[2].parse().ok()?;
                let hour: u32 = parts[3].parse().ok()?;
                let min: u32 = parts[4].parse().ok()?;
                let sec: u32 = parts[5].parse().ok()?;
                let ms: u32 = parts[6].parse().ok()?;

                use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
                let date = NaiveDate::from_ymd_opt(year, month, day)?;
                let time = NaiveTime::from_hms_milli_opt(hour, min, sec, ms)?;
                let dt = NaiveDateTime::new(date, time);
                Some(chrono::DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
            } else {
                None
            }
        })
}

/// Reset error log state for testing.
pub fn reset_error_log_for_testing() {
    *ERROR_LOG_SINK.lock() = None;
    ERROR_QUEUE.lock().clear();
    IN_MEMORY_ERROR_LOG.lock().clear();
}
