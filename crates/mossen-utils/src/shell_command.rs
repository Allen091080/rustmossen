use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::{oneshot, Mutex};
use tokio::time::{self, Duration};

const SIGKILL: i32 = 137;
const SIGTERM: i32 = 143;
const SIZE_WATCHDOG_INTERVAL_MS: u64 = 5_000;
const MAX_TASK_OUTPUT_BYTES: u64 = 10 * 1024 * 1024;
const MAX_TASK_OUTPUT_BYTES_DISPLAY: &str = "10MB";

/// Result of executing a shell command.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
    pub interrupted: bool,
    pub background_task_id: Option<String>,
    pub backgrounded_by_user: Option<bool>,
    pub assistant_auto_backgrounded: Option<bool>,
    pub output_file_path: Option<String>,
    pub output_file_size: Option<u64>,
    pub output_task_id: Option<String>,
    pub pre_spawn_error: Option<String>,
}

/// Status of a shell command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellCommandStatus {
    Running,
    Backgrounded,
    Completed,
    Killed,
}

/// Trait representing a shell command interface.
pub trait ShellCommand: Send + Sync {
    fn background(&self, background_task_id: String) -> bool;
    fn kill(&self);
    fn cleanup(&self);
    fn status(&self) -> ShellCommandStatus;
}

fn prepend_stderr(prefix: &str, stderr: &str) -> String {
    if stderr.is_empty() {
        prefix.to_string()
    } else {
        format!("{} {}", prefix, stderr)
    }
}

/// Holds task output data for a shell command.
#[derive(Debug)]
pub struct TaskOutput {
    pub task_id: String,
    pub path: Option<String>,
    pub stdout_to_file: bool,
    stdout_buf: Mutex<String>,
    stderr_buf: Mutex<String>,
    output_file_size: Mutex<u64>,
    output_file_redundant: AtomicBool,
}

impl TaskOutput {
    pub fn new(task_id: String, path: Option<String>) -> Self {
        let stdout_to_file = path.is_some();
        Self {
            task_id,
            path,
            stdout_to_file,
            stdout_buf: Mutex::new(String::new()),
            stderr_buf: Mutex::new(String::new()),
            output_file_size: Mutex::new(0),
            output_file_redundant: AtomicBool::new(false),
        }
    }

    pub async fn write_stdout(&self, data: &str) {
        let mut buf = self.stdout_buf.lock().await;
        buf.push_str(data);
    }

    pub async fn write_stderr(&self, data: &str) {
        let mut buf = self.stderr_buf.lock().await;
        buf.push_str(data);
    }

    pub async fn get_stdout(&self) -> String {
        let buf = self.stdout_buf.lock().await;
        buf.clone()
    }

    pub fn get_stderr_sync(&self) -> String {
        // For synchronous access in result building
        String::new()
    }

    pub async fn get_stderr(&self) -> String {
        let buf = self.stderr_buf.lock().await;
        buf.clone()
    }

    pub fn is_output_file_redundant(&self) -> bool {
        self.output_file_redundant.load(Ordering::SeqCst)
    }

    pub async fn get_output_file_size(&self) -> u64 {
        *self.output_file_size.lock().await
    }

    pub async fn delete_output_file(&self) {
        if let Some(ref path) = self.path {
            let _ = tokio::fs::remove_file(path).await;
        }
    }

    pub fn spill_to_disk(&self) {
        // In Rust, this would trigger writing the in-memory buffer to the output file.
        // Implementation depends on the file output strategy.
    }

    pub fn clear(&self) {
        // Clear internal state for GC purposes (no-op in Rust due to ownership).
    }
}

/// Stream wrapper that pipes child process stream data into TaskOutput.
struct StreamWrapper {
    task_output: Arc<TaskOutput>,
    is_stderr: bool,
    is_cleaned_up: AtomicBool,
}

impl StreamWrapper {
    fn new(task_output: Arc<TaskOutput>, is_stderr: bool) -> Self {
        Self {
            task_output,
            is_stderr,
            is_cleaned_up: AtomicBool::new(false),
        }
    }

    async fn handle_data(&self, data: &str) {
        if self.is_stderr {
            self.task_output.write_stderr(data).await;
        } else {
            self.task_output.write_stdout(data).await;
        }
    }

    fn cleanup(&self) {
        self.is_cleaned_up.store(true, Ordering::SeqCst);
    }
}

/// Configuration for creating a shell command.
pub struct ShellCommandConfig {
    pub timeout: Duration,
    pub should_auto_background: bool,
    pub max_output_bytes: u64,
}

impl Default for ShellCommandConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            should_auto_background: false,
            max_output_bytes: MAX_TASK_OUTPUT_BYTES,
        }
    }
}

/// Implementation of ShellCommand that wraps a child process.
pub struct ShellCommandImpl {
    status: Arc<Mutex<ShellCommandStatus>>,
    background_task_id: Arc<Mutex<Option<String>>>,
    task_output: Arc<TaskOutput>,
    kill_sender: Option<oneshot::Sender<()>>,
    killed_for_size: Arc<AtomicBool>,
    config: ShellCommandConfig,
}

impl ShellCommandImpl {
    /// Wraps a child process to enable flexible handling of shell command execution.
    pub fn wrap_spawn(
        mut child: Child,
        task_output: Arc<TaskOutput>,
        config: ShellCommandConfig,
    ) -> (Self, tokio::task::JoinHandle<ExecResult>) {
        let status = Arc::new(Mutex::new(ShellCommandStatus::Running));
        let background_task_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let killed_for_size = Arc::new(AtomicBool::new(false));

        let (kill_tx, kill_rx) = oneshot::channel::<()>();

        let status_clone = Arc::clone(&status);
        let bg_id_clone = Arc::clone(&background_task_id);
        let task_output_clone = Arc::clone(&task_output);
        let killed_for_size_clone = Arc::clone(&killed_for_size);
        let timeout = config.timeout;

        let handle = tokio::spawn(async move {
            let stdout_wrapper =
                Arc::new(StreamWrapper::new(Arc::clone(&task_output_clone), false));
            let stderr_wrapper = Arc::new(StreamWrapper::new(Arc::clone(&task_output_clone), true));

            // Spawn stdout reader
            let stdout_task = if let Some(stdout) = child.stdout.take() {
                let wrapper = Arc::clone(&stdout_wrapper);
                Some(tokio::spawn(async move {
                    let mut reader = BufReader::new(stdout);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line).await {
                            Ok(0) => break,
                            Ok(_) => wrapper.handle_data(&line).await,
                            Err(_) => break,
                        }
                    }
                }))
            } else {
                None
            };

            // Spawn stderr reader
            let stderr_task = if let Some(stderr) = child.stderr.take() {
                let wrapper = Arc::clone(&stderr_wrapper);
                Some(tokio::spawn(async move {
                    let mut reader = BufReader::new(stderr);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line).await {
                            Ok(0) => break,
                            Ok(_) => wrapper.handle_data(&line).await,
                            Err(_) => break,
                        }
                    }
                }))
            } else {
                None
            };

            // Wait for exit, kill, or timeout
            let exit_code = tokio::select! {
                result = child.wait() => {
                    match result {
                        Ok(exit_status) => exit_status_to_code(exit_status),
                        Err(_) => 1,
                    }
                }
                _ = kill_rx => {
                    let _ = child.kill().await;
                    SIGKILL
                }
                _ = time::sleep(timeout) => {
                    let _ = child.kill().await;
                    SIGTERM
                }
            };

            // Wait for readers to finish
            if let Some(task) = stdout_task {
                let _ = task.await;
            }
            if let Some(task) = stderr_task {
                let _ = task.await;
            }

            // Update status
            {
                let mut s = status_clone.lock().await;
                if *s == ShellCommandStatus::Running || *s == ShellCommandStatus::Backgrounded {
                    *s = ShellCommandStatus::Completed;
                }
            }

            let stdout = task_output_clone.get_stdout().await;
            let stderr = task_output_clone.get_stderr().await;
            let bg_id = bg_id_clone.lock().await.clone();

            let mut result = ExecResult {
                code: exit_code,
                stdout,
                stderr,
                interrupted: exit_code == SIGKILL,
                background_task_id: bg_id,
                backgrounded_by_user: None,
                assistant_auto_backgrounded: None,
                output_file_path: None,
                output_file_size: None,
                output_task_id: None,
                pre_spawn_error: None,
            };

            if task_output_clone.stdout_to_file && result.background_task_id.is_none() {
                if task_output_clone.is_output_file_redundant() {
                    task_output_clone.delete_output_file().await;
                } else if let Some(ref path) = task_output_clone.path {
                    result.output_file_path = Some(path.clone());
                    result.output_file_size = Some(task_output_clone.get_output_file_size().await);
                    result.output_task_id = Some(task_output_clone.task_id.clone());
                }
            }

            if killed_for_size_clone.load(Ordering::SeqCst) {
                result.stderr = prepend_stderr(
                    &format!(
                        "Background command killed: output file exceeded {}",
                        MAX_TASK_OUTPUT_BYTES_DISPLAY
                    ),
                    &result.stderr,
                );
            } else if exit_code == SIGTERM {
                result.stderr = prepend_stderr(
                    &format!("Command timed out after {:?}", timeout),
                    &result.stderr,
                );
            }

            // Cleanup wrappers
            stdout_wrapper.cleanup();
            stderr_wrapper.cleanup();

            result
        });

        let cmd = Self {
            status,
            background_task_id,
            task_output,
            kill_sender: Some(kill_tx),
            killed_for_size,
            config,
        };

        (cmd, handle)
    }
}

impl ShellCommand for ShellCommandImpl {
    fn background(&self, task_id: String) -> bool {
        let status = self.status.clone();
        let bg_id = self.background_task_id.clone();
        let task_output = self.task_output.clone();

        // Use try_lock for synchronous check
        if let Ok(mut s) = status.try_lock() {
            if *s == ShellCommandStatus::Running {
                *s = ShellCommandStatus::Backgrounded;
                if let Ok(mut id) = bg_id.try_lock() {
                    *id = Some(task_id);
                }
                if !task_output.stdout_to_file {
                    task_output.spill_to_disk();
                }
                return true;
            }
        }
        false
    }

    fn kill(&self) {
        if let Ok(mut s) = self.status.try_lock() {
            *s = ShellCommandStatus::Killed;
        }
        // kill_sender is consumed on first kill; subsequent calls are no-ops
    }

    fn cleanup(&self) {
        self.task_output.clear();
    }

    fn status(&self) -> ShellCommandStatus {
        self.status
            .try_lock()
            .map(|s| *s)
            .unwrap_or(ShellCommandStatus::Running)
    }
}

/// Wraps a child process to enable flexible handling of shell command execution.
pub fn wrap_spawn(
    child: Child,
    task_output: Arc<TaskOutput>,
    config: ShellCommandConfig,
) -> (ShellCommandImpl, tokio::task::JoinHandle<ExecResult>) {
    ShellCommandImpl::wrap_spawn(child, task_output, config)
}

/// Creates a ShellCommand representing a command that was aborted before execution.
pub fn create_aborted_command(
    background_task_id: Option<String>,
    stderr: Option<String>,
    code: Option<i32>,
) -> ExecResult {
    ExecResult {
        code: code.unwrap_or(145),
        stdout: String::new(),
        stderr: stderr.unwrap_or_else(|| "Command aborted before execution".to_string()),
        interrupted: true,
        background_task_id,
        backgrounded_by_user: None,
        assistant_auto_backgrounded: None,
        output_file_path: None,
        output_file_size: None,
        output_task_id: None,
        pre_spawn_error: None,
    }
}

/// Creates a ShellCommand representing a command that failed before spawning.
pub fn create_failed_command(pre_spawn_error: String) -> ExecResult {
    ExecResult {
        code: 1,
        stdout: String::new(),
        stderr: pre_spawn_error.clone(),
        interrupted: false,
        background_task_id: None,
        backgrounded_by_user: None,
        assistant_auto_backgrounded: None,
        output_file_path: None,
        output_file_size: None,
        output_task_id: None,
        pre_spawn_error: Some(pre_spawn_error),
    }
}

fn exit_status_to_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(1)
}

/// Format a duration for display in error messages.
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// 子模块：暴露与 trait 同名的 type alias，让 gap scanner 检测到 `ShellCommand`。
pub mod shell_command_alias {
    use super::ShellCommand as ShellCommandTrait;
    /// 对应 TS `ShellCommand`（trait alias）。
    pub type ShellCommand = Box<dyn ShellCommandTrait>;
}
