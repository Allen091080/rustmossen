//! Shell execution — finding and executing commands via system shell.
//!
//! Translated from utils/Shell.ts

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

use anyhow::anyhow;

const DEFAULT_TIMEOUT_MS: u64 = 30 * 60 * 1000; // 30 minutes

/// Shell configuration.
#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub shell_path: String,
    pub shell_type: ShellType,
}

/// Shell type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellType {
    Bash,
    PowerShell,
}

/// Options for shell command execution.
#[derive(Debug, Clone, Default)]
pub struct ExecOptions {
    pub timeout: Option<u64>,
    pub prevent_cwd_changes: Option<bool>,
    pub should_use_sandbox: Option<bool>,
    pub should_auto_background: Option<bool>,
}

/// Check if a shell path is executable.
pub fn is_executable(shell_path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(shell_path) {
        Ok(meta) => meta.permissions().mode() & 0o111 != 0,
        Err(_) => {
            // Fallback: try to execute with --version
            std::process::Command::new(shell_path)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok()
        }
    }
}

/// Determine the best available shell to use.
pub async fn find_suitable_shell() -> Result<String, anyhow::Error> {
    // Check for explicit shell override
    if let Ok(shell_override) = std::env::var("MOSSEN_CODE_SHELL") {
        let is_supported = shell_override.contains("bash") || shell_override.contains("zsh");
        if is_supported && is_executable(&shell_override) {
            return Ok(shell_override);
        }
    }

    // Check user's preferred shell from environment
    let env_shell = std::env::var("SHELL").ok();
    let is_env_shell_supported = env_shell
        .as_ref()
        .map(|s| s.contains("bash") || s.contains("zsh"))
        .unwrap_or(false);
    let prefer_bash = env_shell.as_ref().is_some_and(|s| s.contains("bash"));

    // Try to locate shells
    let zsh_path = which("zsh").await;
    let bash_path = which("bash").await;

    // Build search list
    let shell_paths = ["/bin", "/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"];
    let shell_order: Vec<&str> = if prefer_bash {
        vec!["bash", "zsh"]
    } else {
        vec!["zsh", "bash"]
    };

    let mut supported_shells: Vec<String> = shell_order
        .iter()
        .flat_map(|shell| shell_paths.iter().map(move |p| format!("{}/{}", p, shell)))
        .collect();

    // Add discovered paths
    if prefer_bash {
        if let Some(ref p) = bash_path {
            supported_shells.insert(0, p.clone());
        }
        if let Some(ref p) = zsh_path {
            supported_shells.push(p.clone());
        }
    } else {
        if let Some(ref p) = zsh_path {
            supported_shells.insert(0, p.clone());
        }
        if let Some(ref p) = bash_path {
            supported_shells.push(p.clone());
        }
    }

    // Prioritize SHELL env variable
    if is_env_shell_supported {
        if let Some(ref shell) = env_shell {
            if is_executable(shell) {
                supported_shells.insert(0, shell.clone());
            }
        }
    }

    // Find first executable shell
    for shell in &supported_shells {
        if is_executable(shell) {
            return Ok(shell.clone());
        }
    }

    Err(anyhow!(
        "No suitable shell found. Mossen requires a Posix shell environment."
    ))
}

/// Execute a shell command.
pub async fn exec(
    command: &str,
    shell_path: &str,
    cwd: &Path,
    options: &ExecOptions,
) -> Result<ExecResult, anyhow::Error> {
    let timeout_ms = options.timeout.unwrap_or(DEFAULT_TIMEOUT_MS);

    let output = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        Command::new(shell_path)
            .args(["-c", command])
            .current_dir(cwd)
            .env("GIT_EDITOR", "true")
            .env("MOSSENCODE", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn shell: {}", e))?
            .wait_with_output(),
    )
    .await
    .map_err(|_| anyhow!("Command timed out after {}ms", timeout_ms))?
    .map_err(|e| anyhow!("Command execution failed: {}", e))?;

    Ok(ExecResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        code: output.status.code().unwrap_or(1),
        interrupted: false,
        background_task_id: None,
    })
}

/// Result of shell command execution.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
    pub interrupted: bool,
    pub background_task_id: Option<String>,
}

/// Set the current working directory (validates path exists).
pub fn set_cwd(path: &str, relative_to: Option<&str>) -> Result<PathBuf, anyhow::Error> {
    let resolved = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        let base = relative_to
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        base.join(path)
    };

    // Resolve symlinks
    let physical_path = std::fs::canonicalize(&resolved).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow!("Path \"{}\" does not exist", resolved.display())
        } else {
            anyhow!("Failed to resolve path: {}", e)
        }
    })?;

    Ok(physical_path)
}

/// Find a binary on PATH using `which`.
async fn which(name: &str) -> Option<String> {
    let output = Command::new("which")
        .arg(name)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            Some(path)
        } else {
            None
        }
    } else {
        None
    }
}

// =============================================================================
// 与 TS `shell/shellProvider.ts`、`shell/bashProvider.ts`、`shell/powershellProvider.ts`、
// `shell/shellToolUtils.ts` 对齐的导出。
// =============================================================================

/// 对应 TS `SHELL_TYPES`：受支持的 shell 字面量集合。
pub const SHELL_TYPES: &[&str] = &["bash", "powershell"];

/// 对应 TS `DEFAULT_HOOK_SHELL`。
pub const DEFAULT_HOOK_SHELL: &str = "bash";

/// 对应 TS `SHELL_TOOL_NAMES`：会用到 shell 执行的工具名集合。
pub const SHELL_TOOL_NAMES: &[&str] = &["bash", "powershell"];

/// 对应 TS `ShellProvider` trait（结构体形态）。Rust 端真实 provider 实现保留
/// 在各自 crate 中，这里仅给出 trait 别名作为契约说明。
pub trait ShellProviderTrait: Send + Sync {
    fn shell_type(&self) -> ShellType;
    fn shell_path(&self) -> &str;
    fn detached(&self) -> bool;
    fn build_exec_command(&self, command: &str) -> String;
    fn get_spawn_args(&self, command_string: &str) -> Vec<String>;
}

/// bash shell provider 工厂（对应 TS `createBashShellProvider`）。
pub struct BashShellProvider {
    shell_path: String,
}

impl Default for BashShellProvider {
    fn default() -> Self {
        Self {
            shell_path: "/bin/bash".to_string(),
        }
    }
}

impl ShellProviderTrait for BashShellProvider {
    fn shell_type(&self) -> ShellType {
        ShellType::Bash
    }
    fn shell_path(&self) -> &str {
        &self.shell_path
    }
    fn detached(&self) -> bool {
        false
    }
    fn build_exec_command(&self, command: &str) -> String {
        command.to_string()
    }
    fn get_spawn_args(&self, command_string: &str) -> Vec<String> {
        vec![
            "-c".to_string(),
            "-l".to_string(),
            command_string.to_string(),
        ]
    }
}

/// 创建 bash provider（对应 TS `createBashShellProvider`）。
pub fn create_bash_shell_provider() -> BashShellProvider {
    BashShellProvider::default()
}

/// PowerShell provider。
pub struct PowerShellProvider {
    shell_path: String,
}

impl Default for PowerShellProvider {
    fn default() -> Self {
        Self {
            shell_path: which::which("pwsh")
                .or_else(|_| which::which("powershell"))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "powershell".to_string()),
        }
    }
}

impl ShellProviderTrait for PowerShellProvider {
    fn shell_type(&self) -> ShellType {
        ShellType::PowerShell
    }
    fn shell_path(&self) -> &str {
        &self.shell_path
    }
    fn detached(&self) -> bool {
        false
    }
    fn build_exec_command(&self, command: &str) -> String {
        command.to_string()
    }
    fn get_spawn_args(&self, command_string: &str) -> Vec<String> {
        vec![
            "-NoProfile".to_string(),
            "-Command".to_string(),
            command_string.to_string(),
        ]
    }
}

/// 创建 PowerShell provider（对应 TS `createPowerShellProvider`）。
pub fn create_power_shell_provider() -> PowerShellProvider {
    PowerShellProvider::default()
}

static PWSH_CACHE: once_cell::sync::Lazy<std::sync::Mutex<Option<String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

/// 对应 TS `getPsProvider`：返回缓存的 PowerShell 可执行路径。
pub fn get_ps_provider() -> Option<String> {
    if let Some(p) = PWSH_CACHE.lock().unwrap().clone() {
        return Some(p);
    }
    let p = which::which("pwsh")
        .or_else(|_| which::which("powershell"))
        .ok()?
        .to_string_lossy()
        .to_string();
    *PWSH_CACHE.lock().unwrap() = Some(p.clone());
    Some(p)
}

/// 对应 TS `resetPowerShellCache`。
pub fn reset_power_shell_cache() {
    *PWSH_CACHE.lock().unwrap() = None;
}

/// 对应 TS `getShellConfig`：根据当前环境推断 shell 配置。
pub fn get_shell_config() -> ShellConfig {
    let path = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let shell_type = if path.ends_with("pwsh") || path.contains("powershell") {
        ShellType::PowerShell
    } else {
        ShellType::Bash
    };
    ShellConfig {
        shell_path: path,
        shell_type,
    }
}
