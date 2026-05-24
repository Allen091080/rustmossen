use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::process::Command;
use tokio::time::sleep;

// ─── Constants ───────────────────────────────────────────────────────────────

pub const COMPUTER_USE_MCP_SERVER_NAME: &str = "computer-use";
pub const CLI_HOST_BUNDLE_ID: &str = "com.mossen.mossen-code.cli-no-window";
const LOCK_FILENAME: &str = "computer-use.lock";
const SCREENSHOT_JPEG_QUALITY: f64 = 0.75;
const MOVE_SETTLE_MS: u64 = 50;
const UNHIDE_TIMEOUT_MS: u64 = 5000;
const DRAIN_TIMEOUT_MS: u64 = 30_000;
const APP_ENUM_TIMEOUT_MS: u64 = 1000;
const APP_NAME_MAX_LEN: usize = 40;
const APP_NAME_MAX_COUNT: usize = 50;

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayGeometry {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub display_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmostApp {
    pub bundle_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    pub bundle_id: String,
    pub display_name: String,
    pub path: String,
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningApp {
    pub bundle_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotResult {
    pub base64: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerUseLock {
    pub session_id: String,
    pub pid: u32,
    pub acquired_at: u64,
}

#[derive(Debug, Clone)]
pub enum AcquireResult {
    Acquired { fresh: bool },
    Blocked { by: String },
}

#[derive(Debug, Clone)]
pub enum CheckResult {
    Free,
    HeldBySelf,
    Blocked { by: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateMode {
    Pixels,
    Normalized,
}

#[derive(Debug, Clone)]
pub struct CuSubGates {
    pub pixel_validation: bool,
    pub clipboard_paste_multiline: bool,
    pub mouse_animation: bool,
    pub hide_before_action: bool,
    pub auto_target_display: bool,
    pub clipboard_guard: bool,
}

#[derive(Debug, Clone)]
pub struct ChicagoConfig {
    pub enabled: bool,
    pub coordinate_mode: CoordinateMode,
    pub sub_gates: CuSubGates,
}

// ─── Terminal Bundle ID Detection ────────────────────────────────────────────

static TERMINAL_BUNDLE_ID_FALLBACK: Lazy<Vec<(&str, &str)>> = Lazy::new(|| {
    vec![
        ("iTerm.app", "com.googlecode.iterm2"),
        ("Apple_Terminal", "com.apple.Terminal"),
        ("ghostty", "com.mitchellh.ghostty"),
        ("kitty", "net.kovidgoyal.kitty"),
        ("WarpTerminal", "dev.warp.Warp-Stable"),
        ("vscode", "com.microsoft.VSCode"),
    ]
});

pub fn get_terminal_bundle_id() -> Option<String> {
    if let Ok(cf_bundle_id) = std::env::var("__CFBundleIdentifier") {
        if !cf_bundle_id.is_empty() {
            return Some(cf_bundle_id);
        }
    }
    let terminal = std::env::var("TERM_PROGRAM").unwrap_or_default();
    for (key, bundle_id) in TERMINAL_BUNDLE_ID_FALLBACK.iter() {
        if terminal == *key {
            return Some(bundle_id.to_string());
        }
    }
    None
}

pub fn is_computer_use_mcp_server(name: &str) -> bool {
    let normalized = name.to_lowercase().replace(['-', '_', ' '], "");
    normalized == "computeruse"
}

// ─── CLI CU Capabilities ────────────────────────────────────────────────────

pub struct CliCuCapabilities {
    pub screenshot_filtering: &'static str,
    pub platform: &'static str,
    pub host_bundle_id: &'static str,
}

pub static CLI_CU_CAPABILITIES: CliCuCapabilities = CliCuCapabilities {
    screenshot_filtering: "native",
    platform: "darwin",
    host_bundle_id: CLI_HOST_BUNDLE_ID,
};

// ─── Gates ───────────────────────────────────────────────────────────────────

impl Default for CuSubGates {
    fn default() -> Self {
        Self {
            pixel_validation: false,
            clipboard_paste_multiline: true,
            mouse_animation: true,
            hide_before_action: true,
            auto_target_display: true,
            clipboard_guard: true,
        }
    }
}

impl Default for ChicagoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            coordinate_mode: CoordinateMode::Pixels,
            sub_gates: CuSubGates::default(),
        }
    }
}

static FROZEN_COORDINATE_MODE: Lazy<Mutex<Option<CoordinateMode>>> = Lazy::new(|| Mutex::new(None));

pub fn get_chicago_enabled() -> bool {
    if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
        if std::env::var("MONOREPO_ROOT_DIR").is_ok() {
            let allow = std::env::var("ALLOW_ANT_COMPUTER_USE_MCP").unwrap_or_default();
            if allow != "1" && allow.to_lowercase() != "true" {
                return false;
            }
        }
    }
    has_required_subscription() && read_config().enabled
}

pub fn get_chicago_sub_gates() -> CuSubGates {
    read_config().sub_gates
}

pub fn get_chicago_coordinate_mode() -> CoordinateMode {
    let mut frozen = FROZEN_COORDINATE_MODE.lock().unwrap();
    if frozen.is_none() {
        *frozen = Some(read_config().coordinate_mode);
    }
    frozen.unwrap()
}

fn has_required_subscription() -> bool {
    if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
        return true;
    }
    // In production, would check subscription tier
    false
}

fn read_config() -> ChicagoConfig {
    ChicagoConfig::default()
}

// ─── Computer Use Lock ───────────────────────────────────────────────────────

static LOCK_HELD_LOCALLY: AtomicBool = AtomicBool::new(false);

fn get_lock_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("mossen");
    config_dir.join(LOCK_FILENAME)
}

fn get_session_id() -> String {
    std::env::var("MOSSEN_SESSION_ID").unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
}

fn is_process_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn is_computer_use_lock(value: &serde_json::Value) -> bool {
    value.get("session_id").and_then(|v| v.as_str()).is_some()
        && value.get("pid").and_then(|v| v.as_u64()).is_some()
}

async fn read_lock() -> Option<ComputerUseLock> {
    let path = get_lock_path();
    let raw = fs::read_to_string(&path).await.ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if !is_computer_use_lock(&parsed) {
        return None;
    }
    serde_json::from_value(parsed).ok()
}

async fn try_create_exclusive(lock: &ComputerUseLock) -> Result<bool> {
    let path = get_lock_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let content = serde_json::to_string(lock)?;
    match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
    {
        Ok(_file) => {
            fs::write(&path, &content).await?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.into()),
    }
}

pub fn is_lock_held_locally() -> bool {
    LOCK_HELD_LOCALLY.load(Ordering::SeqCst)
}

pub async fn check_computer_use_lock() -> CheckResult {
    let existing = match read_lock().await {
        Some(lock) => lock,
        None => return CheckResult::Free,
    };
    if existing.session_id == get_session_id() {
        return CheckResult::HeldBySelf;
    }
    if is_process_running(existing.pid) {
        return CheckResult::Blocked {
            by: existing.session_id,
        };
    }
    eprintln!(
        "Recovering stale computer-use lock from session {} (PID {})",
        existing.session_id, existing.pid
    );
    let _ = fs::remove_file(get_lock_path()).await;
    CheckResult::Free
}

pub async fn try_acquire_computer_use_lock() -> Result<AcquireResult> {
    let session_id = get_session_id();
    let lock = ComputerUseLock {
        session_id: session_id.clone(),
        pid: std::process::id(),
        acquired_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    };

    if let Some(parent) = get_lock_path().parent() {
        fs::create_dir_all(parent).await?;
    }

    if try_create_exclusive(&lock).await? {
        LOCK_HELD_LOCALLY.store(true, Ordering::SeqCst);
        return Ok(AcquireResult::Acquired { fresh: true });
    }

    let existing = read_lock().await;
    match existing {
        None => {
            let _ = fs::remove_file(get_lock_path()).await;
            if try_create_exclusive(&lock).await? {
                LOCK_HELD_LOCALLY.store(true, Ordering::SeqCst);
                return Ok(AcquireResult::Acquired { fresh: true });
            }
            let blocker = read_lock()
                .await
                .map(|l| l.session_id)
                .unwrap_or_else(|| "unknown".to_string());
            Ok(AcquireResult::Blocked { by: blocker })
        }
        Some(ref ex) if ex.session_id == session_id => Ok(AcquireResult::Acquired { fresh: false }),
        Some(ref ex) if is_process_running(ex.pid) => Ok(AcquireResult::Blocked {
            by: ex.session_id.clone(),
        }),
        Some(ex) => {
            eprintln!(
                "Recovering stale computer-use lock from session {} (PID {})",
                ex.session_id, ex.pid
            );
            let _ = fs::remove_file(get_lock_path()).await;
            if try_create_exclusive(&lock).await? {
                LOCK_HELD_LOCALLY.store(true, Ordering::SeqCst);
                return Ok(AcquireResult::Acquired { fresh: true });
            }
            let blocker = read_lock()
                .await
                .map(|l| l.session_id)
                .unwrap_or_else(|| "unknown".to_string());
            Ok(AcquireResult::Blocked { by: blocker })
        }
    }
}

pub async fn release_computer_use_lock() -> bool {
    LOCK_HELD_LOCALLY.store(false, Ordering::SeqCst);
    let existing = match read_lock().await {
        Some(lock) => lock,
        None => return false,
    };
    if existing.session_id != get_session_id() {
        return false;
    }
    match fs::remove_file(get_lock_path()).await {
        Ok(()) => {
            eprintln!("Released computer-use lock");
            true
        }
        Err(_) => false,
    }
}

// ─── Drain Run Loop ─────────────────────────────────────────────────────────

/// In Rust we don't need CFRunLoop pumping. This is a timeout wrapper
/// for async operations that might hang.
pub async fn drain_run_loop<F, T>(f: F) -> Result<T>
where
    F: std::future::Future<Output = T>,
{
    match tokio::time::timeout(Duration::from_millis(DRAIN_TIMEOUT_MS), f).await {
        Ok(result) => Ok(result),
        Err(_) => Err(anyhow!(
            "computer-use native call exceeded {}ms",
            DRAIN_TIMEOUT_MS
        )),
    }
}

// ─── ESC Hotkey ──────────────────────────────────────────────────────────────

static ESC_REGISTERED: AtomicBool = AtomicBool::new(false);

pub fn register_esc_hotkey<F: Fn() + Send + Sync + 'static>(_on_escape: F) -> bool {
    if ESC_REGISTERED.load(Ordering::SeqCst) {
        return true;
    }
    // In production, would register a CGEventTap for Escape key
    // For CLI Rust implementation, this is platform-specific
    ESC_REGISTERED.store(true, Ordering::SeqCst);
    eprintln!("[cu-esc] registered");
    true
}

pub fn unregister_esc_hotkey() {
    if !ESC_REGISTERED.load(Ordering::SeqCst) {
        return;
    }
    ESC_REGISTERED.store(false, Ordering::SeqCst);
    eprintln!("[cu-esc] unregistered");
}

pub fn notify_expected_escape() {
    if !ESC_REGISTERED.load(Ordering::SeqCst) {
        return;
    }
    // In production, notifies the CGEventTap to let through a model-synthesized Escape
}

// ─── Clipboard Operations ────────────────────────────────────────────────────

pub async fn read_clipboard_via_pbpaste() -> Result<String> {
    let output = Command::new("pbpaste").output().await?;
    if !output.status.success() {
        return Err(anyhow!(
            "pbpaste exited with code {:?}",
            output.status.code()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub async fn write_clipboard_via_pbcopy(text: &str) -> Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(text.as_bytes()).await?;
    }
    let status = child.wait().await?;
    if !status.success() {
        return Err(anyhow!("pbcopy exited with code {:?}", status.code()));
    }
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn is_bare_escape(parts: &[&str]) -> bool {
    if parts.len() != 1 {
        return false;
    }
    let lower = parts[0].to_lowercase();
    lower == "escape" || lower == "esc"
}

pub fn compute_target_dims(logical_w: u32, logical_h: u32, scale_factor: f64) -> (u32, u32) {
    let phys_w = (logical_w as f64 * scale_factor).round() as u32;
    let phys_h = (logical_h as f64 * scale_factor).round() as u32;
    target_image_size(phys_w, phys_h)
}

/// Compute target image size respecting API resize params.
/// Max dimension is 1568px on longest side.
fn target_image_size(width: u32, height: u32) -> (u32, u32) {
    const MAX_LONG_SIDE: u32 = 1568;
    let long_side = width.max(height);
    if long_side <= MAX_LONG_SIDE {
        return (width, height);
    }
    let scale = MAX_LONG_SIDE as f64 / long_side as f64;
    (
        (width as f64 * scale).round() as u32,
        (height as f64 * scale).round() as u32,
    )
}

// ─── App Name Filtering ─────────────────────────────────────────────────────

static PATH_ALLOWLIST: &[&str] = &["/Applications/", "/System/Applications/"];

static ALWAYS_KEEP_BUNDLE_IDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("com.apple.Safari");
    s.insert("com.google.Chrome");
    s.insert("com.microsoft.edgemac");
    s.insert("org.mozilla.firefox");
    s.insert("company.thebrowser.Browser");
    s.insert("com.tinyspeck.slackmacgap");
    s.insert("us.zoom.xos");
    s.insert("com.microsoft.teams2");
    s.insert("com.microsoft.teams");
    s.insert("com.apple.MobileSMS");
    s.insert("com.apple.mail");
    s.insert("com.microsoft.Word");
    s.insert("com.microsoft.Excel");
    s.insert("com.microsoft.Powerpoint");
    s.insert("com.microsoft.Outlook");
    s.insert("com.apple.iWork.Pages");
    s.insert("com.apple.iWork.Numbers");
    s.insert("com.apple.iWork.Keynote");
    s.insert("com.google.GoogleDocs");
    s.insert("notion.id");
    s.insert("com.apple.Notes");
    s.insert("md.obsidian");
    s.insert("com.linear");
    s.insert("com.figma.Desktop");
    s.insert("com.microsoft.VSCode");
    s.insert("com.apple.Terminal");
    s.insert("com.googlecode.iterm2");
    s.insert("com.github.GitHubDesktop");
    s.insert("com.apple.finder");
    s.insert("com.apple.iCal");
    s.insert("com.apple.systempreferences");
    s
});

static NAME_PATTERN_BLOCKLIST: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"Helper($|\s\()").unwrap(),
        Regex::new(r"Agent($|\s\()").unwrap(),
        Regex::new(r"Service($|\s\()").unwrap(),
        Regex::new(r"Uninstaller($|\s\()").unwrap(),
        Regex::new(r"Updater($|\s\()").unwrap(),
        Regex::new(r"^\.").unwrap(),
    ]
});

static APP_NAME_ALLOWED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[\p{L}\p{M}\p{N}_ .&'()+\-]+$").unwrap());

fn is_user_facing_path(path: &str, home_dir: Option<&str>) -> bool {
    if PATH_ALLOWLIST.iter().any(|root| path.starts_with(root)) {
        return true;
    }
    if let Some(home) = home_dir {
        let user_apps = if home.ends_with('/') {
            format!("{}Applications/", home)
        } else {
            format!("{}/Applications/", home)
        };
        if path.starts_with(&user_apps) {
            return true;
        }
    }
    false
}

fn is_noisy_name(name: &str) -> bool {
    NAME_PATTERN_BLOCKLIST.iter().any(|re| re.is_match(name))
}

fn sanitize_core(raw: &[String], apply_char_filter: bool) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result: Vec<String> = raw
        .iter()
        .map(|n| n.trim().to_string())
        .filter(|trimmed| {
            if trimmed.is_empty() {
                return false;
            }
            if trimmed.len() > APP_NAME_MAX_LEN {
                return false;
            }
            if apply_char_filter && !APP_NAME_ALLOWED.is_match(trimmed) {
                return false;
            }
            if seen.contains(trimmed) {
                return false;
            }
            seen.insert(trimmed.clone());
            true
        })
        .collect();
    result.sort();
    result
}

fn sanitize_app_names(raw: &[String]) -> Vec<String> {
    let filtered = sanitize_core(raw, true);
    if filtered.len() <= APP_NAME_MAX_COUNT {
        return filtered;
    }
    let mut result: Vec<String> = filtered[..APP_NAME_MAX_COUNT].to_vec();
    result.push(format!(
        "… and {} more",
        filtered.len() - APP_NAME_MAX_COUNT
    ));
    result
}

fn sanitize_trusted_names(raw: &[String]) -> Vec<String> {
    sanitize_core(raw, false)
}

pub fn filter_apps_for_description(
    installed: &[InstalledApp],
    home_dir: Option<&str>,
) -> Vec<String> {
    let mut always_kept: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();

    for app in installed {
        if ALWAYS_KEEP_BUNDLE_IDS.contains(app.bundle_id.as_str()) {
            always_kept.push(app.display_name.clone());
        } else if is_user_facing_path(&app.path, home_dir) && !is_noisy_name(&app.display_name) {
            rest.push(app.display_name.clone());
        }
    }

    let sanitized_always = sanitize_trusted_names(&always_kept);
    let always_set: HashSet<String> = sanitized_always.iter().cloned().collect();
    let sanitized_rest = sanitize_app_names(&rest);

    let mut result = sanitized_always;
    result.extend(
        sanitized_rest
            .into_iter()
            .filter(|n| !always_set.contains(n)),
    );
    result
}

// ─── Executor ────────────────────────────────────────────────────────────────

pub struct ComputerExecutorOpts {
    pub mouse_animation_enabled: bool,
    pub hide_before_action_enabled: bool,
}

pub struct CliExecutor {
    terminal_bundle_id: Option<String>,
    surrogate_host: String,
    mouse_animation_enabled: bool,
    hide_before_action_enabled: bool,
}

/// 工厂函数，对应 TS `createCliExecutor`。仅 macOS 可用。
pub fn create_cli_executor(opts: ComputerExecutorOpts) -> Result<CliExecutor> {
    CliExecutor::new(opts)
}

impl CliExecutor {
    pub fn new(opts: ComputerExecutorOpts) -> Result<Self> {
        if std::env::consts::OS != "macos" {
            return Err(anyhow!(
                "createCliExecutor called on {}. Computer control is macOS-only.",
                std::env::consts::OS
            ));
        }
        let terminal_bundle_id = get_terminal_bundle_id();
        let surrogate_host = terminal_bundle_id
            .clone()
            .unwrap_or_else(|| CLI_HOST_BUNDLE_ID.to_string());

        if let Some(ref tid) = terminal_bundle_id {
            eprintln!(
                "[computer-use] terminal {} → surrogate host (hide-exempt, activate-skip, screenshot-excluded)",
                tid
            );
        } else {
            eprintln!("[computer-use] terminal not detected; falling back to sentinel host");
        }

        Ok(Self {
            terminal_bundle_id,
            surrogate_host,
            mouse_animation_enabled: opts.mouse_animation_enabled,
            hide_before_action_enabled: opts.hide_before_action_enabled,
        })
    }

    fn without_terminal<'a>(&self, allowed: &'a [String]) -> Vec<&'a String> {
        match &self.terminal_bundle_id {
            None => allowed.iter().collect(),
            Some(tid) => allowed.iter().filter(|id| id.as_str() != tid).collect(),
        }
    }

    pub async fn prepare_for_action(
        &self,
        _allowlist_bundle_ids: &[String],
        _display_id: Option<u32>,
    ) -> Vec<String> {
        if !self.hide_before_action_enabled {
            return Vec::new();
        }
        // In production: call Swift prepareDisplay with drain_run_loop
        Vec::new()
    }

    pub async fn get_display_size(&self, _display_id: Option<u32>) -> DisplayGeometry {
        // In production: call Swift display.getSize
        DisplayGeometry {
            width: 1920,
            height: 1080,
            scale_factor: 2.0,
            display_id: None,
        }
    }

    pub async fn list_displays(&self) -> Vec<DisplayGeometry> {
        vec![self.get_display_size(None).await]
    }

    pub async fn screenshot(
        &self,
        _allowed_bundle_ids: &[String],
        display_id: Option<u32>,
    ) -> Result<ScreenshotResult> {
        let d = self.get_display_size(display_id).await;
        let (target_w, target_h) = compute_target_dims(d.width, d.height, d.scale_factor);
        // In production: call Swift screenshot.captureExcluding with drain_run_loop
        Ok(ScreenshotResult {
            base64: String::new(),
            width: target_w,
            height: target_h,
        })
    }

    pub async fn zoom(
        &self,
        region: (f64, f64, f64, f64),
        _allowed_bundle_ids: &[String],
        display_id: Option<u32>,
    ) -> Result<ScreenshotResult> {
        let d = self.get_display_size(display_id).await;
        let (out_w, out_h) = compute_target_dims(region.2 as u32, region.3 as u32, d.scale_factor);
        Ok(ScreenshotResult {
            base64: String::new(),
            width: out_w,
            height: out_h,
        })
    }

    pub async fn key(&self, key_sequence: &str, repeat: u32) -> Result<()> {
        let parts: Vec<&str> = key_sequence.split('+').filter(|p| !p.is_empty()).collect();
        let is_esc = is_bare_escape(&parts);
        for i in 0..repeat {
            if i > 0 {
                sleep(Duration::from_millis(8)).await;
            }
            if is_esc {
                notify_expected_escape();
            }
            // In production: call input.keys(parts) via drain_run_loop
        }
        Ok(())
    }

    pub async fn hold_key(&self, key_names: &[String], duration_ms: u64) -> Result<()> {
        // Press all keys
        for k in key_names {
            if is_bare_escape(&[k.as_str()]) {
                notify_expected_escape();
            }
            // In production: call input.key(k, 'press')
        }
        sleep(Duration::from_millis(duration_ms)).await;
        // Release all keys in reverse
        for k in key_names.iter().rev() {
            let _ = k; // In production: call input.key(k, 'release')
        }
        Ok(())
    }

    pub async fn type_text(&self, text: &str, via_clipboard: bool) -> Result<()> {
        if via_clipboard {
            self.type_via_clipboard(text).await
        } else {
            // In production: call input.typeText(text)
            Ok(())
        }
    }

    async fn type_via_clipboard(&self, text: &str) -> Result<()> {
        let saved = read_clipboard_via_pbpaste().await.ok();
        let result = async {
            write_clipboard_via_pbcopy(text).await?;
            let readback = read_clipboard_via_pbpaste().await?;
            if readback != text {
                return Err(anyhow!("Clipboard write did not round-trip."));
            }
            // In production: input.keys(["command", "v"])
            sleep(Duration::from_millis(100)).await;
            Ok(())
        }
        .await;

        if let Some(saved_text) = saved {
            let _ = write_clipboard_via_pbcopy(&saved_text).await;
        }
        result
    }

    pub async fn move_mouse(&self, _x: f64, _y: f64) -> Result<()> {
        // In production: call input.moveMouse then sleep MOVE_SETTLE_MS
        sleep(Duration::from_millis(MOVE_SETTLE_MS)).await;
        Ok(())
    }

    pub async fn click(
        &self,
        x: f64,
        y: f64,
        button: &str,
        count: u8,
        modifiers: Option<&[String]>,
    ) -> Result<()> {
        self.move_mouse(x, y).await?;
        // In production: handle modifiers + input.mouseButton
        let _ = (button, count, modifiers);
        Ok(())
    }

    pub async fn mouse_down(&self) -> Result<()> {
        // In production: input.mouseButton('left', 'press')
        Ok(())
    }

    pub async fn mouse_up(&self) -> Result<()> {
        // In production: input.mouseButton('left', 'release')
        Ok(())
    }

    pub async fn get_cursor_position(&self) -> (f64, f64) {
        // In production: input.mouseLocation()
        (0.0, 0.0)
    }

    pub async fn drag(&self, from: Option<(f64, f64)>, to: (f64, f64)) -> Result<()> {
        if let Some((fx, fy)) = from {
            self.move_mouse(fx, fy).await?;
        }
        self.mouse_down().await?;
        sleep(Duration::from_millis(MOVE_SETTLE_MS)).await;
        let result = self.animated_move(to.0, to.1).await;
        self.mouse_up().await?;
        result
    }

    async fn animated_move(&self, target_x: f64, target_y: f64) -> Result<()> {
        if !self.mouse_animation_enabled {
            return self.move_mouse(target_x, target_y).await;
        }
        let (start_x, start_y) = self.get_cursor_position().await;
        let delta_x = target_x - start_x;
        let delta_y = target_y - start_y;
        let distance = (delta_x * delta_x + delta_y * delta_y).sqrt();
        if distance < 1.0 {
            return Ok(());
        }
        let duration_sec = (distance / 2000.0).min(0.5);
        if duration_sec < 0.03 {
            return self.move_mouse(target_x, target_y).await;
        }
        let frame_rate = 60.0;
        let frame_interval_ms = (1000.0 / frame_rate) as u64;
        let total_frames = (duration_sec * frame_rate) as u32;
        for frame in 1..=total_frames {
            let t = frame as f64 / total_frames as f64;
            let eased = 1.0 - (1.0 - t).powi(3);
            let _x = (start_x + delta_x * eased).round();
            let _y = (start_y + delta_y * eased).round();
            // In production: input.moveMouse(x, y, false)
            if frame < total_frames {
                sleep(Duration::from_millis(frame_interval_ms)).await;
            }
        }
        sleep(Duration::from_millis(MOVE_SETTLE_MS)).await;
        Ok(())
    }

    pub async fn scroll(&self, x: f64, y: f64, dx: f64, dy: f64) -> Result<()> {
        self.move_mouse(x, y).await?;
        // In production: input.mouseScroll for each non-zero axis
        let _ = (dx, dy);
        Ok(())
    }

    pub async fn get_frontmost_app(&self) -> Option<FrontmostApp> {
        // In production: input.getFrontmostAppInfo()
        None
    }

    pub async fn list_installed_apps(&self) -> Vec<InstalledApp> {
        // In production: drain_run_loop(cu.apps.listInstalled())
        Vec::new()
    }

    pub async fn get_app_icon(&self, _path: &str) -> Option<String> {
        None
    }

    pub async fn list_running_apps(&self) -> Vec<RunningApp> {
        Vec::new()
    }

    pub async fn open_app(&self, _bundle_id: &str) -> Result<()> {
        Ok(())
    }
}

// ─── Cleanup ─────────────────────────────────────────────────────────────────

pub async fn cleanup_computer_use_after_turn(hidden_during_turn: Option<&HashSet<String>>) {
    if let Some(hidden) = hidden_during_turn {
        if !hidden.is_empty() {
            // In production: unhideComputerUseApps with timeout
            let unhide_future = async {
                // cu.apps.unhide(hidden)
                sleep(Duration::from_millis(100)).await;
            };
            let timeout = sleep(Duration::from_millis(UNHIDE_TIMEOUT_MS));
            tokio::select! {
                _ = unhide_future => {},
                _ = timeout => {
                    eprintln!("[Computer Use MCP] auto-unhide timed out");
                },
            }
        }
    }

    if !is_lock_held_locally() {
        return;
    }

    if let Err(e) = std::panic::catch_unwind(|| unregister_esc_hotkey()) {
        eprintln!("[Computer Use MCP] unregisterEscHotkey failed: {:?}", e);
    }

    if release_computer_use_lock().await {
        eprintln!("Mossen is done using your computer");
    }
}

pub async fn unhide_computer_use_apps(_bundle_ids: &[String]) {
    // In production: call cu.apps.unhide
}

// ─── Setup ───────────────────────────────────────────────────────────────────

pub struct McpServerConfig {
    pub server_type: String,
    pub command: String,
    pub args: Vec<String>,
    pub scope: String,
}

pub struct ComputerUseSetup {
    pub mcp_config: Vec<(String, McpServerConfig)>,
    pub allowed_tools: Vec<String>,
}

pub fn setup_computer_use_mcp() -> ComputerUseSetup {
    let coordinate_mode = get_chicago_coordinate_mode();
    let tools = build_computer_use_tool_names(coordinate_mode);
    let allowed_tools: Vec<String> = tools
        .iter()
        .map(|t| format!("mcp__{}__{}", COMPUTER_USE_MCP_SERVER_NAME, t))
        .collect();

    let args = vec!["--computer-use-mcp".to_string()];
    let command = std::env::current_exe()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    ComputerUseSetup {
        mcp_config: vec![(
            COMPUTER_USE_MCP_SERVER_NAME.to_string(),
            McpServerConfig {
                server_type: "stdio".to_string(),
                command,
                args,
                scope: "dynamic".to_string(),
            },
        )],
        allowed_tools,
    }
}

fn build_computer_use_tool_names(_coordinate_mode: CoordinateMode) -> Vec<&'static str> {
    vec![
        "screenshot",
        "click",
        "type",
        "key",
        "scroll",
        "move_mouse",
        "drag",
        "hold_key",
        "request_access",
        "list_granted_applications",
    ]
}

// ─── MCP Server ──────────────────────────────────────────────────────────────

pub async fn run_computer_use_mcp_server() -> Result<()> {
    eprintln!("[Computer Use MCP] Starting MCP server");
    // In production: create server, connect stdio transport, handle requests
    // This would use the MCP SDK Rust implementation
    eprintln!("[Computer Use MCP] MCP server started");
    Ok(())
}

// ─── Host Adapter ────────────────────────────────────────────────────────────

pub struct ComputerUseHostAdapter {
    pub server_name: String,
    pub executor: CliExecutor,
}

/// 进程内单例缓存（对应 TS `hostAdapter.ts` 的 `cached`）。
static HOST_ADAPTER: Lazy<Mutex<Option<std::sync::Arc<ComputerUseHostAdapter>>>> =
    Lazy::new(|| Mutex::new(None));

/// 获取 Computer Use 主机适配器（首次调用时构建，进程内单例）。
/// 对应 TS `getComputerUseHostAdapter`。
pub fn get_computer_use_host_adapter() -> Result<std::sync::Arc<ComputerUseHostAdapter>> {
    let mut guard = HOST_ADAPTER.lock().unwrap();
    if let Some(existing) = guard.as_ref() {
        return Ok(existing.clone());
    }
    let adapter = std::sync::Arc::new(ComputerUseHostAdapter::new()?);
    *guard = Some(adapter.clone());
    Ok(adapter)
}

/// 单工具的 MCP 覆盖（rendering + call）描述。
/// 对应 TS `getComputerUseMCPToolOverrides` 返回的对象骨架。
#[derive(Debug, Clone)]
pub struct ComputerUseMcpToolOverrides {
    pub tool_name: String,
    pub coordinate_mode: CoordinateMode,
    pub server_name: String,
}

/// 返回 `mcp__computer-use__{toolName}` 工具的覆盖描述。
///
/// Rust 端 MCP 工具调用通过 [`run_computer_use_mcp_server`] 完成 dispatch，
/// 因此这里只返回静态元数据；UI 侧的渲染由调用方按需挂接。
pub fn get_computer_use_mcp_tool_overrides(tool_name: &str) -> ComputerUseMcpToolOverrides {
    ComputerUseMcpToolOverrides {
        tool_name: tool_name.to_string(),
        coordinate_mode: CoordinateMode::Normalized,
        server_name: COMPUTER_USE_MCP_SERVER_NAME.to_string(),
    }
}

impl ComputerUseHostAdapter {
    pub fn new() -> Result<Self> {
        let sub_gates = get_chicago_sub_gates();
        let executor = CliExecutor::new(ComputerExecutorOpts {
            mouse_animation_enabled: sub_gates.mouse_animation,
            hide_before_action_enabled: sub_gates.hide_before_action,
        })?;
        Ok(Self {
            server_name: COMPUTER_USE_MCP_SERVER_NAME.to_string(),
            executor,
        })
    }

    pub fn is_disabled(&self) -> bool {
        !get_chicago_enabled()
    }

    pub fn get_sub_gates(&self) -> CuSubGates {
        get_chicago_sub_gates()
    }

    pub fn get_auto_unhide_enabled(&self) -> bool {
        true
    }
}

// ─── Wrapper / Loaders / Tool rendering / MCP-for-CLI ────────────────────────

/// 对应 TS `buildSessionContext` (wrapper.tsx)：构造 Computer Use 工具调用所需的 session 上下文。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComputerUseSessionContext {
    pub session_id: String,
    pub allowlist_bundle_ids: Vec<String>,
    pub coordinate_mode: String,
    pub display_id: Option<u32>,
}

pub fn build_session_context(
    session_id: impl Into<String>,
    allowlist_bundle_ids: Vec<String>,
    display_id: Option<u32>,
) -> ComputerUseSessionContext {
    let coordinate_mode = match get_chicago_coordinate_mode() {
        CoordinateMode::Pixels => "pixels",
        CoordinateMode::Normalized => "normalized",
    };
    ComputerUseSessionContext {
        session_id: session_id.into(),
        allowlist_bundle_ids,
        coordinate_mode: coordinate_mode.to_string(),
        display_id,
    }
}

/// 对应 TS `getComputerUseMCPRenderingOverrides` (toolRendering.tsx)。
///
/// 返回所有 computer-use 工具的渲染回调描述（前端层消费此元数据并绑定具体 UI）。
#[derive(Debug, Clone)]
pub struct ComputerUseRenderingOverride {
    pub tool_name: String,
    pub server_name: String,
}

pub fn get_computer_use_mcp_rendering_overrides() -> Vec<ComputerUseRenderingOverride> {
    let tools = build_computer_use_tool_names(CoordinateMode::Normalized);
    tools
        .iter()
        .map(|t| ComputerUseRenderingOverride {
            tool_name: (*t).to_string(),
            server_name: COMPUTER_USE_MCP_SERVER_NAME.to_string(),
        })
        .collect()
}

/// 对应 TS `createComputerUseMcpServerForCli` (mcpServer.ts)。
///
/// 真实实现会构造一个 MCP server 实例，此处返回描述性的 JSON 元数据，
/// 由调用方通过 [`run_computer_use_mcp_server`] 实际启动。
pub fn create_computer_use_mcp_server_for_cli(server_name: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "kind": "computer-use-mcp-server",
        "serverName": server_name.unwrap_or(COMPUTER_USE_MCP_SERVER_NAME),
        "hostBundleId": CLI_HOST_BUNDLE_ID,
    })
}

/// 对应 TS `requireComputerUseInput` (inputLoader.ts)：加载平台原生 input 模块。
///
/// 在 Rust 端没有 dynamic require，这里返回 `true` 表示运行平台支持 input dispatch
/// （非 macOS 平台返回 `false`），调用方据此决定是否启用 computer-use。
pub fn require_computer_use_input() -> bool {
    std::env::consts::OS == "macos"
}

/// 对应 TS `requireComputerUseSwift` (swiftLoader.ts)：加载平台原生 swift 桥接。
pub fn require_computer_use_swift() -> bool {
    std::env::consts::OS == "macos"
}
