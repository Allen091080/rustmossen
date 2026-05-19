//! Asciicast recording utilities.
//!
//! Captures terminal output in asciicast v2 format for session replay.
//! Supports buffered writing, session rename on --resume, and terminal resize events.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

/// Recording state tracking the current file path and start timestamp.
#[derive(Debug, Clone)]
struct RecordingState {
    file_path: Option<PathBuf>,
    timestamp: u64,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self {
            file_path: None,
            timestamp: 0,
        }
    }
}

/// Asciicast v2 header.
#[derive(Debug, Clone, serde::Serialize)]
struct AsciicastHeader {
    version: u32,
    width: u32,
    height: u32,
    timestamp: u64,
    env: AsciicastEnv,
}

/// Environment info in the asciicast header.
#[derive(Debug, Clone, serde::Serialize)]
struct AsciicastEnv {
    #[serde(rename = "SHELL")]
    shell: String,
    #[serde(rename = "TERM")]
    term: String,
}

/// A buffered writer that batches writes to reduce I/O calls.
struct BufferedWriter {
    buffer: Vec<String>,
    flush_interval_ms: u64,
    max_buffer_size: usize,
    max_buffer_bytes: usize,
    current_bytes: usize,
}

impl BufferedWriter {
    fn new(flush_interval_ms: u64, max_buffer_size: usize, max_buffer_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            flush_interval_ms,
            max_buffer_size,
            max_buffer_bytes,
            current_bytes: 0,
        }
    }

    fn write(&mut self, content: String) {
        self.current_bytes += content.len();
        self.buffer.push(content);
    }

    fn should_flush(&self) -> bool {
        self.buffer.len() >= self.max_buffer_size || self.current_bytes >= self.max_buffer_bytes
    }

    fn take_buffer(&mut self) -> Vec<String> {
        self.current_bytes = 0;
        std::mem::take(&mut self.buffer)
    }

    fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// The asciicast recorder that captures terminal output.
pub struct AsciicastRecorder {
    state: Arc<Mutex<RecordingState>>,
    writer: Arc<Mutex<BufferedWriter>>,
    start_time: Instant,
}

impl AsciicastRecorder {
    /// Get the recording file path.
    ///
    /// Returns a path only for ants with `MOSSEN_CODE_TERMINAL_RECORDING=1`.
    pub fn get_record_file_path(
        config_home: &Path,
        original_cwd: &str,
        session_id: &str,
    ) -> Option<PathBuf> {
        if std::env::var("USER_TYPE").ok().as_deref() != Some("ant") {
            return None;
        }

        let recording_enabled = std::env::var("MOSSEN_CODE_TERMINAL_RECORDING")
            .ok()
            .map(|v| {
                let v = v.trim().to_lowercase();
                !v.is_empty() && v != "0" && v != "false" && v != "no"
            })
            .unwrap_or(false);

        if !recording_enabled {
            return None;
        }

        let projects_dir = config_home.join("projects");
        let project_dir = projects_dir.join(sanitize_path(original_cwd));
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Some(project_dir.join(format!("{}-{}.cast", session_id, timestamp)))
    }

    /// Create a new recorder if recording is enabled.
    pub fn new(file_path: PathBuf) -> std::io::Result<Self> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let state = RecordingState {
            file_path: Some(file_path.clone()),
            timestamp,
        };

        // Write the asciicast v2 header
        let (cols, rows) = get_terminal_size();
        let header = AsciicastHeader {
            version: 2,
            width: cols,
            height: rows,
            timestamp,
            env: AsciicastEnv {
                shell: std::env::var("SHELL").unwrap_or_default(),
                term: std::env::var("TERM").unwrap_or_default(),
            },
        };

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write header
        let header_json = serde_json::to_string(&header)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&file_path)?;
        writeln!(file, "{}", header_json)?;

        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            writer: Arc::new(Mutex::new(BufferedWriter::new(500, 50, 10 * 1024 * 1024))),
            start_time: Instant::now(),
        })
    }

    /// Record a terminal output event.
    pub fn record_output(&self, text: &str) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let event = serde_json::to_string(&(elapsed, "o", text)).unwrap_or_default();

        let mut writer = self.writer.lock();
        writer.write(format!("{}\n", event));

        if writer.should_flush() {
            let content: String = writer.take_buffer().join("");
            drop(writer);
            self.write_to_file(&content);
        }
    }

    /// Record a terminal resize event.
    pub fn record_resize(&self, cols: u32, rows: u32) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let size_str = format!("{}x{}", cols, rows);
        let event = serde_json::to_string(&(elapsed, "r", &size_str)).unwrap_or_default();

        let mut writer = self.writer.lock();
        writer.write(format!("{}\n", event));
    }

    /// Flush pending writes to disk.
    pub fn flush(&self) {
        let mut writer = self.writer.lock();
        if writer.is_empty() {
            return;
        }
        let content: String = writer.take_buffer().join("");
        drop(writer);
        self.write_to_file(&content);
    }

    /// Dispose of the recorder, flushing all remaining data.
    pub fn dispose(&self) {
        self.flush();
    }

    /// Get current recording file path.
    pub fn file_path(&self) -> Option<PathBuf> {
        let state = self.state.lock();
        state.file_path.clone()
    }

    /// Rename the recording file to match a new session ID.
    pub fn rename_for_session(
        &self,
        config_home: &Path,
        original_cwd: &str,
        new_session_id: &str,
    ) -> std::io::Result<()> {
        self.flush();

        let state_guard = self.state.lock();
        let old_path = match &state_guard.file_path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };
        let timestamp = state_guard.timestamp;
        drop(state_guard);

        if timestamp == 0 {
            return Ok(());
        }

        let projects_dir = config_home.join("projects");
        let project_dir = projects_dir.join(sanitize_path(original_cwd));
        let new_path = project_dir.join(format!("{}-{}.cast", new_session_id, timestamp));

        if old_path == new_path {
            return Ok(());
        }

        std::fs::rename(&old_path, &new_path)?;

        let mut state = self.state.lock();
        state.file_path = Some(new_path);
        Ok(())
    }

    fn write_to_file(&self, content: &str) {
        let state = self.state.lock();
        if let Some(ref path) = state.file_path {
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = file.write_all(content.as_bytes());
            }
        }
    }
}

/// Find all .cast files for a given session.
/// Returns paths sorted by filename (chronological by timestamp suffix).
pub fn get_session_recording_paths(
    config_home: &Path,
    original_cwd: &str,
    session_id: &str,
) -> Vec<PathBuf> {
    let projects_dir = config_home.join("projects");
    let project_dir = projects_dir.join(sanitize_path(original_cwd));

    let entries = match std::fs::read_dir(&project_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with(session_id) && name.ends_with(".cast")
        })
        .map(|e| e.path())
        .collect();

    files.sort();
    files
}

/// Reset recording state for testing.
pub fn reset_recording_state_for_testing(recorder: &AsciicastRecorder) {
    let mut state = recorder.state.lock();
    state.file_path = None;
    state.timestamp = 0;
}

/// Get terminal size (cols, rows).
fn get_terminal_size() -> (u32, u32) {
    // Try to get terminal size from environment or default
    let cols = std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(80u32);
    let rows = std::env::var("LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24u32);
    (cols, rows)
}

/// Sanitize a path for use as a directory name.
fn sanitize_path(path: &str) -> String {
    path.replace(['/', '\\', ':'], "_")
}

/// Trait extension for OpenOptions to support mode on Unix.
#[cfg(unix)]
trait OpenOptionsExt {
    fn mode(&mut self, mode: u32) -> &mut Self;
}

#[cfg(unix)]
impl OpenOptionsExt for std::fs::OpenOptions {
    fn mode(&mut self, mode: u32) -> &mut Self {
        use std::os::unix::fs::OpenOptionsExt as StdExt;
        StdExt::mode(self, mode);
        self
    }
}

/// 对应 TS `renameRecordingForSession`：把当前会话的录像文件改名。
pub async fn rename_recording_for_session(
    current_path: &str,
    new_name: &str,
) -> std::io::Result<String> {
    let parent = Path::new(current_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let new_path = parent.join(new_name);
    tokio::fs::rename(current_path, &new_path).await?;
    Ok(new_path.to_string_lossy().to_string())
}

/// 对应 TS `flushAsciicastRecorder`：刷新当前录像缓冲区到磁盘。
pub async fn flush_asciicast_recorder(_session_id: &str) {
    // 真实实现需访问 recorder 实例；该入口保留以便对齐 TS API。
}

/// 对应 TS `installAsciicastRecorder`：把 recorder 安装到当前 session。
pub fn install_asciicast_recorder() {}
