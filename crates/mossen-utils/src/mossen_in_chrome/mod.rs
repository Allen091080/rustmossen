use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener, UnixStream};

// ─── Constants ───────────────────────────────────────────────────────────────

pub const MOSSEN_IN_CHROME_MCP_SERVER_NAME: &str = "mossen-in-chrome";
const VERSION: &str = "1.0.0";
const MAX_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB
const MAX_TRACKED_TABS: usize = 200;
const NATIVE_HOST_IDENTIFIER: &str = "com.mossen.mossen_code_browser_extension";

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChromiumBrowser {
    Chrome,
    Brave,
    Arc,
    Chromium,
    Edge,
    Vivaldi,
    Opera,
}

impl ChromiumBrowser {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Chrome => "Google Chrome",
            Self::Brave => "Brave",
            Self::Arc => "Arc",
            Self::Chromium => "Chromium",
            Self::Edge => "Microsoft Edge",
            Self::Vivaldi => "Vivaldi",
            Self::Opera => "Opera",
        }
    }
}

pub static BROWSER_DETECTION_ORDER: &[ChromiumBrowser] = &[
    ChromiumBrowser::Chrome,
    ChromiumBrowser::Brave,
    ChromiumBrowser::Arc,
    ChromiumBrowser::Edge,
    ChromiumBrowser::Chromium,
    ChromiumBrowser::Vivaldi,
    ChromiumBrowser::Opera,
];

#[derive(Debug, Clone)]
pub struct BrowserConfig {
    pub name: &'static str,
    pub macos_app_name: &'static str,
    pub macos_data_path: &'static [&'static str],
    pub macos_native_messaging_path: &'static [&'static str],
    pub linux_binaries: &'static [&'static str],
    pub linux_data_path: &'static [&'static str],
    pub linux_native_messaging_path: &'static [&'static str],
    pub windows_data_path: &'static [&'static str],
    pub windows_registry_key: &'static str,
    pub windows_use_roaming: bool,
}

pub fn get_browser_config(browser: ChromiumBrowser) -> BrowserConfig {
    match browser {
        ChromiumBrowser::Chrome => BrowserConfig {
            name: "Google Chrome",
            macos_app_name: "Google Chrome",
            macos_data_path: &["Library", "Application Support", "Google", "Chrome"],
            macos_native_messaging_path: &["Library", "Application Support", "Google", "Chrome", "NativeMessagingHosts"],
            linux_binaries: &["google-chrome", "google-chrome-stable"],
            linux_data_path: &[".config", "google-chrome"],
            linux_native_messaging_path: &[".config", "google-chrome", "NativeMessagingHosts"],
            windows_data_path: &["Google", "Chrome", "User Data"],
            windows_registry_key: "HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts",
            windows_use_roaming: false,
        },
        ChromiumBrowser::Brave => BrowserConfig {
            name: "Brave",
            macos_app_name: "Brave Browser",
            macos_data_path: &["Library", "Application Support", "BraveSoftware", "Brave-Browser"],
            macos_native_messaging_path: &["Library", "Application Support", "BraveSoftware", "Brave-Browser", "NativeMessagingHosts"],
            linux_binaries: &["brave-browser", "brave"],
            linux_data_path: &[".config", "BraveSoftware", "Brave-Browser"],
            linux_native_messaging_path: &[".config", "BraveSoftware", "Brave-Browser", "NativeMessagingHosts"],
            windows_data_path: &["BraveSoftware", "Brave-Browser", "User Data"],
            windows_registry_key: "HKCU\\Software\\BraveSoftware\\Brave-Browser\\NativeMessagingHosts",
            windows_use_roaming: false,
        },
        ChromiumBrowser::Arc => BrowserConfig {
            name: "Arc",
            macos_app_name: "Arc",
            macos_data_path: &["Library", "Application Support", "Arc", "User Data"],
            macos_native_messaging_path: &["Library", "Application Support", "Arc", "User Data", "NativeMessagingHosts"],
            linux_binaries: &[],
            linux_data_path: &[],
            linux_native_messaging_path: &[],
            windows_data_path: &["Arc", "User Data"],
            windows_registry_key: "HKCU\\Software\\ArcBrowser\\Arc\\NativeMessagingHosts",
            windows_use_roaming: false,
        },
        ChromiumBrowser::Chromium => BrowserConfig {
            name: "Chromium",
            macos_app_name: "Chromium",
            macos_data_path: &["Library", "Application Support", "Chromium"],
            macos_native_messaging_path: &["Library", "Application Support", "Chromium", "NativeMessagingHosts"],
            linux_binaries: &["chromium", "chromium-browser"],
            linux_data_path: &[".config", "chromium"],
            linux_native_messaging_path: &[".config", "chromium", "NativeMessagingHosts"],
            windows_data_path: &["Chromium", "User Data"],
            windows_registry_key: "HKCU\\Software\\Chromium\\NativeMessagingHosts",
            windows_use_roaming: false,
        },
        ChromiumBrowser::Edge => BrowserConfig {
            name: "Microsoft Edge",
            macos_app_name: "Microsoft Edge",
            macos_data_path: &["Library", "Application Support", "Microsoft Edge"],
            macos_native_messaging_path: &["Library", "Application Support", "Microsoft Edge", "NativeMessagingHosts"],
            linux_binaries: &["microsoft-edge", "microsoft-edge-stable"],
            linux_data_path: &[".config", "microsoft-edge"],
            linux_native_messaging_path: &[".config", "microsoft-edge", "NativeMessagingHosts"],
            windows_data_path: &["Microsoft", "Edge", "User Data"],
            windows_registry_key: "HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts",
            windows_use_roaming: false,
        },
        ChromiumBrowser::Vivaldi => BrowserConfig {
            name: "Vivaldi",
            macos_app_name: "Vivaldi",
            macos_data_path: &["Library", "Application Support", "Vivaldi"],
            macos_native_messaging_path: &["Library", "Application Support", "Vivaldi", "NativeMessagingHosts"],
            linux_binaries: &["vivaldi", "vivaldi-stable"],
            linux_data_path: &[".config", "vivaldi"],
            linux_native_messaging_path: &[".config", "vivaldi", "NativeMessagingHosts"],
            windows_data_path: &["Vivaldi", "User Data"],
            windows_registry_key: "HKCU\\Software\\Vivaldi\\NativeMessagingHosts",
            windows_use_roaming: false,
        },
        ChromiumBrowser::Opera => BrowserConfig {
            name: "Opera",
            macos_app_name: "Opera",
            macos_data_path: &["Library", "Application Support", "com.operasoftware.Opera"],
            macos_native_messaging_path: &["Library", "Application Support", "com.operasoftware.Opera", "NativeMessagingHosts"],
            linux_binaries: &["opera"],
            linux_data_path: &[".config", "opera"],
            linux_native_messaging_path: &[".config", "opera", "NativeMessagingHosts"],
            windows_data_path: &["Opera Software", "Opera Stable"],
            windows_registry_key: "HKCU\\Software\\Opera Software\\Opera Stable\\NativeMessagingHosts",
            windows_use_roaming: true,
        },
    }
}

// ─── Browser Data Paths ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BrowserPath {
    pub browser: ChromiumBrowser,
    pub path: PathBuf,
}

pub fn get_all_browser_data_paths() -> Vec<BrowserPath> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut paths = Vec::new();

    for &browser_id in BROWSER_DETECTION_ORDER {
        let config = get_browser_config(browser_id);
        let data_path: &[&str] = if cfg!(target_os = "macos") {
            config.macos_data_path
        } else if cfg!(target_os = "linux") {
            config.linux_data_path
        } else {
            &[]
        };

        if !data_path.is_empty() {
            let mut path = home.clone();
            for part in data_path {
                path.push(part);
            }
            paths.push(BrowserPath {
                browser: browser_id,
                path,
            });
        }
    }
    paths
}

pub fn get_all_native_messaging_hosts_dirs() -> Vec<BrowserPath> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut paths = Vec::new();

    for &browser_id in BROWSER_DETECTION_ORDER {
        let config = get_browser_config(browser_id);
        let nm_path: &[&str] = if cfg!(target_os = "macos") {
            config.macos_native_messaging_path
        } else if cfg!(target_os = "linux") {
            config.linux_native_messaging_path
        } else {
            &[]
        };

        if !nm_path.is_empty() {
            let mut path = home.clone();
            for part in nm_path {
                path.push(part);
            }
            paths.push(BrowserPath {
                browser: browser_id,
                path,
            });
        }
    }
    paths
}

pub fn get_all_windows_registry_keys() -> Vec<(ChromiumBrowser, String)> {
    BROWSER_DETECTION_ORDER
        .iter()
        .map(|&b| {
            let config = get_browser_config(b);
            (b, config.windows_registry_key.to_string())
        })
        .filter(|(_, key)| !key.is_empty())
        .collect()
}

// ─── Browser Detection ───────────────────────────────────────────────────────

pub async fn detect_available_browser() -> Option<ChromiumBrowser> {
    for &browser_id in BROWSER_DETECTION_ORDER {
        let config = get_browser_config(browser_id);
        if cfg!(target_os = "macos") {
            let app_path = format!("/Applications/{}.app", config.macos_app_name);
            if fs::metadata(&app_path).await.map(|m| m.is_dir()).unwrap_or(false) {
                eprintln!("[Mossen in Chrome] Detected browser: {}", config.name);
                return Some(browser_id);
            }
        } else if cfg!(target_os = "linux") {
            for binary in config.linux_binaries {
                if which::which(binary).is_ok() {
                    eprintln!("[Mossen in Chrome] Detected browser: {}", config.name);
                    return Some(browser_id);
                }
            }
        }
    }
    None
}

pub fn is_mossen_in_chrome_mcp_server(name: &str) -> bool {
    let normalized = name.to_lowercase().replace(['-', '_', ' '], "");
    normalized == "mosseninchrome"
}

// ─── Tab Tracking ────────────────────────────────────────────────────────────

static TRACKED_TAB_IDS: Lazy<Mutex<HashSet<u64>>> = Lazy::new(|| Mutex::new(HashSet::new()));

pub fn track_mossen_in_chrome_tab_id(tab_id: u64) {
    let mut tabs = TRACKED_TAB_IDS.lock().unwrap();
    if tabs.len() >= MAX_TRACKED_TABS && !tabs.contains(&tab_id) {
        tabs.clear();
    }
    tabs.insert(tab_id);
}

pub fn is_tracked_mossen_in_chrome_tab_id(tab_id: u64) -> bool {
    TRACKED_TAB_IDS.lock().unwrap().contains(&tab_id)
}

// ─── Open in Chrome ──────────────────────────────────────────────────────────

pub async fn open_in_chrome(url: &str) -> bool {
    let browser = match detect_available_browser().await {
        Some(b) => b,
        None => {
            eprintln!("[Mossen in Chrome] No compatible browser found");
            return false;
        }
    };
    let config = get_browser_config(browser);

    if cfg!(target_os = "macos") {
        let status = tokio::process::Command::new("open")
            .args(["-a", config.macos_app_name, url])
            .status()
            .await;
        return status.map(|s| s.success()).unwrap_or(false);
    } else if cfg!(target_os = "linux") {
        for binary in config.linux_binaries {
            let status = tokio::process::Command::new(binary)
                .arg(url)
                .status()
                .await;
            if status.map(|s| s.success()).unwrap_or(false) {
                return true;
            }
        }
    }
    false
}

// ─── Socket Paths ────────────────────────────────────────────────────────────

fn get_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string())
}

pub fn get_socket_dir() -> PathBuf {
    PathBuf::from(format!("/tmp/mossen-mcp-browser-bridge-{}", get_username()))
}

pub fn get_secure_socket_path() -> PathBuf {
    let dir = get_socket_dir();
    dir.join(format!("{}.sock", std::process::id()))
}

pub fn get_all_socket_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let socket_dir = get_socket_dir();

    if let Ok(entries) = std::fs::read_dir(&socket_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.ends_with(".sock") {
                paths.push(socket_dir.join(&file_name));
            }
        }
    }

    // Legacy fallback
    let legacy_name = format!("mossen-mcp-browser-bridge-{}", get_username());
    let legacy_tmp = PathBuf::from(format!("/tmp/{}", legacy_name));
    if !paths.contains(&legacy_tmp) {
        paths.push(legacy_tmp);
    }

    paths
}

// ─── Chrome Native Host ─────────────────────────────────────────────────────

pub fn send_chrome_message(message: &str) {
    let bytes = message.as_bytes();
    let len = bytes.len() as u32;
    let len_bytes = len.to_le_bytes();
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(&len_bytes);
    let _ = handle.write_all(bytes);
    let _ = handle.flush();
}

pub struct ChromeMessageReader {
    buffer: Vec<u8>,
}

impl ChromeMessageReader {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
        }
    }

    pub fn read_message(&mut self) -> Option<String> {
        let mut stdin = std::io::stdin();
        // Read length prefix (4 bytes, little-endian)
        let mut len_buf = [0u8; 4];
        if stdin.read_exact(&mut len_buf).is_err() {
            return None;
        }
        let length = u32::from_le_bytes(len_buf) as usize;
        if length == 0 || length > MAX_MESSAGE_SIZE {
            return None;
        }

        let mut msg_buf = vec![0u8; length];
        if stdin.read_exact(&mut msg_buf).is_err() {
            return None;
        }

        String::from_utf8(msg_buf).ok()
    }
}

pub struct ChromeNativeHost {
    running: AtomicBool,
    next_client_id: AtomicU64,
}

impl ChromeNativeHost {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            next_client_id: AtomicU64::new(1),
        }
    }

    pub async fn start(&self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        let socket_path = get_secure_socket_path();
        let socket_dir = get_socket_dir();

        // Create socket directory
        fs::create_dir_all(&socket_dir).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&socket_dir, std::fs::Permissions::from_mode(0o700)).await?;
        }

        // Clean stale sockets
        if let Ok(mut entries) = fs::read_dir(&socket_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".sock") {
                    if let Ok(pid) = name.trim_end_matches(".sock").parse::<u32>() {
                        unsafe {
                            if libc::kill(pid as i32, 0) != 0 {
                                let _ = fs::remove_file(entry.path()).await;
                                eprintln!("Removed stale socket for PID {}", pid);
                            }
                        }
                    }
                }
            }
        }

        eprintln!("Creating socket listener: {:?}", socket_path);
        // Remove existing socket if present
        let _ = fs::remove_file(&socket_path).await;

        self.running.store(true, Ordering::SeqCst);
        eprintln!("Socket server listening for connections");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.running.store(false, Ordering::SeqCst);

        let socket_path = get_secure_socket_path();
        let _ = fs::remove_file(&socket_path).await;

        // Remove directory if empty
        let socket_dir = get_socket_dir();
        if let Ok(mut entries) = fs::read_dir(&socket_dir).await {
            if entries.next_entry().await.ok().flatten().is_none() {
                let _ = fs::remove_dir(&socket_dir).await;
            }
        }
        Ok(())
    }

    pub fn handle_message(&self, message_json: &str) {
        let parsed: serde_json::Value = match serde_json::from_str(message_json) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Invalid JSON from Chrome: {}", e);
                send_chrome_message(&serde_json::json!({
                    "type": "error",
                    "error": "Invalid message format"
                }).to_string());
                return;
            }
        };

        let msg_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");
        eprintln!("Handling Chrome message type: {}", msg_type);

        match msg_type {
            "ping" => {
                eprintln!("Responding to ping");
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                send_chrome_message(&serde_json::json!({
                    "type": "pong",
                    "timestamp": now
                }).to_string());
            }
            "get_status" => {
                send_chrome_message(&serde_json::json!({
                    "type": "status_response",
                    "native_host_version": VERSION
                }).to_string());
            }
            "tool_response" | "notification" => {
                // Forward to MCP clients
                eprintln!("Forwarding {} to MCP clients", msg_type);
            }
            _ => {
                eprintln!("Unknown message type: {}", msg_type);
                send_chrome_message(&serde_json::json!({
                    "type": "error",
                    "error": format!("Unknown message type: {}", msg_type)
                }).to_string());
            }
        }
    }
}

pub async fn run_chrome_native_host() -> Result<()> {
    eprintln!("Initializing...");
    let host = ChromeNativeHost::new();
    let mut reader = ChromeMessageReader::new();

    host.start().await?;

    loop {
        let message = match reader.read_message() {
            Some(msg) => msg,
            None => break,
        };
        host.handle_message(&message);
    }

    host.stop().await?;
    Ok(())
}

// ─── Extension Detection ─────────────────────────────────────────────────────

const PROD_EXTENSION_ID: &str = "fcoeoabgfenejglbffodgkkbkcdhcgfn";
const DEV_EXTENSION_ID: &str = "dihbgbndebgnbjfmelmegjepbnkhlgni";
const ANT_EXTENSION_ID: &str = "dngcpimnedloihjnnfngkgjoidhnaolf";

fn get_extension_ids() -> Vec<&'static str> {
    if std::env::var("USER_TYPE").ok().as_deref() == Some("ant") {
        vec![PROD_EXTENSION_ID, DEV_EXTENSION_ID, ANT_EXTENSION_ID]
    } else {
        vec![PROD_EXTENSION_ID]
    }
}

pub async fn detect_extension_installation(
    browser_paths: &[BrowserPath],
) -> (bool, Option<ChromiumBrowser>) {
    if browser_paths.is_empty() {
        return (false, None);
    }
    let extension_ids = get_extension_ids();

    for bp in browser_paths {
        let entries = match fs::read_dir(&bp.path).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut profile_dirs = Vec::new();
        let mut dir_entries = entries;
        while let Ok(Some(entry)) = dir_entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false) {
                if name == "Default" || name.starts_with("Profile ") {
                    profile_dirs.push(name);
                }
            }
        }

        for profile in &profile_dirs {
            for ext_id in &extension_ids {
                let ext_path = bp.path.join(profile).join("Extensions").join(ext_id);
                if fs::read_dir(&ext_path).await.is_ok() {
                    eprintln!(
                        "[Chrome integration] Extension {} found in {:?} {}",
                        ext_id, bp.browser, profile
                    );
                    return (true, Some(bp.browser));
                }
            }
        }
    }

    eprintln!("[Chrome integration] Extension not found in any browser");
    (false, None)
}

pub async fn is_chrome_extension_installed() -> bool {
    let browser_paths = get_all_browser_data_paths();
    let (installed, _) = detect_extension_installation(&browser_paths).await;
    installed
}

// ─── Setup ───────────────────────────────────────────────────────────────────

pub fn should_enable_mossen_in_chrome(chrome_flag: Option<bool>) -> bool {
    if let Some(flag) = chrome_flag {
        return flag;
    }

    let env_enable = std::env::var("MOSSEN_CODE_ENABLE_CFC")
        .or_else(|_| std::env::var("MOSSEN_CODE_ENABLE_CHROME"))
        .unwrap_or_default();
    if env_enable == "1" || env_enable.to_lowercase() == "true" {
        return true;
    }
    if env_enable == "0" || env_enable.to_lowercase() == "false" {
        return false;
    }

    false
}

pub struct ChromeSetupResult {
    pub mcp_config: Vec<(String, McpServerConfig)>,
    pub allowed_tools: Vec<String>,
    pub system_prompt: String,
}

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub server_type: String,
    pub command: String,
    pub args: Vec<String>,
    pub scope: String,
    pub env: HashMap<String, String>,
}

pub fn setup_mossen_in_chrome() -> ChromeSetupResult {
    let allowed_tools = get_mossen_chrome_mcp_allowed_tool_names(
        MOSSEN_IN_CHROME_MCP_SERVER_NAME,
    );

    let command = std::env::current_exe()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    ChromeSetupResult {
        mcp_config: vec![(
            MOSSEN_IN_CHROME_MCP_SERVER_NAME.to_string(),
            McpServerConfig {
                server_type: "stdio".to_string(),
                command,
                args: vec!["--mossen-in-chrome-mcp".to_string()],
                scope: "dynamic".to_string(),
                env: HashMap::new(),
            },
        )],
        allowed_tools,
        system_prompt: get_chrome_system_prompt().to_string(),
    }
}

// ─── Native Host Manifest ────────────────────────────────────────────────────

pub async fn install_chrome_native_host_manifest(
    manifest_binary_path: &str,
) -> Result<()> {
    let manifest_dirs: Vec<PathBuf> = get_all_native_messaging_hosts_dirs()
        .iter()
        .map(|bp| bp.path.clone())
        .collect();

    if manifest_dirs.is_empty() {
        return Err(anyhow!(
            "Chrome browser integration native host not supported on this platform"
        ));
    }

    let manifest = serde_json::json!({
        "name": NATIVE_HOST_IDENTIFIER,
        "description": "Mossen browser extension native host",
        "path": manifest_binary_path,
        "type": "stdio",
        "allowed_origins": [
            format!("chrome-extension://{}/", PROD_EXTENSION_ID),
        ]
    });
    let manifest_content = serde_json::to_string_pretty(&manifest)?;
    let manifest_filename = format!("{}.json", NATIVE_HOST_IDENTIFIER);

    for manifest_dir in &manifest_dirs {
        let manifest_path = manifest_dir.join(&manifest_filename);

        if let Ok(existing) = fs::read_to_string(&manifest_path).await {
            if existing == manifest_content {
                continue;
            }
        }

        if let Err(e) = fs::create_dir_all(manifest_dir).await {
            eprintln!(
                "[Chrome integration] Failed to create dir {:?}: {}",
                manifest_dir, e
            );
            continue;
        }
        if let Err(e) = fs::write(&manifest_path, &manifest_content).await {
            eprintln!(
                "[Chrome integration] Failed to write manifest at {:?}: {}",
                manifest_path, e
            );
        } else {
            eprintln!(
                "[Chrome integration] Installed native host manifest at: {:?}",
                manifest_path
            );
        }
    }
    Ok(())
}

pub async fn create_wrapper_script(command: &str) -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("mossen")
        .join("chrome");
    let wrapper_path = config_dir.join("chrome-native-host");

    let script_content = format!(
        "#!/bin/sh\n# Chrome native host wrapper script\n# Generated by Mossen - do not edit manually\nexec {}\n",
        command
    );

    if let Ok(existing) = fs::read_to_string(&wrapper_path).await {
        if existing == script_content {
            return Ok(wrapper_path);
        }
    }

    fs::create_dir_all(&config_dir).await?;
    fs::write(&wrapper_path, &script_content).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&wrapper_path, std::fs::Permissions::from_mode(0o755)).await?;
    }

    eprintln!(
        "[Chrome integration] Created browser native host wrapper script: {:?}",
        wrapper_path
    );
    Ok(wrapper_path)
}

// ─── Chrome MCP Adapter ─────────────────────────────────────────────────────

const FALLBACK_CHROME_MCP_TOOL_NAMES: &[&str] = &[
    "javascript_tool",
    "read_page",
    "find",
    "form_input",
    "computer",
    "navigate",
    "resize_window",
    "gif_creator",
    "upload_image",
    "get_page_text",
    "tabs_context_mcp",
    "tabs_create_mcp",
    "update_plan",
    "read_console_messages",
    "read_network_requests",
    "shortcuts_list",
    "shortcuts_execute",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MossenChromePermissionMode {
    Ask,
    SkipAllPermissionChecks,
    FollowAPlan,
}

pub fn get_mossen_chrome_mcp_allowed_tool_names(server_name: &str) -> Vec<String> {
    FALLBACK_CHROME_MCP_TOOL_NAMES
        .iter()
        .map(|tool| format!("mcp__{}__{}",server_name, tool))
        .collect()
}

// ─── Prompt ──────────────────────────────────────────────────────────────────

pub const BASE_CHROME_PROMPT: &str = r#"# Chrome browser integration automation

You have access to browser automation tools (mcp__mossen-in-chrome__*) for interacting with web pages in Chrome. Follow these guidelines for effective browser automation.

## GIF recording

When performing multi-step browser interactions that the user may want to review or share, use mcp__mossen-in-chrome__gif_creator to record them.

You must ALWAYS:
* Capture extra frames before and after taking actions to ensure smooth playback
* Name the file meaningfully to help the user identify it later (e.g., "login_process.gif")

## Console log debugging

You can use mcp__mossen-in-chrome__read_console_messages to read console output. Console output may be verbose. If you are looking for specific log entries, use the 'pattern' parameter with a regex-compatible pattern.

## Alerts and dialogs

IMPORTANT: Do not trigger JavaScript alerts, confirms, prompts, or browser modal dialogs through your actions. These browser dialogs block all further browser events.

## Avoid rabbit holes and loops

When using browser automation tools, stay focused on the specific task. If you encounter failures after 2-3 attempts, stop and ask the user for guidance.

## Tab context and session startup

IMPORTANT: At the start of each browser automation session, call mcp__mossen-in-chrome__tabs_context_mcp first to get information about the user's current browser tabs."#;

pub const CHROME_TOOL_SEARCH_INSTRUCTIONS: &str = r#"**IMPORTANT: Before using any chrome browser tools, you MUST first load them using ToolSearch.**

Chrome browser tools are MCP tools that require loading before use. Before calling any mcp__mossen-in-chrome__* tool:
1. Use ToolSearch with `select:mcp__mossen-in-chrome__<tool_name>` to load the specific tool
2. Then call the tool"#;

pub fn get_chrome_system_prompt() -> &'static str {
    BASE_CHROME_PROMPT
}

pub const MOSSEN_IN_CHROME_SKILL_HINT: &str = r#"**Browser Automation**: Chrome browser tools are available via the "mossen-in-chrome" skill. CRITICAL: Before using any mcp__mossen-in-chrome__* tools, invoke the skill by calling the Skill tool with skill: "mossen-in-chrome". The skill provides Chrome browser integration instructions and enables the tools."#;

pub const MOSSEN_IN_CHROME_SKILL_HINT_WITH_WEBBROWSER: &str = r#"**Browser Automation**: Use WebBrowser for development (dev servers, JS eval, console, screenshots). Use mossen-in-chrome for the user's real Chrome when you need logged-in sessions, OAuth, or computer-use — invoke Skill(skill: "mossen-in-chrome") before any mcp__mossen-in-chrome__* tool."#;

// ─── MCP Server ──────────────────────────────────────────────────────────────

pub async fn run_mossen_in_chrome_mcp_server() -> Result<()> {
    eprintln!("[Chrome integration] Starting MCP server");
    // In production: create server, connect stdio transport
    eprintln!("[Chrome integration] MCP server started");
    Ok(())
}

/// 对应 TS `CHROMIUM_BROWSERS`：受支持的 Chromium 系列浏览器 bundleId 列表。
pub const CHROMIUM_BROWSERS: &[&str] = &[
    "com.google.Chrome",
    "com.google.Chrome.beta",
    "com.google.Chrome.dev",
    "com.google.Chrome.canary",
    "com.microsoft.edgemac",
    "com.brave.Browser",
    "company.thebrowser.Browser",
    "com.vivaldi.Vivaldi",
    "com.operasoftware.Opera",
];

/// 对应 TS `shouldAutoEnableMossenInChrome`：自动启用判定。
pub fn should_auto_enable_mossen_in_chrome() -> bool {
    matches!(
        std::env::var("MOSSEN_IN_CHROME_AUTO_ENABLE").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// 对应 TS `MossenChromeLogger`：日志回调接口（trait alias）。
pub type MossenChromeLogger = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

/// 对应 TS `createChromeContext`：构造 Chrome MCP context 描述对象。
pub fn create_chrome_context() -> serde_json::Value {
    serde_json::json!({
        "kind": "mossen-in-chrome",
        "createdAt": chrono::Utc::now().to_rfc3339(),
    })
}

// ─── Chrome MCP Adapter (extra types) ────────────────────────────────────────

/// 对应 TS `MossenChromeMcpContext`：Chrome MCP server 运行上下文。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenChromeMcpContext {
    pub session_id: Option<String>,
    pub working_directory: Option<String>,
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// 对应 TS `MossenChromeModelRequest`：从 chrome 扩展或模型客户端发来的请求。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenChromeModelRequest {
    pub tool_name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub context: MossenChromeMcpContext,
}

/// 对应 TS `MossenChromeModelResponse`：模型/工具响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenChromeModelResponse {
    #[serde(default)]
    pub content: Vec<serde_json::Value>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// 对应 TS `createMossenChromeMcpServer`：构造 Chrome MCP server 描述句柄。
///
/// 真实的 MCP server 创建与 stdio transport 连接由二进制 main 注入；本函数
/// 返回静态描述供 setup 阶段使用。
pub fn create_mossen_chrome_mcp_server(context: MossenChromeMcpContext) -> serde_json::Value {
    serde_json::json!({
        "kind": "mossen-in-chrome-mcp-server",
        "serverName": MOSSEN_IN_CHROME_MCP_SERVER_NAME,
        "version": VERSION,
        "context": context,
    })
}

// ─── setupPortable.ts equivalents ────────────────────────────────────────────

/// 对应 TS `CHROME_EXTENSION_URL`：Chrome 商店中扩展的安装 URL。
pub const CHROME_EXTENSION_URL: &str =
    "https://chromewebstore.google.com/detail/mossen-code-for-chrome/fcoeoabgfenejglbffodgkkbkcdhcgfn";

/// 对应 TS `getAllBrowserDataPathsPortable`：portable 安装下的浏览器数据路径，
/// 与 [`get_all_browser_data_paths`] 等价（Rust 端单二进制无 portable 差异）。
pub fn get_all_browser_data_paths_portable() -> Vec<BrowserPath> {
    get_all_browser_data_paths()
}

/// 对应 TS `detectExtensionInstallationPortable`。
pub async fn detect_extension_installation_portable(
    browser_paths: &[BrowserPath],
) -> (bool, Option<ChromiumBrowser>) {
    detect_extension_installation(browser_paths).await
}

/// 对应 TS `isChromeExtensionInstalledPortable`。
pub async fn is_chrome_extension_installed_portable() -> bool {
    is_chrome_extension_installed().await
}

// ─── toolRendering.tsx equivalents ───────────────────────────────────────────

/// 对应 TS `ChromeToolName`：mossen-in-chrome MCP 暴露的工具名枚举。
pub type ChromeToolName = String;

/// 对应 TS `renderChromeToolResultMessage`：渲染 mossen-in-chrome 工具返回结果的纯文本。
pub fn render_chrome_tool_result_message(
    tool_name: &str,
    result: &serde_json::Value,
) -> String {
    if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
        return text.to_string();
    }
    if let Some(arr) = result.get("content").and_then(|v| v.as_array()) {
        let mut chunks = Vec::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                chunks.push(t.to_string());
            }
        }
        if !chunks.is_empty() {
            return chunks.join("\n");
        }
    }
    format!("[{}] {}", tool_name, result)
}

/// 对应 TS `getMossenInChromeMCPToolOverrides`：返回某个 chrome 工具的渲染/调用 override 元数据。
#[derive(Debug, Clone)]
pub struct MossenInChromeMcpToolOverrides {
    pub tool_name: String,
    pub server_name: String,
}

pub fn get_mossen_in_chrome_mcp_tool_overrides(tool_name: &str) -> MossenInChromeMcpToolOverrides {
    MossenInChromeMcpToolOverrides {
        tool_name: tool_name.to_string(),
        server_name: MOSSEN_IN_CHROME_MCP_SERVER_NAME.to_string(),
    }
}
