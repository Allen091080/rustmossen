use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::process::Command;

// ─── Constants ───────────────────────────────────────────────────────────────

pub const VERSION_RETENTION_COUNT: usize = 2;
const LOCK_STALE_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const FALLBACK_STALE_MS: u64 = 2 * 60 * 60 * 1000;
const DEFAULT_STALL_TIMEOUT_MS: u64 = 60_000;
const MAX_DOWNLOAD_RETRIES: u32 = 3;

pub const GCS_BUCKET_URL: &str =
    "https://storage.googleapis.com/cli-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/cli-releases";

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Homebrew,
    Winget,
    Pacman,
    Deb,
    Rpm,
    Apk,
    Mise,
    Asdf,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct SetupMessage {
    pub message: String,
    pub user_action_required: bool,
    pub msg_type: SetupMessageType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupMessageType {
    Path,
    Alias,
    Info,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionLockContent {
    pub pid: u32,
    pub version: String,
    pub exec_path: String,
    pub acquired_at: u64,
}

#[derive(Debug, Clone)]
pub struct LockInfo {
    pub version: String,
    pub pid: u32,
    pub is_process_running: bool,
    pub exec_path: String,
    pub acquired_at: u64,
    pub lock_file_path: PathBuf,
}

// ─── Platform Detection ──────────────────────────────────────────────────────

pub fn get_platform() -> String {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "win32"
    } else {
        "linux"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        panic!("Unsupported architecture");
    };

    if os == "linux" && is_musl_environment() {
        return format!("{}-{}-musl", os, arch);
    }

    format!("{}-{}", os, arch)
}

fn is_musl_environment() -> bool {
    #[cfg(target_os = "linux")]
    {
        // Check if running on musl by inspecting the dynamic linker
        if let Ok(output) = std::process::Command::new("ldd").arg("--version").output() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stderr.contains("musl") || stdout.contains("musl");
        }
    }
    false
}

pub fn get_binary_name(platform: &str) -> &'static str {
    if platform.starts_with("win32") {
        "mossen.exe"
    } else {
        "mossen"
    }
}

// ─── Directory Structure ─────────────────────────────────────────────────────

pub fn get_base_directories() -> BaseDirectories {
    let data_home = dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"));
    let cache_home =
        dirs::cache_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".cache"));
    let state_home = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/state"));
    let bin_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".local")
        .join("bin");

    let platform = get_platform();
    let binary_name = get_binary_name(&platform);

    BaseDirectories {
        versions: data_home.join("mossen").join("versions"),
        staging: cache_home.join("mossen").join("staging"),
        locks: state_home.join("mossen").join("locks"),
        executable: bin_dir.join(binary_name),
    }
}

pub struct BaseDirectories {
    pub versions: PathBuf,
    pub staging: PathBuf,
    pub locks: PathBuf,
    pub executable: PathBuf,
}

// ─── PID Lock ────────────────────────────────────────────────────────────────

pub fn is_pid_based_locking_enabled() -> bool {
    let env_var = std::env::var("ENABLE_PID_BASED_VERSION_LOCKING").unwrap_or_default();
    if env_var == "1" || env_var.to_lowercase() == "true" {
        return true;
    }
    if env_var == "0" || env_var.to_lowercase() == "false" {
        return false;
    }
    false
}

pub fn is_process_running(pid: u32) -> bool {
    if pid <= 1 {
        return false;
    }
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn is_mossen_process(pid: u32, expected_exec_path: &str) -> bool {
    if !is_process_running(pid) {
        return false;
    }
    if pid == std::process::id() {
        return true;
    }
    match get_process_command(pid) {
        Some(command) => {
            let normalized_cmd = command.to_lowercase();
            let normalized_path = expected_exec_path.to_lowercase();
            normalized_cmd.contains("mossen") || normalized_cmd.contains(&normalized_path)
        }
        None => true, // Conservative: trust PID check if can't get command
    }
}

fn get_process_command(pid: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
            .ok()?;
        if output.status.success() {
            return Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
    }
    #[cfg(target_os = "linux")]
    {
        let cmdline_path = format!("/proc/{}/cmdline", pid);
        if let Ok(content) = std::fs::read_to_string(&cmdline_path) {
            return Some(content.replace('\0', " ").trim().to_string());
        }
    }
    None
}

pub fn read_lock_content(lock_file_path: &Path) -> Option<VersionLockContent> {
    let content = std::fs::read_to_string(lock_file_path).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    let parsed: VersionLockContent = serde_json::from_str(&content).ok()?;
    if parsed.version.is_empty() || parsed.exec_path.is_empty() {
        return None;
    }
    Some(parsed)
}

pub fn is_lock_active(lock_file_path: &Path) -> bool {
    let content = match read_lock_content(lock_file_path) {
        Some(c) => c,
        None => return false,
    };

    if !is_process_running(content.pid) {
        return false;
    }

    if !is_mossen_process(content.pid, &content.exec_path) {
        eprintln!(
            "Lock PID {} is running but does not appear to be Mossen - treating as stale",
            content.pid
        );
        return false;
    }

    // Fallback age check
    if let Ok(metadata) = std::fs::metadata(lock_file_path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(age) = SystemTime::now().duration_since(modified) {
                if age.as_millis() as u64 > FALLBACK_STALE_MS && !is_process_running(content.pid) {
                    return false;
                }
            }
        }
    }
    true
}

fn write_lock_file(lock_file_path: &Path, content: &VersionLockContent) -> Result<()> {
    let temp_path = format!(
        "{}.tmp.{}.{}",
        lock_file_path.display(),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let json = serde_json::to_string_pretty(content)?;
    std::fs::write(&temp_path, &json)?;
    std::fs::rename(&temp_path, lock_file_path).inspect_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
    })?;
    Ok(())
}

pub async fn try_acquire_lock(
    version_path: &Path,
    lock_file_path: &Path,
) -> Option<Box<dyn FnOnce()>> {
    let version_name = version_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if is_lock_active(lock_file_path) {
        let existing = read_lock_content(lock_file_path);
        eprintln!(
            "Cannot acquire lock for {} - held by PID {:?}",
            version_name,
            existing.map(|c| c.pid)
        );
        return None;
    }

    let lock_content = VersionLockContent {
        pid: std::process::id(),
        version: version_name.clone(),
        exec_path: std::env::current_exe()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        acquired_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    };

    if let Err(e) = write_lock_file(lock_file_path, &lock_content) {
        eprintln!("Failed to acquire lock for {}: {}", version_name, e);
        return None;
    }

    // Verify we got the lock
    let verify = read_lock_content(lock_file_path);
    if verify.as_ref().map(|c| c.pid) != Some(std::process::id()) {
        return None;
    }

    eprintln!(
        "Acquired PID lock for {} (PID {})",
        version_name,
        std::process::id()
    );

    let lfp = lock_file_path.to_path_buf();
    let vn = version_name.clone();
    Some(Box::new(move || {
        if let Some(content) = read_lock_content(&lfp) {
            if content.pid == std::process::id() {
                let _ = std::fs::remove_file(&lfp);
                eprintln!("Released PID lock for {}", vn);
            }
        }
    }))
}

pub fn get_all_lock_info(locks_dir: &Path) -> Vec<LockInfo> {
    let mut infos = Vec::new();
    let entries = match std::fs::read_dir(locks_dir) {
        Ok(e) => e,
        Err(_) => return infos,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "lock").unwrap_or(false) {
            if let Some(content) = read_lock_content(&path) {
                infos.push(LockInfo {
                    version: content.version.clone(),
                    pid: content.pid,
                    is_process_running: is_process_running(content.pid),
                    exec_path: content.exec_path.clone(),
                    acquired_at: content.acquired_at,
                    lock_file_path: path,
                });
            }
        }
    }
    infos
}

pub fn cleanup_stale_locks(locks_dir: &Path) -> u32 {
    let mut cleaned = 0u32;
    let entries = match std::fs::read_dir(locks_dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().map(|e| e == "lock").unwrap_or(false) {
            continue;
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if metadata.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
                cleaned += 1;
                eprintln!("Cleaned up legacy directory lock: {:?}", path.file_name());
            } else if !is_lock_active(&path) {
                let _ = std::fs::remove_file(&path);
                cleaned += 1;
                eprintln!("Cleaned up stale lock: {:?}", path.file_name());
            }
        }
    }
    cleaned
}

// ─── Package Manager Detection ───────────────────────────────────────────────

pub fn detect_homebrew() -> bool {
    if cfg!(not(any(target_os = "macos", target_os = "linux"))) {
        return false;
    }
    let exec_path = std::env::current_exe()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    exec_path.contains("/Caskroom/")
}

pub fn detect_winget() -> bool {
    if cfg!(not(target_os = "windows")) {
        return false;
    }
    let exec_path = std::env::current_exe()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let winget_re = Regex::new(r"(?i)Microsoft[/\\]WinGet[/\\](Packages|Links)").unwrap();
    winget_re.is_match(&exec_path)
}

pub fn detect_mise() -> bool {
    let exec_path = std::env::current_exe()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let re = Regex::new(r"(?i)[/\\]mise[/\\]installs[/\\]").unwrap();
    re.is_match(&exec_path)
}

pub fn detect_asdf() -> bool {
    let exec_path = std::env::current_exe()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let re = Regex::new(r"(?i)[/\\]\.?asdf[/\\]installs[/\\]").unwrap();
    re.is_match(&exec_path)
}

pub async fn detect_pacman() -> bool {
    if cfg!(not(target_os = "linux")) {
        return false;
    }
    if let Some(os_release) = get_os_release().await {
        if !is_distro_family(&os_release, &["arch"]) {
            return false;
        }
    }
    let exec_path = std::env::current_exe().unwrap_or_default();
    let output = Command::new("pacman")
        .args(["-Qo", &exec_path.to_string_lossy()])
        .output()
        .await;
    output.map(|o| o.status.success()).unwrap_or(false)
}

pub async fn detect_deb() -> bool {
    if cfg!(not(target_os = "linux")) {
        return false;
    }
    if let Some(os_release) = get_os_release().await {
        if !is_distro_family(&os_release, &["debian"]) {
            return false;
        }
    }
    let exec_path = std::env::current_exe().unwrap_or_default();
    let output = Command::new("dpkg")
        .args(["-S", &exec_path.to_string_lossy()])
        .output()
        .await;
    output.map(|o| o.status.success()).unwrap_or(false)
}

pub async fn detect_rpm() -> bool {
    if cfg!(not(target_os = "linux")) {
        return false;
    }
    if let Some(os_release) = get_os_release().await {
        if !is_distro_family(&os_release, &["fedora", "rhel", "suse"]) {
            return false;
        }
    }
    let exec_path = std::env::current_exe().unwrap_or_default();
    let output = Command::new("rpm")
        .args(["-qf", &exec_path.to_string_lossy()])
        .output()
        .await;
    output.map(|o| o.status.success()).unwrap_or(false)
}

pub async fn detect_apk() -> bool {
    if cfg!(not(target_os = "linux")) {
        return false;
    }
    if let Some(os_release) = get_os_release().await {
        if !is_distro_family(&os_release, &["alpine"]) {
            return false;
        }
    }
    let exec_path = std::env::current_exe().unwrap_or_default();
    let output = Command::new("apk")
        .args(["info", "--who-owns", &exec_path.to_string_lossy()])
        .output()
        .await;
    output.map(|o| o.status.success()).unwrap_or(false)
}

#[derive(Debug, Clone)]
struct OsRelease {
    id: String,
    id_like: Vec<String>,
}

async fn get_os_release() -> Option<OsRelease> {
    let content = tokio::fs::read_to_string("/etc/os-release").await.ok()?;
    let id_re = Regex::new(r#"(?m)^ID=["']?(\S+?)["']?\s*$"#).unwrap();
    let id_like_re = Regex::new(r#"(?m)^ID_LIKE=["']?(.+?)["']?\s*$"#).unwrap();
    let id = id_re
        .captures(&content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let id_like: Vec<String> = id_like_re
        .captures(&content)
        .and_then(|c| c.get(1))
        .map(|m| {
            m.as_str()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    Some(OsRelease { id, id_like })
}

fn is_distro_family(os_release: &OsRelease, families: &[&str]) -> bool {
    families.contains(&os_release.id.as_str())
        || os_release
            .id_like
            .iter()
            .any(|like| families.contains(&like.as_str()))
}

pub async fn get_package_manager() -> PackageManager {
    if detect_homebrew() {
        return PackageManager::Homebrew;
    }
    if detect_winget() {
        return PackageManager::Winget;
    }
    if detect_mise() {
        return PackageManager::Mise;
    }
    if detect_asdf() {
        return PackageManager::Asdf;
    }
    if detect_pacman().await {
        return PackageManager::Pacman;
    }
    if detect_apk().await {
        return PackageManager::Apk;
    }
    if detect_deb().await {
        return PackageManager::Deb;
    }
    if detect_rpm().await {
        return PackageManager::Rpm;
    }
    PackageManager::Unknown
}

// ─── Download ────────────────────────────────────────────────────────────────

pub async fn get_latest_version(channel_or_version: &str) -> Result<String> {
    let version_re = Regex::new(r"^v?\d+\.\d+\.\d+(-\S+)?$").unwrap();
    if version_re.is_match(channel_or_version) {
        let normalized = if channel_or_version.starts_with('v') {
            &channel_or_version[1..]
        } else {
            channel_or_version
        };
        return Ok(normalized.to_string());
    }

    if channel_or_version != "stable" && channel_or_version != "latest" {
        return Err(anyhow!(
            "Invalid channel: {}. Use 'stable' or 'latest'",
            channel_or_version
        ));
    }

    get_latest_version_from_binary_repo(channel_or_version, GCS_BUCKET_URL).await
}

pub async fn get_latest_version_from_binary_repo(channel: &str, base_url: &str) -> Result<String> {
    let url = format!("{}/{}", base_url, channel);
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to fetch version from {}: HTTP {}",
            url,
            response.status()
        ));
    }
    let text = response.text().await?;
    Ok(text.trim().to_string())
}

pub async fn download_version_from_binary_repo(
    version: &str,
    staging_path: &Path,
    base_url: &str,
) -> Result<()> {
    let _ = fs::remove_dir_all(staging_path).await;
    let platform = get_platform();

    // Fetch manifest
    let manifest_url = format!("{}/{}/manifest.json", base_url, version);
    let client = reqwest::Client::new();
    let manifest_resp = client
        .get(&manifest_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    if !manifest_resp.status().is_success() {
        return Err(anyhow!(
            "Failed to fetch manifest from {}: HTTP {}",
            manifest_url,
            manifest_resp.status()
        ));
    }
    let manifest: serde_json::Value = manifest_resp.json().await?;

    let platform_info = manifest
        .get("platforms")
        .and_then(|p| p.get(&platform))
        .ok_or_else(|| {
            anyhow!(
                "Platform {} not found in manifest for version {}",
                platform,
                version
            )
        })?;

    let expected_checksum = platform_info
        .get("checksum")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("No checksum found in manifest"))?;

    let binary_name = get_binary_name(&platform);
    let binary_url = format!("{}/{}/{}/{}", base_url, version, platform, binary_name);

    fs::create_dir_all(staging_path).await?;
    let binary_path = staging_path.join(binary_name);

    download_and_verify_binary(&binary_url, expected_checksum, &binary_path).await
}

async fn download_and_verify_binary(
    url: &str,
    expected_checksum: &str,
    binary_path: &Path,
) -> Result<()> {
    let client = reqwest::Client::new();

    for attempt in 1..=MAX_DOWNLOAD_RETRIES {
        let response = client
            .get(url)
            .timeout(Duration::from_secs(300))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                let bytes = resp.bytes().await?;

                // Verify checksum
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                let actual_checksum = format!("{:x}", hasher.finalize());

                if actual_checksum != expected_checksum {
                    return Err(anyhow!(
                        "Checksum mismatch: expected {}, got {}",
                        expected_checksum,
                        actual_checksum
                    ));
                }

                fs::write(binary_path, &bytes).await?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(binary_path, std::fs::Permissions::from_mode(0o755))
                        .await?;
                }
                return Ok(());
            }
            Ok(resp) => {
                return Err(anyhow!("Download failed: HTTP {}", resp.status()));
            }
            Err(e) if attempt < MAX_DOWNLOAD_RETRIES => {
                eprintln!(
                    "Download attempt {}/{} failed, retrying: {}",
                    attempt, MAX_DOWNLOAD_RETRIES, e
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            Err(e) => {
                return Err(anyhow!("Download failed after all retries: {}", e));
            }
        }
    }
    unreachable!()
}

pub async fn download_version(version: &str, staging_path: &Path) -> Result<&'static str> {
    download_version_from_binary_repo(version, staging_path, GCS_BUCKET_URL).await?;
    Ok("binary")
}

// ─── Installer ───────────────────────────────────────────────────────────────

pub async fn install_latest(channel_or_version: &str) -> Result<Vec<SetupMessage>> {
    let version = get_latest_version(channel_or_version).await?;
    let dirs = get_base_directories();

    // Create directories
    fs::create_dir_all(&dirs.versions).await?;
    fs::create_dir_all(&dirs.staging).await?;
    fs::create_dir_all(&dirs.locks).await?;
    if let Some(parent) = dirs.executable.parent() {
        fs::create_dir_all(parent).await?;
    }

    let install_path = dirs.versions.join(&version);
    let staging_path = dirs.staging.join(&version);

    // Download
    eprintln!("Downloading version {}...", version);
    download_version(&version, &staging_path).await?;

    // Install from staging
    let platform = get_platform();
    let binary_name = get_binary_name(&platform);
    let staged_binary = staging_path.join(binary_name);

    // Atomic move to install path
    atomic_move_to_install_path(&staged_binary, &install_path).await?;

    // Clean staging
    let _ = fs::remove_dir_all(&staging_path).await;

    // Create/update symlink
    let _ = fs::remove_file(&dirs.executable).await;
    #[cfg(unix)]
    {
        tokio::fs::symlink(&install_path, &dirs.executable).await?;
    }

    let mut messages = Vec::new();
    messages.push(SetupMessage {
        message: format!("Successfully installed Mossen v{}", version),
        user_action_required: false,
        msg_type: SetupMessageType::Info,
    });

    // Check PATH
    let bin_dir = dirs
        .executable
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let path_var = std::env::var("PATH").unwrap_or_default();
    if !path_var.split(':').any(|p| p == bin_dir) {
        messages.push(SetupMessage {
            message: format!("Add {} to your PATH", bin_dir),
            user_action_required: true,
            msg_type: SetupMessageType::Path,
        });
    }

    Ok(messages)
}

async fn atomic_move_to_install_path(staged_binary: &Path, install_path: &Path) -> Result<()> {
    if let Some(parent) = install_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let temp_path = format!(
        "{}.tmp.{}.{}",
        install_path.display(),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    fs::copy(staged_binary, &temp_path).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755)).await?;
    }
    fs::rename(&temp_path, install_path)
        .await
        .inspect_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
        })?;
    eprintln!("Atomically installed binary to {:?}", install_path);
    Ok(())
}

pub async fn check_install() -> Result<Option<String>> {
    let dirs = get_base_directories();
    if !dirs.executable.exists() {
        return Ok(None);
    }
    // Read symlink target to get current version
    match fs::read_link(&dirs.executable).await {
        Ok(target) => {
            let version = target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            Ok(Some(version))
        }
        Err(_) => Ok(None),
    }
}

pub fn lock_current_version() -> Result<()> {
    let dirs = get_base_directories();
    let exec_path = std::env::current_exe()?;
    // Determine which version we are
    let version = exec_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let lock_path = dirs.locks.join(format!("{}.lock", version));
    let content = VersionLockContent {
        pid: std::process::id(),
        version,
        exec_path: exec_path.to_string_lossy().to_string(),
        acquired_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    };
    std::fs::create_dir_all(&dirs.locks)?;
    write_lock_file(&lock_path, &content)
}

pub async fn cleanup_old_versions() -> Result<u32> {
    let dirs = get_base_directories();
    let mut entries: Vec<(PathBuf, SystemTime)> = Vec::new();

    let mut read_dir = fs::read_dir(&dirs.versions).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if let Ok(meta) = fs::metadata(&path).await {
            if let Ok(modified) = meta.modified() {
                entries.push((path, modified));
            }
        }
    }

    // Sort by modified time descending (newest first)
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    let mut removed = 0u32;
    for (path, _) in entries.iter().skip(VERSION_RETENTION_COUNT) {
        let lock_name = format!(
            "{}.lock",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        let lock_path = dirs.locks.join(&lock_name);
        if is_lock_active(&lock_path) {
            continue;
        }
        if fs::remove_file(&path).await.is_ok() {
            removed += 1;
            eprintln!("Removed old version: {:?}", path.file_name());
        }
    }
    Ok(removed)
}

pub async fn remove_installed_symlink() -> Result<bool> {
    let dirs = get_base_directories();
    match fs::remove_file(&dirs.executable).await {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

pub async fn cleanup_npm_installations() -> Result<()> {
    // Remove global npm installations of mossen
    let _output = Command::new("npm")
        .args(["ls", "-g", "--depth=0", "--json"])
        .output()
        .await;
    // Best-effort cleanup
    Ok(())
}

pub async fn cleanup_shell_aliases() -> Result<Vec<SetupMessage>> {
    // Check shell config files for mossen aliases that need cleanup
    let mut messages = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();
    let shell_configs = vec![
        home.join(".bashrc"),
        home.join(".zshrc"),
        home.join(".bash_profile"),
        home.join(".profile"),
    ];

    for config_path in &shell_configs {
        if let Ok(content) = fs::read_to_string(config_path).await {
            let alias_re = Regex::new(r"(?m)^.*alias.*mossen.*$").unwrap();
            if alias_re.is_match(&content) {
                messages.push(SetupMessage {
                    message: format!(
                        "Found mossen alias in {:?} - consider removing it",
                        config_path.file_name()
                    ),
                    user_action_required: true,
                    msg_type: SetupMessageType::Alias,
                });
            }
        }
    }
    Ok(messages)
}

// =============================================================================
// 与 TS `nativeInstaller/installer.ts` / `nativeInstaller/download.ts` 对齐的
// 补充导出。
// =============================================================================

/// 对应 TS `removeDirectoryIfEmpty`：当目录为空时移除它，否则保持不变。
pub async fn remove_directory_if_empty(path: &str) -> std::io::Result<()> {
    let mut entries = fs::read_dir(path).await?;
    if entries.next_entry().await?.is_none() {
        fs::remove_dir(path).await?;
    }
    Ok(())
}

/// 对应 TS `ARTIFACTORY_REGISTRY_URL`：内部 npm registry URL。
pub const ARTIFACTORY_REGISTRY_URL: &str =
    "https://artifactory.corp.mossen.com/artifactory/api/npm/mossen-npm";

/// 对应 TS `STALL_TIMEOUT_MS`：下载阶段无进度的超时阈值。
pub const STALL_TIMEOUT_MS: u64 = 30_000;

/// 对应 TS `_downloadAndVerifyBinaryForTesting`：测试用入口，下载并验证 binary。
#[doc(hidden)]
pub async fn _download_and_verify_binary_for_testing(
    url: &str,
    expected_sha256: &str,
) -> anyhow::Result<Vec<u8>> {
    let bytes = reqwest::get(url).await?.bytes().await?;
    let actual = {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = hasher.finalize();
        hex::encode(digest)
    };
    if actual != expected_sha256 {
        anyhow::bail!("sha256 mismatch: expected {expected_sha256}, got {actual}");
    }
    Ok(bytes.to_vec())
}

/// 对应 TS `getLatestVersionFromArtifactory`：从内部 registry 抓取最新版本号。
pub async fn get_latest_version_from_artifactory() -> Option<String> {
    let url = format!("{}/@internal/cli", ARTIFACTORY_REGISTRY_URL);
    let body = reqwest::get(&url).await.ok()?.text().await.ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    v.get("dist-tags")
        .and_then(|d| d.get("latest"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}

/// 对应 TS `downloadVersionFromArtifactory`：下载指定版本的 tarball。
pub async fn download_version_from_artifactory(version: &str) -> anyhow::Result<Vec<u8>> {
    let url = format!(
        "{}/@internal/cli/-/cli-{}.tgz",
        ARTIFACTORY_REGISTRY_URL, version
    );
    let bytes = reqwest::get(&url).await?.bytes().await?;
    Ok(bytes.to_vec())
}

// ─── pidLock.ts: acquireProcessLifetimeLock / withLock ───────────────────────

/// 对应 TS `acquireProcessLifetimeLock`：写入并持有一个跟随进程生命周期的 lock 文件。
///
/// 返回 `true` 表示成功获取（文件已创建并记录当前 PID），`false` 表示锁已被其它活跃进程持有。
/// Rust 端不需要显式 release：进程退出时调用方可自行 unlink 文件，或依赖
/// [`cleanup_stale_locks`] 在下一次启动时清理。
pub async fn acquire_process_lifetime_lock(
    lock_file_path: &std::path::Path,
) -> anyhow::Result<bool> {
    use tokio::io::AsyncWriteExt;

    if let Some(parent) = lock_file_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Check if existing lock is active
    if is_lock_active(lock_file_path) {
        return Ok(false);
    }

    // Remove stale lock if present
    let _ = tokio::fs::remove_file(lock_file_path).await;

    let content = VersionLockContent {
        version: String::new(),
        pid: std::process::id(),
        exec_path: std::env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_default(),
        acquired_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    };
    let json = serde_json::to_string(&content)?;
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(lock_file_path)
        .await?;
    file.write_all(json.as_bytes()).await?;
    Ok(true)
}

/// 对应 TS `withLock`：在持锁状态下执行 `f`，结束后释放锁。
///
/// 失败（无法获取锁）时直接返回 `None`，不执行 `f`。
pub async fn with_lock<F, Fut, T>(
    lock_file_path: &std::path::Path,
    f: F,
) -> anyhow::Result<Option<T>>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    if !acquire_process_lifetime_lock(lock_file_path).await? {
        return Ok(None);
    }
    let result = f().await;
    let _ = tokio::fs::remove_file(lock_file_path).await;
    Ok(Some(result?))
}
