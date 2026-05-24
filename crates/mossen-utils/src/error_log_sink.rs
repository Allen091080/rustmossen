//! Error log sink implementation.
//!
//! Handles file-based error logging to disk. Should be initialized during app startup.
//! Separate from log.rs to avoid import cycles.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use once_cell::sync::Lazy;

/// Date string formatted for filenames (YYYY-MM-DD).
fn date_to_filename(dt: &chrono::DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d").to_string()
}

static DATE: Lazy<String> = Lazy::new(|| date_to_filename(&Utc::now()));

/// Gets the path to the errors log file.
pub fn get_errors_path(errors_dir: &Path) -> PathBuf {
    errors_dir.join(format!("{}.jsonl", *DATE))
}

/// Gets the path to MCP logs for a server.
pub fn get_mcp_logs_path(mcp_logs_dir: &Path, server_name: &str) -> PathBuf {
    mcp_logs_dir
        .join(server_name)
        .join(format!("{}.jsonl", *DATE))
}

/// A buffered JSONL writer.
struct JsonlWriter {
    buffer: Vec<String>,
    max_buffer_size: usize,
    path: PathBuf,
}

impl JsonlWriter {
    fn new(path: PathBuf, max_buffer_size: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_buffer_size,
            path,
        }
    }

    fn write(&mut self, obj: &serde_json::Value) {
        let line = serde_json::to_string(obj).unwrap_or_default();
        self.buffer.push(line);
        if self.buffer.len() >= self.max_buffer_size {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let content: String = self.buffer.drain(..).map(|l| l + "\n").collect();
        let dir = self.path.parent();
        // Attempt to write; create directory if needed
        if let Err(_) = Self::append_to_file(&self.path, &content) {
            if let Some(d) = dir {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = Self::append_to_file(&self.path, &content);
        }
    }

    fn append_to_file(path: &Path, content: &str) -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(content.as_bytes())
    }

    fn dispose(&mut self) {
        self.flush();
    }
}

impl Drop for JsonlWriter {
    fn drop(&mut self) {
        self.dispose();
    }
}

static LOG_WRITERS: Lazy<Mutex<HashMap<PathBuf, JsonlWriter>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Flush all buffered log writers. Used for testing.
pub fn flush_log_writers_for_testing() {
    if let Ok(mut writers) = LOG_WRITERS.lock() {
        for writer in writers.values_mut() {
            writer.flush();
        }
    }
}

/// Clear all buffered log writers. Used for testing.
pub fn clear_log_writers_for_testing() {
    if let Ok(mut writers) = LOG_WRITERS.lock() {
        for writer in writers.values_mut() {
            writer.dispose();
        }
        writers.clear();
    }
}

fn get_log_writer(path: PathBuf) -> () {
    let mut writers = LOG_WRITERS.lock().unwrap();
    if !writers.contains_key(&path) {
        let writer = JsonlWriter::new(path.clone(), 50);
        writers.insert(path, writer);
    }
}

fn write_to_log(path: &Path, message: &serde_json::Value) {
    let mut writers = LOG_WRITERS.lock().unwrap();
    if !writers.contains_key(path) {
        let writer = JsonlWriter::new(path.to_path_buf(), 50);
        writers.insert(path.to_path_buf(), writer);
    }
    if let Some(writer) = writers.get_mut(path) {
        writer.write(message);
    }
}

/// Appends a log entry to the given path, enriched with timestamp and metadata.
fn append_to_log(path: &Path, message: serde_json::Value, session_id: &str, version: &str) {
    let user_type = std::env::var("USER_TYPE").unwrap_or_default();
    if user_type != "internal" {
        return;
    }

    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let enriched = match message {
        serde_json::Value::Object(mut map) => {
            map.insert(
                "timestamp".to_string(),
                serde_json::Value::String(Utc::now().to_rfc3339()),
            );
            map.insert("cwd".to_string(), serde_json::Value::String(cwd));
            map.insert("userType".to_string(), serde_json::Value::String(user_type));
            map.insert(
                "sessionId".to_string(),
                serde_json::Value::String(session_id.to_string()),
            );
            map.insert(
                "version".to_string(),
                serde_json::Value::String(version.to_string()),
            );
            serde_json::Value::Object(map)
        }
        other => other,
    };

    write_to_log(path, &enriched);
}

/// Extracts a server message string from unknown response data.
pub fn extract_server_message(data: &serde_json::Value) -> Option<String> {
    match data {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(obj) => {
            if let Some(serde_json::Value::String(msg)) = obj.get("message") {
                return Some(msg.clone());
            }
            if let Some(serde_json::Value::Object(err)) = obj.get("error") {
                if let Some(serde_json::Value::String(msg)) = err.get("message") {
                    return Some(msg.clone());
                }
            }
            None
        }
        _ => None,
    }
}

/// Configuration for the error log sink.
pub struct ErrorLogSinkConfig {
    pub errors_dir: PathBuf,
    pub mcp_logs_base_dir: PathBuf,
    pub session_id: String,
    pub version: String,
}

/// Error log sink that writes errors and MCP logs to disk.
pub struct ErrorLogSink {
    config: ErrorLogSinkConfig,
}

impl ErrorLogSink {
    /// Create a new error log sink with the given config.
    pub fn new(config: ErrorLogSinkConfig) -> Self {
        tracing::debug!("Error log sink initialized");
        Self { config }
    }

    /// Log an error to the error log file.
    pub fn log_error(&self, error: &dyn std::error::Error) {
        let error_str = format!("{error}");

        tracing::error!("{}", error_str);

        let path = get_errors_path(&self.config.errors_dir);
        let message = serde_json::json!({
            "error": error_str,
        });
        append_to_log(
            &path,
            message,
            &self.config.session_id,
            &self.config.version,
        );
    }

    /// Log an error string to the error log file.
    pub fn log_error_str(&self, error: &str) {
        tracing::error!("{}", error);

        let path = get_errors_path(&self.config.errors_dir);
        let message = serde_json::json!({
            "error": error,
        });
        append_to_log(
            &path,
            message,
            &self.config.session_id,
            &self.config.version,
        );
    }

    /// Log an MCP server error to the MCP log file.
    pub fn log_mcp_error(&self, server_name: &str, error: &str) {
        tracing::error!("MCP server \"{}\": {}", server_name, error);

        let log_file = get_mcp_logs_path(&self.config.mcp_logs_base_dir, server_name);
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let error_info = serde_json::json!({
            "error": error,
            "timestamp": Utc::now().to_rfc3339(),
            "sessionId": self.config.session_id,
            "cwd": cwd,
        });
        write_to_log(&log_file, &error_info);
    }

    /// Log an MCP debug message to the MCP log file.
    pub fn log_mcp_debug(&self, server_name: &str, message: &str) {
        tracing::debug!("MCP server \"{}\": {}", server_name, message);

        let log_file = get_mcp_logs_path(&self.config.mcp_logs_base_dir, server_name);
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let debug_info = serde_json::json!({
            "debug": message,
            "timestamp": Utc::now().to_rfc3339(),
            "sessionId": self.config.session_id,
            "cwd": cwd,
        });
        write_to_log(&log_file, &debug_info);
    }

    /// Get the errors log path.
    pub fn get_errors_path(&self) -> PathBuf {
        get_errors_path(&self.config.errors_dir)
    }

    /// Get the MCP logs path for a server.
    pub fn get_mcp_logs_path(&self, server_name: &str) -> PathBuf {
        get_mcp_logs_path(&self.config.mcp_logs_base_dir, server_name)
    }
}
