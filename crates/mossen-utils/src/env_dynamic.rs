use std::sync::Mutex;

use once_cell::sync::Lazy;
use tokio::fs;
use tokio::process::Command;

/// Known JetBrains IDEs.
pub const JETBRAINS_IDES: &[&str] = &[
    "idea", "pycharm", "webstorm", "phpstorm", "rustrover", "clion",
    "goland", "rider", "datagrip", "dataspell", "fleet",
];

/// Cached JetBrains IDE detection result.
static JETBRAINS_IDE_CACHE: Lazy<Mutex<Option<Option<String>>>> = Lazy::new(|| Mutex::new(None));

/// Runtime musl detection cache.
static MUSL_RUNTIME_CACHE: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));

/// Environment information.
#[derive(Debug, Clone)]
pub struct EnvInfo {
    pub platform: String,
    pub terminal: Option<String>,
}

/// Check if running in Docker.
pub async fn get_is_docker() -> bool {
    if std::env::consts::OS != "linux" {
        return false;
    }
    match Command::new("test")
        .args(["-f", "/.dockerenv"])
        .output()
        .await
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Check if running in a bubblewrap sandbox.
pub fn get_is_bubblewrap_sandbox() -> bool {
    std::env::consts::OS == "linux"
        && std::env::var("MOSSEN_CODE_BUBBLEWRAP")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
}

/// Check if the system is using MUSL libc.
pub fn is_musl_environment() -> bool {
    if std::env::consts::OS != "linux" {
        return false;
    }

    let cache = MUSL_RUNTIME_CACHE.lock().unwrap();
    cache.unwrap_or(false)
}

/// Initialize MUSL detection (call at startup on Linux).
pub async fn init_musl_detection() {
    if std::env::consts::OS != "linux" {
        return;
    }

    let musl_arch = if std::env::consts::ARCH == "x86_64" {
        "x86_64"
    } else {
        "aarch64"
    };

    let path = format!("/lib/libc.musl-{}.so.1", musl_arch);
    let is_musl = fs::metadata(&path).await.is_ok();

    let mut cache = MUSL_RUNTIME_CACHE.lock().unwrap();
    *cache = Some(is_musl);
}

/// Detect JetBrains IDE from parent process (async).
async fn detect_jetbrains_ide_from_parent_process_async() -> Option<String> {
    // Check cache first
    {
        let cache = JETBRAINS_IDE_CACHE.lock().unwrap();
        if let Some(ref result) = *cache {
            return result.clone();
        }
    }

    if std::env::consts::OS == "macos" {
        let mut cache = JETBRAINS_IDE_CACHE.lock().unwrap();
        *cache = Some(None);
        return None;
    }

    // Try to get ancestor commands
    let result = match get_ancestor_commands_async(std::process::id(), 10).await {
        Ok(commands) => {
            let mut found = None;
            for command in &commands {
                let lower = command.to_lowercase();
                for ide in JETBRAINS_IDES {
                    if lower.contains(ide) {
                        found = Some(ide.to_string());
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            found
        }
        Err(_) => None,
    };

    let mut cache = JETBRAINS_IDE_CACHE.lock().unwrap();
    *cache = Some(result.clone());
    result
}

/// Get terminal with JetBrains detection (async).
pub async fn get_terminal_with_jetbrains_detection_async(
    env_info: &EnvInfo,
) -> Option<String> {
    if std::env::var("TERMINAL_EMULATOR")
        .map(|v| v == "JetBrains-JediTerm")
        .unwrap_or(false)
    {
        if env_info.platform != "darwin" {
            let specific_ide = detect_jetbrains_ide_from_parent_process_async().await;
            return Some(specific_ide.unwrap_or_else(|| "pycharm".to_string()));
        }
    }
    env_info.terminal.clone()
}

/// Get terminal with JetBrains detection (sync, uses cache).
pub fn get_terminal_with_jetbrains_detection(env_info: &EnvInfo) -> Option<String> {
    if std::env::var("TERMINAL_EMULATOR")
        .map(|v| v == "JetBrains-JediTerm")
        .unwrap_or(false)
    {
        if env_info.platform != "darwin" {
            let cache = JETBRAINS_IDE_CACHE.lock().unwrap();
            if let Some(ref result) = *cache {
                return Some(result.clone().unwrap_or_else(|| "pycharm".to_string()));
            }
            return Some("pycharm".to_string());
        }
    }
    env_info.terminal.clone()
}

/// Initialize JetBrains IDE detection asynchronously.
pub async fn init_jetbrains_detection() {
    if std::env::var("TERMINAL_EMULATOR")
        .map(|v| v == "JetBrains-JediTerm")
        .unwrap_or(false)
    {
        let _ = detect_jetbrains_ide_from_parent_process_async().await;
    }
}

/// Get ancestor commands for a given PID (async, cross-platform).
async fn get_ancestor_commands_async(
    pid: u32,
    max_depth: usize,
) -> Result<Vec<String>, std::io::Error> {
    let mut commands = Vec::new();
    let mut current_pid = pid;

    for _ in 0..max_depth {
        #[cfg(target_os = "linux")]
        {
            let cmdline_path = format!("/proc/{}/cmdline", current_pid);
            match fs::read_to_string(&cmdline_path).await {
                Ok(cmdline) => {
                    let cmd = cmdline.replace('\0', " ").trim().to_string();
                    if !cmd.is_empty() {
                        commands.push(cmd);
                    }
                }
                Err(_) => break,
            }

            // Get parent PID
            let stat_path = format!("/proc/{}/stat", current_pid);
            match fs::read_to_string(&stat_path).await {
                Ok(stat) => {
                    // Format: pid (comm) state ppid ...
                    let parts: Vec<&str> = stat.split(')').last().unwrap_or("").split_whitespace().collect();
                    if parts.len() >= 2 {
                        current_pid = parts[1].parse().unwrap_or(0);
                        if current_pid <= 1 {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        #[cfg(target_os = "macos")]
        {
            let output = Command::new("ps")
                .args(["-o", "ppid=,command=", "-p", &current_pid.to_string()])
                .output()
                .await;

            match output {
                Ok(out) if out.status.success() => {
                    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
                    if parts.len() == 2 {
                        commands.push(parts[1].trim().to_string());
                        current_pid = parts[0].trim().parse().unwrap_or(0);
                        if current_pid <= 1 {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            break;
        }
    }

    Ok(commands)
}
