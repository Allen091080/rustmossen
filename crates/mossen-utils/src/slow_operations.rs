//! Slow operation detection and wrapped JSON/clone operations.
//!
//! Provides instrumented wrappers around JSON serialization, parsing,
//! cloning, and file writes to detect performance bottlenecks.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Threshold in milliseconds for logging slow operations.
static SLOW_OPERATION_THRESHOLD_MS: Lazy<f64> = Lazy::new(|| {
    if let Ok(val) = std::env::var("MOSSEN_CODE_SLOW_OPERATION_THRESHOLD_MS") {
        if let Ok(parsed) = val.parse::<f64>() {
            if parsed >= 0.0 {
                return parsed;
            }
        }
    }
    if std::env::var("NODE_ENV").map(|v| v == "development").unwrap_or(false) {
        return 20.0;
    }
    if std::env::var("USER_TYPE").map(|v| v == "ant").unwrap_or(false) {
        return 300.0;
    }
    f64::INFINITY
});

/// Re-entrancy guard.
static IS_LOGGING: AtomicBool = AtomicBool::new(false);

/// Callback type for slow operation reporting.
pub type SlowOperationCallback = Box<dyn Fn(&str, f64) + Send + Sync>;

/// Global callback for slow operations.
static SLOW_OP_CALLBACK: Lazy<Mutex<Option<SlowOperationCallback>>> = Lazy::new(|| Mutex::new(None));

/// Set the slow operation callback.
pub fn set_slow_operation_callback(cb: SlowOperationCallback) {
    *SLOW_OP_CALLBACK.lock() = Some(cb);
}

/// Get the slow operation threshold.
pub fn get_slow_operation_threshold_ms() -> f64 {
    *SLOW_OPERATION_THRESHOLD_MS
}

/// Extract the caller frame from a backtrace.
pub fn caller_frame() -> String {
    let bt = std::backtrace::Backtrace::capture();
    let bt_str = format!("{}", bt);
    for line in bt_str.lines() {
        if line.contains("slow_operations") {
            continue;
        }
        if let Some(idx) = line.rfind('/') {
            let rest = &line[idx + 1..];
            if let Some(colon) = rest.find(':') {
                return format!(" @ {}", &rest[..colon + rest[colon + 1..].find(':').map_or(rest.len() - colon - 1, |i| i + 1)]);
            }
        }
    }
    String::new()
}

/// A guard that times an operation and logs if slow.
pub struct SlowOperationGuard {
    description: String,
    start: Instant,
}

impl SlowOperationGuard {
    /// Create a new slow operation guard.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            start: Instant::now(),
        }
    }
}

impl Drop for SlowOperationGuard {
    fn drop(&mut self) {
        let duration_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        if duration_ms > *SLOW_OPERATION_THRESHOLD_MS
            && !IS_LOGGING.swap(true, Ordering::SeqCst)
        {
            let desc = format!("{}{}", self.description, caller_frame());
            tracing::debug!("[SLOW OPERATION DETECTED] {} ({:.1}ms)", desc, duration_ms);
            if let Some(ref cb) = *SLOW_OP_CALLBACK.lock() {
                cb(&desc, duration_ms);
            }
            IS_LOGGING.store(false, Ordering::SeqCst);
        }
    }
}

/// Create a slow operation guard (no-op in external builds without the feature).
pub fn slow_logging(description: impl Into<String>) -> SlowOperationGuard {
    SlowOperationGuard::new(description)
}

/// Wrapped JSON stringify with slow operation logging.
pub fn json_stringify(value: &serde_json::Value) -> String {
    let _guard = slow_logging(format!("JSON.stringify({})", describe_value(value)));
    serde_json::to_string(value).unwrap_or_default()
}

/// Wrapped JSON stringify with pretty printing.
pub fn json_stringify_pretty(value: &serde_json::Value) -> String {
    let _guard = slow_logging(format!("JSON.stringify({})", describe_value(value)));
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// Wrapped JSON parse with slow operation logging.
pub fn json_parse(text: &str) -> Result<serde_json::Value, serde_json::Error> {
    let desc = if text.len() > 80 {
        format!("JSON.parse({}...)", &text[..80])
    } else {
        format!("JSON.parse({})", text)
    };
    let _guard = slow_logging(desc);
    serde_json::from_str(text)
}

/// Wrapped clone with slow operation logging.
pub fn clone_value<T: Clone>(value: &T, description: &str) -> T {
    let _guard = slow_logging(format!("structuredClone({})", description));
    value.clone()
}

/// Wrapped deep clone (same as clone in Rust).
pub fn clone_deep<T: Clone>(value: &T, description: &str) -> T {
    let _guard = slow_logging(format!("cloneDeep({})", description));
    value.clone()
}

/// Write file sync with slow operation logging and optional flush.
pub fn write_file_sync(file_path: &Path, data: &[u8], flush: bool) -> std::io::Result<()> {
    let desc = format!("fs.writeFileSync({}, {} bytes)", file_path.display(), data.len());
    let _guard = slow_logging(desc);

    if flush {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)?;
        file.write_all(data)?;
        file.sync_all()?;
    } else {
        std::fs::write(file_path, data)?;
    }
    Ok(())
}

/// Describe a JSON value for logging purposes.
fn describe_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(arr) => format!("Array[{}]", arr.len()),
        serde_json::Value::Object(obj) => format!("Object{{{} keys}}", obj.len()),
        serde_json::Value::String(s) => {
            if s.len() > 80 {
                format!("\"{}…\"", &s[..80])
            } else {
                format!("\"{}\"", s)
            }
        }
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
    }
}

/// 对应 TS `writeFileSync_DEPRECATED`。同步写入，仅用于过渡期。
#[doc(hidden)]
pub fn write_file_sync_deprecated(path: &str, contents: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

/// 对应 TS `writeFileSyncAndFlush_DEPRECATED`：写入并强制 fsync。
#[doc(hidden)]
pub fn write_file_sync_and_flush_deprecated(path: &str, contents: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    f.write_all(contents)?;
    f.flush()?;
    f.sync_all()?;
    Ok(())
}
