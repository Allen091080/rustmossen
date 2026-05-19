// Translated from utils/task/*.ts (5 files)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use anyhow::Result;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;

// ============================================================================
// framework.ts
// ============================================================================

pub const POLL_INTERVAL_MS: u64 = 1000;
pub const STOPPED_DISPLAY_MS: u64 = 3_000;
pub const PANEL_GRACE_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Killed,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Killed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Bash,
    Hook,
    LocalAgent,
    Workflow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAttachment {
    pub task_id: String,
    pub tool_use_id: Option<String>,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub description: String,
    pub delta_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub id: String,
    pub tool_use_id: Option<String>,
    pub description: String,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub notified: bool,
    pub output_offset: usize,
    pub start_time: u64,
    #[serde(default)]
    pub evict_after: Option<u64>,
    #[serde(default)]
    pub retain: bool,
}

/// Get human-readable status text.
pub fn get_status_text(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Completed => "completed successfully",
        TaskStatus::Failed => "failed",
        TaskStatus::Killed => "was stopped",
        TaskStatus::Running => "is running",
        TaskStatus::Pending => "is pending",
    }
}

/// Get all running tasks.
pub fn get_running_tasks(tasks: &HashMap<String, TaskState>) -> Vec<&TaskState> {
    tasks.values().filter(|t| t.status == TaskStatus::Running).collect()
}

// ============================================================================
// outputFormatting.ts
// ============================================================================

pub const TASK_MAX_OUTPUT_UPPER_LIMIT: usize = 160_000;
pub const TASK_MAX_OUTPUT_DEFAULT: usize = 32_000;

pub fn get_max_task_output_length() -> usize {
    std::env::var("TASK_MAX_OUTPUT_LENGTH")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|v| v.min(TASK_MAX_OUTPUT_UPPER_LIMIT))
        .unwrap_or(TASK_MAX_OUTPUT_DEFAULT)
}

/// Format task output for API consumption, truncating if too large.
pub fn format_task_output(output: &str, task_id: &str) -> (String, bool) {
    let max_len = get_max_task_output_length();
    if output.len() <= max_len {
        return (output.to_string(), false);
    }
    let file_path = get_task_output_path(task_id);
    let header = format!("[Truncated. Full output: {}]\n\n", file_path);
    let available_space = max_len.saturating_sub(header.len());
    let truncated = &output[output.len().saturating_sub(available_space)..];
    (format!("{}{}", header, truncated), true)
}

// ============================================================================
// sdkProgress.ts
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressParams {
    pub task_id: String,
    pub tool_use_id: Option<String>,
    pub description: String,
    pub start_time: u64,
    pub total_tokens: u64,
    pub tool_uses: u64,
    pub last_tool_name: Option<String>,
    pub summary: Option<String>,
}

pub fn emit_task_progress(_params: TaskProgressParams) {
    // In a real implementation, would enqueue SDK event
}

// ============================================================================
// diskOutput.ts
// ============================================================================

pub const MAX_TASK_OUTPUT_BYTES: u64 = 5 * 1024 * 1024 * 1024;
pub const MAX_TASK_OUTPUT_BYTES_DISPLAY: &str = "5GB";
const DEFAULT_MAX_READ_BYTES: usize = 8 * 1024 * 1024;

static TASK_OUTPUT_DIR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

pub fn get_task_output_dir() -> String {
    let mut dir = TASK_OUTPUT_DIR.lock().unwrap();
    if dir.is_none() {
        let temp_dir = std::env::temp_dir().join("mossen").join("tasks");
        *dir = Some(temp_dir.to_string_lossy().to_string());
    }
    dir.clone().unwrap()
}

pub fn reset_task_output_dir_for_test() {
    *TASK_OUTPUT_DIR.lock().unwrap() = None;
}

pub fn get_task_output_path(task_id: &str) -> String {
    format!("{}/{}.output", get_task_output_dir(), task_id)
}

async fn ensure_output_dir() -> Result<()> {
    fs::create_dir_all(get_task_output_dir()).await?;
    Ok(())
}

/// Disk-backed task output writer.
pub struct DiskTaskOutput {
    path: String,
    queue: Mutex<Vec<String>>,
    bytes_written: Mutex<u64>,
    capped: Mutex<bool>,
}

impl DiskTaskOutput {
    pub fn new(task_id: &str) -> Self {
        Self {
            path: get_task_output_path(task_id),
            queue: Mutex::new(Vec::new()),
            bytes_written: Mutex::new(0),
            capped: Mutex::new(false),
        }
    }

    pub fn append(&self, content: &str) {
        let mut capped = self.capped.lock().unwrap();
        if *capped {
            return;
        }
        let mut bytes = self.bytes_written.lock().unwrap();
        *bytes += content.len() as u64;
        if *bytes > MAX_TASK_OUTPUT_BYTES {
            *capped = true;
            self.queue.lock().unwrap().push(format!(
                "\n[output truncated: exceeded {} disk cap]\n",
                MAX_TASK_OUTPUT_BYTES_DISPLAY
            ));
        } else {
            self.queue.lock().unwrap().push(content.to_string());
        }
    }

    pub async fn flush(&self) -> Result<()> {
        let chunks: Vec<String> = {
            let mut queue = self.queue.lock().unwrap();
            queue.drain(..).collect()
        };
        if chunks.is_empty() {
            return Ok(());
        }
        ensure_output_dir().await?;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        for chunk in chunks {
            file.write_all(chunk.as_bytes()).await?;
        }
        Ok(())
    }

    pub fn cancel(&self) {
        self.queue.lock().unwrap().clear();
    }
}

static OUTPUTS: Lazy<Mutex<HashMap<String, Arc<DiskTaskOutput>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Append output to a task's disk file asynchronously.
pub fn append_task_output(task_id: &str, content: &str) {
    let output = get_or_create_output(task_id);
    output.append(content);
}

fn get_or_create_output(task_id: &str) -> Arc<DiskTaskOutput> {
    let mut outputs = OUTPUTS.lock().unwrap();
    outputs
        .entry(task_id.to_string())
        .or_insert_with(|| Arc::new(DiskTaskOutput::new(task_id)))
        .clone()
}

/// Wait for all pending writes for a task to complete.
pub async fn flush_task_output(task_id: &str) -> Result<()> {
    let output = {
        let outputs = OUTPUTS.lock().unwrap();
        outputs.get(task_id).cloned()
    };
    if let Some(output) = output {
        output.flush().await?;
    }
    Ok(())
}

/// Evict a task's DiskTaskOutput from the in-memory map after flushing.
pub async fn evict_task_output(task_id: &str) -> Result<()> {
    flush_task_output(task_id).await?;
    OUTPUTS.lock().unwrap().remove(task_id);
    Ok(())
}

/// Get delta (new content) since last read.
pub async fn get_task_output_delta(
    task_id: &str,
    from_offset: usize,
    max_bytes: Option<usize>,
) -> (String, usize) {
    let max = max_bytes.unwrap_or(DEFAULT_MAX_READ_BYTES);
    let path = get_task_output_path(task_id);
    match tokio::fs::read(&path).await {
        Ok(data) => {
            if from_offset >= data.len() {
                return (String::new(), from_offset);
            }
            let end = (from_offset + max).min(data.len());
            let content = String::from_utf8_lossy(&data[from_offset..end]).to_string();
            (content, end)
        }
        Err(_) => (String::new(), from_offset),
    }
}

/// Get output for a task, reading the tail of the file.
pub async fn get_task_output(task_id: &str, max_bytes: Option<usize>) -> String {
    let max = max_bytes.unwrap_or(DEFAULT_MAX_READ_BYTES);
    let path = get_task_output_path(task_id);
    match tokio::fs::read(&path).await {
        Ok(data) => {
            if data.len() <= max {
                String::from_utf8_lossy(&data).to_string()
            } else {
                let omitted_kb = (data.len() - max) / 1024;
                let tail = String::from_utf8_lossy(&data[data.len() - max..]).to_string();
                format!("[{}KB of earlier output omitted]\n{}", omitted_kb, tail)
            }
        }
        Err(_) => String::new(),
    }
}

/// Get the current size of a task's output file.
pub async fn get_task_output_size(task_id: &str) -> u64 {
    let path = get_task_output_path(task_id);
    tokio::fs::metadata(&path)
        .await
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Clean up a task's output file and write queue.
pub async fn cleanup_task_output(task_id: &str) {
    {
        let mut outputs = OUTPUTS.lock().unwrap();
        if let Some(output) = outputs.remove(task_id) {
            output.cancel();
        }
    }
    let path = get_task_output_path(task_id);
    let _ = tokio::fs::remove_file(&path).await;
}

/// Initialize output file for a new task.
pub async fn init_task_output(task_id: &str) -> Result<String> {
    ensure_output_dir().await?;
    let output_path = get_task_output_path(task_id);
    tokio::fs::write(&output_path, b"").await?;
    Ok(output_path)
}

/// Initialize output file as a symlink to another file.
pub async fn init_task_output_as_symlink(task_id: &str, target_path: &str) -> Result<String> {
    ensure_output_dir().await?;
    let output_path = get_task_output_path(task_id);
    let _ = tokio::fs::remove_file(&output_path).await;
    #[cfg(unix)]
    {
        tokio::fs::symlink(target_path, &output_path).await?;
    }
    #[cfg(not(unix))]
    {
        tokio::fs::write(&output_path, b"").await?;
    }
    Ok(output_path)
}

// ============================================================================
// TaskOutput.ts
// ============================================================================

/// Single source of truth for a shell command's output.
pub struct TaskOutput {
    pub task_id: String,
    pub path: String,
    pub stdout_to_file: bool,
    stdout_buffer: Mutex<String>,
    stderr_buffer: Mutex<String>,
    disk: Mutex<Option<Arc<DiskTaskOutput>>>,
    total_lines: Mutex<usize>,
    total_bytes: Mutex<usize>,
    max_memory: usize,
    output_file_redundant: Mutex<bool>,
    output_file_size: Mutex<u64>,
}

impl TaskOutput {
    pub fn new(task_id: &str, stdout_to_file: bool, max_memory: Option<usize>) -> Self {
        let max = max_memory.unwrap_or(8 * 1024 * 1024);
        Self {
            task_id: task_id.to_string(),
            path: get_task_output_path(task_id),
            stdout_to_file,
            stdout_buffer: Mutex::new(String::new()),
            stderr_buffer: Mutex::new(String::new()),
            disk: Mutex::new(None),
            total_lines: Mutex::new(0),
            total_bytes: Mutex::new(0),
            max_memory: max,
            output_file_redundant: Mutex::new(false),
            output_file_size: Mutex::new(0),
        }
    }

    pub fn write_stdout(&self, data: &str) {
        self.write_buffered(data, false);
    }

    pub fn write_stderr(&self, data: &str) {
        self.write_buffered(data, true);
    }

    fn write_buffered(&self, data: &str, is_stderr: bool) {
        *self.total_bytes.lock().unwrap() += data.len();

        // Count lines
        let line_count = data.chars().filter(|c| *c == '\n').count();
        *self.total_lines.lock().unwrap() += line_count;

        // Check if already spilled to disk
        let has_disk = self.disk.lock().unwrap().is_some();
        if has_disk {
            let disk = self.disk.lock().unwrap();
            if let Some(d) = disk.as_ref() {
                let content = if is_stderr {
                    format!("[stderr] {}", data)
                } else {
                    data.to_string()
                };
                d.append(&content);
            }
            return;
        }

        // Check memory limit
        let stdout_len = self.stdout_buffer.lock().unwrap().len();
        let stderr_len = self.stderr_buffer.lock().unwrap().len();
        let total_mem = stdout_len + stderr_len + data.len();
        if total_mem > self.max_memory {
            self.spill_to_disk(
                if is_stderr { Some(data) } else { None },
                if is_stderr { None } else { Some(data) },
            );
            return;
        }

        if is_stderr {
            self.stderr_buffer.lock().unwrap().push_str(data);
        } else {
            self.stdout_buffer.lock().unwrap().push_str(data);
        }
    }

    fn spill_to_disk(&self, stderr_chunk: Option<&str>, stdout_chunk: Option<&str>) {
        let disk = Arc::new(DiskTaskOutput::new(&self.task_id));

        let stdout = self.stdout_buffer.lock().unwrap().clone();
        if !stdout.is_empty() {
            disk.append(&stdout);
            self.stdout_buffer.lock().unwrap().clear();
        }
        let stderr = self.stderr_buffer.lock().unwrap().clone();
        if !stderr.is_empty() {
            disk.append(&format!("[stderr] {}", stderr));
            self.stderr_buffer.lock().unwrap().clear();
        }
        if let Some(chunk) = stdout_chunk {
            disk.append(chunk);
        }
        if let Some(chunk) = stderr_chunk {
            disk.append(&format!("[stderr] {}", chunk));
        }

        *self.disk.lock().unwrap() = Some(disk);
    }

    pub async fn get_stdout(&self) -> String {
        if self.stdout_to_file {
            return self.read_stdout_from_file().await;
        }
        let has_disk = self.disk.lock().unwrap().is_some();
        if has_disk {
            let total_bytes = *self.total_bytes.lock().unwrap();
            let size_kb = total_bytes / 1024;
            return format!(
                "\nOutput truncated ({}KB total). Full output saved to: {}",
                size_kb, self.path
            );
        }
        self.stdout_buffer.lock().unwrap().clone()
    }

    async fn read_stdout_from_file(&self) -> String {
        let max_bytes = get_max_task_output_length();
        match tokio::fs::read(&self.path).await {
            Ok(data) => {
                *self.output_file_size.lock().unwrap() = data.len() as u64;
                *self.output_file_redundant.lock().unwrap() = data.len() <= max_bytes;
                let end = max_bytes.min(data.len());
                String::from_utf8_lossy(&data[..end]).to_string()
            }
            Err(_) => {
                *self.output_file_redundant.lock().unwrap() = true;
                String::new()
            }
        }
    }

    pub fn get_stderr(&self) -> String {
        if self.disk.lock().unwrap().is_some() {
            return String::new();
        }
        self.stderr_buffer.lock().unwrap().clone()
    }

    pub fn is_overflowed(&self) -> bool {
        self.disk.lock().unwrap().is_some()
    }

    pub fn total_lines(&self) -> usize {
        *self.total_lines.lock().unwrap()
    }

    pub fn total_bytes(&self) -> usize {
        *self.total_bytes.lock().unwrap()
    }

    pub fn output_file_redundant(&self) -> bool {
        *self.output_file_redundant.lock().unwrap()
    }

    pub fn output_file_size(&self) -> u64 {
        *self.output_file_size.lock().unwrap()
    }

    pub fn spill_to_disk_force(&self) {
        if self.disk.lock().unwrap().is_none() {
            self.spill_to_disk(None, None);
        }
    }

    pub async fn flush(&self) {
        if let Some(disk) = self.disk.lock().unwrap().as_ref() {
            let _ = disk.flush().await;
        }
    }

    pub async fn delete_output_file(&self) {
        let _ = tokio::fs::remove_file(&self.path).await;
    }

    pub fn clear(&self) {
        self.stdout_buffer.lock().unwrap().clear();
        self.stderr_buffer.lock().unwrap().clear();
        if let Some(disk) = self.disk.lock().unwrap().as_ref() {
            disk.cancel();
        }
    }
}

// =============================================================================
// 与 TS `task/framework.ts` 对齐的高层入口。Rust 端在 `tasks.rs` / 此模块共同
// 维护 `HashMap<task_id, TaskState>`；下面几个函数提供与 TS 同名的入口。
// =============================================================================

use std::sync::Mutex as StdMutex;

static TASK_REGISTRY: once_cell::sync::Lazy<StdMutex<HashMap<String, TaskState>>> =
    once_cell::sync::Lazy::new(|| StdMutex::new(HashMap::new()));

/// 更新指定 task 的状态字段（对应 TS `updateTaskState`）。返回是否完成更新。
pub fn update_task_state<F: FnOnce(&mut TaskState)>(task_id: &str, update: F) -> bool {
    let mut reg = TASK_REGISTRY.lock().unwrap();
    if let Some(state) = reg.get_mut(task_id) {
        update(state);
        true
    } else {
        false
    }
}

/// 注册一个新任务（对应 TS `registerTask`）。
pub fn register_task(task: TaskState) {
    let mut reg = TASK_REGISTRY.lock().unwrap();
    reg.insert(task.id.clone(), task);
}

/// 移除终态任务（对应 TS `evictTerminalTask`）。
pub fn evict_terminal_task(task_id: &str) -> bool {
    let mut reg = TASK_REGISTRY.lock().unwrap();
    if let Some(state) = reg.get(task_id) {
        if state.status.is_terminal() {
            reg.remove(task_id);
            return true;
        }
    }
    false
}

/// 为所有运行中任务生成附件描述（对应 TS `generateTaskAttachments`）。
pub async fn generate_task_attachments() -> Vec<TaskAttachment> {
    let reg = TASK_REGISTRY.lock().unwrap();
    reg.values()
        .filter(|s| !s.status.is_terminal())
        .map(|s| TaskAttachment {
            task_id: s.id.clone(),
            tool_use_id: s.tool_use_id.clone(),
            task_type: s.task_type,
            status: s.status,
            description: s.description.clone(),
            delta_summary: None,
        })
        .collect()
}

/// 根据 attachment offset 与驱逐策略对任务列表做调整。
pub fn apply_task_offsets_and_evictions(_offsets: &HashMap<String, usize>) {
    let mut reg = TASK_REGISTRY.lock().unwrap();
    reg.retain(|_, state| !state.status.is_terminal());
}

/// 轮询任务状态（对应 TS `pollTasks`）。返回当前所有任务的快照。
pub async fn poll_tasks() -> Vec<TaskState> {
    let reg = TASK_REGISTRY.lock().unwrap();
    reg.values().cloned().collect()
}

/// 对应 TS `_clearOutputsForTest`：仅测试用，清空 task 输出注册表与磁盘文件。
#[doc(hidden)]
pub async fn _clear_outputs_for_test() {
    {
        let mut reg = TASK_REGISTRY.lock().unwrap();
        reg.clear();
    }
    let dir = get_task_output_dir();
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
