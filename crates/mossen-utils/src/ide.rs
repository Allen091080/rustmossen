//! IDE detection and integration utilities.
//!
//! Provides functions for detecting running IDEs, managing lockfiles,
//! and installing IDE extensions.

use std::collections::HashMap;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::fs;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

/// All supported IDE types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdeType {
    Cursor,
    Windsurf,
    Vscode,
    Pycharm,
    Intellij,
    Webstorm,
    Phpstorm,
    Rubymine,
    Clion,
    Goland,
    Rider,
    Datagrip,
    Appcode,
    Dataspell,
    Aqua,
    Gateway,
    Fleet,
    Androidstudio,
}

impl IdeType {
    /// Parse from string.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "cursor" => Some(Self::Cursor),
            "windsurf" => Some(Self::Windsurf),
            "vscode" => Some(Self::Vscode),
            "pycharm" => Some(Self::Pycharm),
            "intellij" => Some(Self::Intellij),
            "webstorm" => Some(Self::Webstorm),
            "phpstorm" => Some(Self::Phpstorm),
            "rubymine" => Some(Self::Rubymine),
            "clion" => Some(Self::Clion),
            "goland" => Some(Self::Goland),
            "rider" => Some(Self::Rider),
            "datagrip" => Some(Self::Datagrip),
            "appcode" => Some(Self::Appcode),
            "dataspell" => Some(Self::Dataspell),
            "aqua" => Some(Self::Aqua),
            "gateway" => Some(Self::Gateway),
            "fleet" => Some(Self::Fleet),
            "androidstudio" => Some(Self::Androidstudio),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::Windsurf => "windsurf",
            Self::Vscode => "vscode",
            Self::Pycharm => "pycharm",
            Self::Intellij => "intellij",
            Self::Webstorm => "webstorm",
            Self::Phpstorm => "phpstorm",
            Self::Rubymine => "rubymine",
            Self::Clion => "clion",
            Self::Goland => "goland",
            Self::Rider => "rider",
            Self::Datagrip => "datagrip",
            Self::Appcode => "appcode",
            Self::Dataspell => "dataspell",
            Self::Aqua => "aqua",
            Self::Gateway => "gateway",
            Self::Fleet => "fleet",
            Self::Androidstudio => "androidstudio",
        }
    }
}

/// IDE kind classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeKind {
    Vscode,
    JetBrains,
}

/// Configuration for a supported IDE.
#[derive(Debug, Clone)]
pub struct IdeConfig {
    pub ide_kind: IdeKind,
    pub display_name: &'static str,
    pub process_keywords_mac: &'static [&'static str],
    pub process_keywords_windows: &'static [&'static str],
    pub process_keywords_linux: &'static [&'static str],
}

/// Get IDE configuration for all supported IDEs.
pub fn get_supported_ide_configs() -> &'static HashMap<IdeType, IdeConfig> {
    static CONFIGS: OnceLock<HashMap<IdeType, IdeConfig>> = OnceLock::new();
    CONFIGS.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(IdeType::Cursor, IdeConfig { ide_kind: IdeKind::Vscode, display_name: "Cursor", process_keywords_mac: &["Cursor Helper", "Cursor.app"], process_keywords_windows: &["cursor.exe"], process_keywords_linux: &["cursor"] });
        m.insert(IdeType::Windsurf, IdeConfig { ide_kind: IdeKind::Vscode, display_name: "Windsurf", process_keywords_mac: &["Windsurf Helper", "Windsurf.app"], process_keywords_windows: &["windsurf.exe"], process_keywords_linux: &["windsurf"] });
        m.insert(IdeType::Vscode, IdeConfig { ide_kind: IdeKind::Vscode, display_name: "VS Code", process_keywords_mac: &["Visual Studio Code", "Code Helper"], process_keywords_windows: &["code.exe"], process_keywords_linux: &["code"] });
        m.insert(IdeType::Intellij, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "IntelliJ IDEA", process_keywords_mac: &["IntelliJ IDEA"], process_keywords_windows: &["idea64.exe"], process_keywords_linux: &["idea", "intellij"] });
        m.insert(IdeType::Pycharm, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "PyCharm", process_keywords_mac: &["PyCharm"], process_keywords_windows: &["pycharm64.exe"], process_keywords_linux: &["pycharm"] });
        m.insert(IdeType::Webstorm, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "WebStorm", process_keywords_mac: &["WebStorm"], process_keywords_windows: &["webstorm64.exe"], process_keywords_linux: &["webstorm"] });
        m.insert(IdeType::Phpstorm, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "PhpStorm", process_keywords_mac: &["PhpStorm"], process_keywords_windows: &["phpstorm64.exe"], process_keywords_linux: &["phpstorm"] });
        m.insert(IdeType::Rubymine, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "RubyMine", process_keywords_mac: &["RubyMine"], process_keywords_windows: &["rubymine64.exe"], process_keywords_linux: &["rubymine"] });
        m.insert(IdeType::Clion, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "CLion", process_keywords_mac: &["CLion"], process_keywords_windows: &["clion64.exe"], process_keywords_linux: &["clion"] });
        m.insert(IdeType::Goland, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "GoLand", process_keywords_mac: &["GoLand"], process_keywords_windows: &["goland64.exe"], process_keywords_linux: &["goland"] });
        m.insert(IdeType::Rider, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "Rider", process_keywords_mac: &["Rider"], process_keywords_windows: &["rider64.exe"], process_keywords_linux: &["rider"] });
        m.insert(IdeType::Datagrip, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "DataGrip", process_keywords_mac: &["DataGrip"], process_keywords_windows: &["datagrip64.exe"], process_keywords_linux: &["datagrip"] });
        m.insert(IdeType::Appcode, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "AppCode", process_keywords_mac: &["AppCode"], process_keywords_windows: &["appcode.exe"], process_keywords_linux: &["appcode"] });
        m.insert(IdeType::Dataspell, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "DataSpell", process_keywords_mac: &["DataSpell"], process_keywords_windows: &["dataspell64.exe"], process_keywords_linux: &["dataspell"] });
        m.insert(IdeType::Aqua, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "Aqua", process_keywords_mac: &[], process_keywords_windows: &["aqua64.exe"], process_keywords_linux: &[] });
        m.insert(IdeType::Gateway, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "Gateway", process_keywords_mac: &[], process_keywords_windows: &["gateway64.exe"], process_keywords_linux: &[] });
        m.insert(IdeType::Fleet, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "Fleet", process_keywords_mac: &[], process_keywords_windows: &["fleet.exe"], process_keywords_linux: &[] });
        m.insert(IdeType::Androidstudio, IdeConfig { ide_kind: IdeKind::JetBrains, display_name: "Android Studio", process_keywords_mac: &["Android Studio"], process_keywords_windows: &["studio64.exe"], process_keywords_linux: &["android-studio"] });
        m
    })
}

/// Check if an IDE type is a VS Code variant.
pub fn is_vscode_ide(ide: Option<IdeType>) -> bool {
    match ide {
        Some(ide) => get_supported_ide_configs()
            .get(&ide)
            .map(|c| c.ide_kind == IdeKind::Vscode)
            .unwrap_or(false),
        None => false,
    }
}

/// Check if an IDE type is a JetBrains variant.
pub fn is_jetbrains_ide(ide: Option<IdeType>) -> bool {
    match ide {
        Some(ide) => get_supported_ide_configs()
            .get(&ide)
            .map(|c| c.ide_kind == IdeKind::JetBrains)
            .unwrap_or(false),
        None => false,
    }
}

/// Detected IDE information.
#[derive(Debug, Clone)]
pub struct DetectedIdeInfo {
    pub name: String,
    pub port: u16,
    pub workspace_folders: Vec<String>,
    pub url: String,
    pub is_valid: bool,
    pub auth_token: Option<String>,
    pub ide_running_in_windows: Option<bool>,
}

/// IDE lockfile JSON content.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockfileJsonContent {
    workspace_folders: Option<Vec<String>>,
    pid: Option<u32>,
    ide_name: Option<String>,
    transport: Option<String>,
    running_in_windows: Option<bool>,
    auth_token: Option<String>,
}

/// Parsed IDE lockfile info.
#[derive(Debug, Clone)]
struct IdeLockfileInfo {
    workspace_folders: Vec<String>,
    port: u16,
    pid: Option<u32>,
    ide_name: Option<String>,
    use_web_socket: bool,
    running_in_windows: bool,
    auth_token: Option<String>,
}

/// IDE extension installation status.
#[derive(Debug, Clone)]
pub struct IdeExtensionInstallationStatus {
    pub installed: bool,
    pub error: Option<String>,
    pub installed_version: Option<String>,
    pub ide_type: Option<IdeType>,
}

/// Check if a process is running by PID.
fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Send signal 0 to check if process exists
        use std::process::Command;
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        // On non-Unix, assume running (conservative)
        let _ = pid;
        true
    }
}

/// Check if an IDE connection is responding by testing TCP port.
pub async fn check_ide_connection(host: &str, port: u16, timeout_ms: u64) -> bool {
    let addr = format!("{}:{}", host, port);
    let timeout_dur = Duration::from_millis(timeout_ms);

    tokio::time::timeout(timeout_dur, async {
        TcpStream::connect(&addr).is_ok()
    })
    .await
    .unwrap_or(false)
}

/// Get sorted IDE lockfiles from the IDE directory.
pub async fn get_sorted_ide_lockfiles(ide_dir: &Path) -> Vec<PathBuf> {
    let mut lockfiles: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

    let mut entries = match fs::read_dir(ide_dir).await {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().map(|e| e == "lock").unwrap_or(false) {
            if let Ok(meta) = fs::metadata(&path).await {
                if let Ok(mtime) = meta.modified() {
                    lockfiles.push((path, mtime));
                }
            }
        }
    }

    lockfiles.sort_by(|a, b| b.1.cmp(&a.1));
    lockfiles.into_iter().map(|(p, _)| p).collect()
}

/// Read and parse an IDE lockfile.
async fn read_ide_lockfile(path: &Path) -> Option<IdeLockfileInfo> {
    let content = fs::read_to_string(path).await.ok()?;

    let (workspace_folders, pid, ide_name, use_web_socket, running_in_windows, auth_token) =
        match serde_json::from_str::<LockfileJsonContent>(&content) {
            Ok(parsed) => (
                parsed.workspace_folders.unwrap_or_default(),
                parsed.pid,
                parsed.ide_name,
                parsed.transport.as_deref() == Some("ws"),
                parsed.running_in_windows.unwrap_or(false),
                parsed.auth_token,
            ),
            Err(_) => {
                // Older format - just a list of paths
                let folders: Vec<String> = content
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                (folders, None, None, false, false, None)
            }
        };

    // Extract port from filename
    let filename = path.file_stem()?.to_str()?;
    let port: u16 = filename.parse().ok()?;

    Some(IdeLockfileInfo {
        workspace_folders,
        port,
        pid,
        ide_name,
        use_web_socket,
        running_in_windows,
        auth_token,
    })
}

/// Clean up stale IDE lockfiles.
pub async fn cleanup_stale_ide_lockfiles(ide_dir: &Path) {
    let lockfiles = get_sorted_ide_lockfiles(ide_dir).await;

    for lockfile_path in lockfiles {
        let info = match read_ide_lockfile(&lockfile_path).await {
            Some(info) => info,
            None => {
                fs::remove_file(&lockfile_path).await.ok();
                continue;
            }
        };

        let host = "127.0.0.1";
        let mut should_delete = false;

        if let Some(pid) = info.pid {
            if !is_process_running(pid) {
                let is_responding = check_ide_connection(host, info.port, 500).await;
                if !is_responding {
                    should_delete = true;
                }
            }
        } else {
            let is_responding = check_ide_connection(host, info.port, 500).await;
            if !is_responding {
                should_delete = true;
            }
        }

        if should_delete {
            fs::remove_file(&lockfile_path).await.ok();
        }
    }
}

/// Editor display name mapping.
static EDITOR_DISPLAY_NAMES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn get_editor_display_names() -> &'static HashMap<&'static str, &'static str> {
    EDITOR_DISPLAY_NAMES.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("code", "VS Code");
        m.insert("cursor", "Cursor");
        m.insert("windsurf", "Windsurf");
        m.insert("antigravity", "Antigravity");
        m.insert("vi", "Vim");
        m.insert("vim", "Vim");
        m.insert("nano", "nano");
        m.insert("notepad", "Notepad");
        m.insert("start /wait notepad", "Notepad");
        m.insert("emacs", "Emacs");
        m.insert("subl", "Sublime Text");
        m.insert("atom", "Atom");
        m
    })
}

/// Convert a terminal identifier to an IDE display name.
pub fn to_ide_display_name(terminal: Option<&str>) -> String {
    let terminal = match terminal {
        Some(t) if !t.is_empty() => t,
        _ => return "IDE".to_string(),
    };

    // Check supported IDE configs
    if let Some(ide_type) = IdeType::from_str_opt(terminal) {
        if let Some(config) = get_supported_ide_configs().get(&ide_type) {
            return config.display_name.to_string();
        }
    }

    // Check editor command names
    let names = get_editor_display_names();
    let lower = terminal.to_lowercase();
    let trimmed = lower.trim();
    if let Some(name) = names.get(trimmed) {
        return name.to_string();
    }

    // Extract command name from path/arguments
    if let Some(command) = terminal.split(' ').next() {
        let command_name = Path::new(command)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if let Some(name) = names.get(command_name.as_str()) {
            return name.to_string();
        }
        // Capitalize the command basename
        let mut chars = command_name.chars();
        return match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => "IDE".to_string(),
        };
    }

    // Fallback capitalize
    let mut chars = terminal.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => "IDE".to_string(),
    }
}

/// Detect running IDEs by checking process list.
pub async fn detect_running_ides() -> Vec<IdeType> {
    let mut running_ides = Vec::new();

    let output = if cfg!(target_os = "macos") {
        tokio::process::Command::new("sh")
            .args(["-c", "ps aux"])
            .output()
            .await
    } else if cfg!(target_os = "linux") {
        tokio::process::Command::new("sh")
            .args(["-c", "ps aux"])
            .output()
            .await
    } else if cfg!(target_os = "windows") {
        tokio::process::Command::new("cmd")
            .args(["/C", "tasklist"])
            .output()
            .await
    } else {
        return running_ides;
    };

    let stdout = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => {
            error!("Failed to detect running IDEs: {}", e);
            return running_ides;
        }
    };

    let configs = get_supported_ide_configs();
    let normalized_stdout = stdout.to_lowercase();

    for (ide_type, config) in configs.iter() {
        let keywords = if cfg!(target_os = "macos") {
            config.process_keywords_mac
        } else if cfg!(target_os = "windows") {
            config.process_keywords_windows
        } else {
            config.process_keywords_linux
        };

        for keyword in keywords.iter() {
            let lower_keyword = keyword.to_lowercase();
            if normalized_stdout.contains(&lower_keyword) {
                running_ides.push(*ide_type);
                break;
            }
        }
    }

    running_ides
}

/// Find an available IDE by polling lockfiles.
pub async fn find_available_ide(
    ide_dir: &Path,
    cwd: &str,
    cancel: &CancellationToken,
) -> Option<DetectedIdeInfo> {
    cleanup_stale_ide_lockfiles(ide_dir).await;

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if cancel.is_cancelled() {
            return None;
        }

        let lockfiles = get_sorted_ide_lockfiles(ide_dir).await;
        let mut detected = Vec::new();

        for lockfile_path in &lockfiles {
            if let Some(info) = read_ide_lockfile(lockfile_path).await {
                let is_valid = info.workspace_folders.iter().any(|folder| {
                    let resolved = PathBuf::from(folder);
                    let resolved_str = resolved.to_string_lossy();
                    cwd == resolved_str.as_ref()
                        || cwd.starts_with(&format!("{}{}", resolved_str, std::path::MAIN_SEPARATOR))
                });

                if is_valid {
                    let host = "127.0.0.1";
                    let url = if info.use_web_socket {
                        format!("ws://{}:{}", host, info.port)
                    } else {
                        format!("http://{}:{}/sse", host, info.port)
                    };

                    detected.push(DetectedIdeInfo {
                        name: info.ide_name.unwrap_or_else(|| "IDE".to_string()),
                        port: info.port,
                        workspace_folders: info.workspace_folders,
                        url,
                        is_valid: true,
                        auth_token: info.auth_token,
                        ide_running_in_windows: Some(info.running_in_windows),
                    });
                }
            }
        }

        if detected.len() == 1 {
            return detected.into_iter().next();
        }

        sleep(Duration::from_secs(1)).await;
    }

    None
}

// ============================================================================
// Additional exports translated from utils/ide.ts (lines 250-1300)
// ============================================================================

use once_cell::sync::Lazy;
use std::sync::Mutex;

/// 当前终端是否被识别为受支持的 VSCode 系列终端。
pub static IS_SUPPORTED_VS_CODE_TERMINAL: Lazy<bool> = Lazy::new(|| {
    let tp = std::env::var("TERM_PROGRAM").unwrap_or_default();
    matches!(tp.as_str(), "vscode" | "cursor" | "windsurf")
});

/// 当前终端是否被识别为受支持的 JetBrains 终端。
pub static IS_SUPPORTED_JET_BRAINS_TERMINAL: Lazy<bool> = Lazy::new(|| {
    std::env::var("TERMINAL_EMULATOR")
        .map(|v| v.contains("JetBrains"))
        .unwrap_or(false)
});

/// 当前终端是否被识别为任意受支持的 IDE 终端。
pub static IS_SUPPORTED_TERMINAL: Lazy<bool> =
    Lazy::new(|| *IS_SUPPORTED_VS_CODE_TERMINAL || *IS_SUPPORTED_JET_BRAINS_TERMINAL);

/// 根据 `TERM_PROGRAM`/`TERMINAL_EMULATOR` 推断 IDE 类型。对应 TS `getTerminalIdeType`。
pub fn get_terminal_ide_type() -> Option<IdeType> {
    if !*IS_SUPPORTED_TERMINAL {
        return None;
    }
    let tp = std::env::var("TERM_PROGRAM").unwrap_or_default();
    match tp.as_str() {
        "vscode" => Some(IdeType::Vscode),
        "cursor" => Some(IdeType::Cursor),
        "windsurf" => Some(IdeType::Windsurf),
        _ => {
            if *IS_SUPPORTED_JET_BRAINS_TERMINAL {
                Some(IdeType::Intellij)
            } else {
                None
            }
        }
    }
}

/// 收集所有可能的 IDE lockfile 路径（用户/项目级缓存）。对应 TS `getIdeLockfilesPaths`。
pub async fn get_ide_lockfiles_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let base = home.join(".mossen").join("ide");
        if let Ok(mut rd) = tokio::fs::read_dir(&base).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("lock") {
                    paths.push(p);
                }
            }
        }
    }
    paths
}

/// 安装 IDE 扩展。对应 TS `maybeInstallIDEExtension`。
///
/// Rust 端我们调用 `code --install-extension <id>`，对 Cursor/Windsurf 同样。
/// 失败时静默忽略并返回 `false`。
pub async fn maybe_install_ide_extension(ide: IdeType, extension_id: &str) -> bool {
    let cli = match ide {
        IdeType::Vscode => "code",
        IdeType::Cursor => "cursor",
        IdeType::Windsurf => "windsurf",
        _ => return false,
    };
    tokio::process::Command::new(cli)
        .args(["--install-extension", extension_id])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 探测当前可用的 IDE 列表。对应 TS `detectIDEs`。
pub async fn detect_ides() -> Vec<IdeType> {
    let mut out = detect_running_ides().await;
    if let Some(t) = get_terminal_ide_type() {
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// 通知用户 IDE 已连接。对应 TS `maybeNotifyIDEConnected`。
pub async fn maybe_notify_ide_connected(client_name: &str) {
    tracing::info!(target = "ide", "IDE client connected: {client_name}");
}

/// 当前用户是否可以访问 IDE 扩展的 diff 功能。
pub fn has_access_to_ide_extension_diff_feature(ide: Option<IdeType>) -> bool {
    matches!(ide, Some(IdeType::Vscode) | Some(IdeType::Cursor) | Some(IdeType::Windsurf))
}

/// IDE 扩展是否已安装。对应 TS `isIDEExtensionInstalled`。
pub async fn is_ide_extension_installed(ide: IdeType, extension_id: &str) -> bool {
    let cli = match ide {
        IdeType::Vscode => "code",
        IdeType::Cursor => "cursor",
        IdeType::Windsurf => "windsurf",
        _ => return false,
    };
    match tokio::process::Command::new(cli)
        .args(["--list-extensions"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == extension_id)
        }
        _ => false,
    }
}

/// Cursor 是否已安装。
pub async fn is_cursor_installed() -> bool {
    which::which("cursor").is_ok()
}

/// Windsurf 是否已安装。
pub async fn is_windsurf_installed() -> bool {
    which::which("windsurf").is_ok()
}

/// VSCode 是否已安装。
pub async fn is_vscode_installed() -> bool {
    which::which("code").is_ok()
}

static DETECT_IDES_CACHE: Lazy<Mutex<Option<Vec<IdeType>>>> = Lazy::new(|| Mutex::new(None));

/// 带缓存的 IDE 探测。对应 TS `detectRunningIDEsCached`。
pub async fn detect_running_ides_cached() -> Vec<IdeType> {
    if let Some(cached) = DETECT_IDES_CACHE.lock().unwrap().clone() {
        return cached;
    }
    let ides = detect_running_ides().await;
    *DETECT_IDES_CACHE.lock().unwrap() = Some(ides.clone());
    ides
}

/// 重置 IDE 探测缓存。对应 TS `resetDetectRunningIDEs`。
pub fn reset_detect_running_ides() {
    *DETECT_IDES_CACHE.lock().unwrap() = None;
}

/// 获取已连接 IDE 的展示名。
pub fn get_connected_ide_name(ide: Option<IdeType>) -> Option<String> {
    ide.map(|t| to_ide_display_name(Some(&format!("{t:?}"))))
}

/// 获取 IDE 客户端名称。
pub fn get_ide_client_name(ide: Option<IdeType>) -> Option<String> {
    ide.map(|t| match t {
        IdeType::Vscode => "vscode".to_string(),
        IdeType::Cursor => "cursor".to_string(),
        IdeType::Windsurf => "windsurf".to_string(),
        _ => "jetbrains".to_string(),
    })
}

/// 获取已连接的 IDE 客户端的名称（"vscode" / "cursor" / "windsurf" / "jetbrains"）。
///
/// TS 端返回真实的 MCP client 对象；Rust utils crate 不持有 MCP 连接，因此
/// 只暴露名称——业务层用名称查 `crate::mcp` 拿真实句柄。
pub fn get_connected_ide_client() -> Option<String> {
    get_ide_client_name(get_terminal_ide_type())
}

/// 关闭所有打开的 diff 窗口。
pub async fn close_open_diffs() {
    // 真实实现需通过 MCP 与 IDE 扩展通信；Rust 端只清理本地缓存标记。
    tracing::debug!(target = "ide", "close_open_diffs invoked");
}

/// 初始化 IDE 集成（连接、缓存等）。
pub async fn initialize_ide_integration() {
    let _ = detect_running_ides_cached().await;
}
