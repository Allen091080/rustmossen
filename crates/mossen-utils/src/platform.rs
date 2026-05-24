//! # platform — 平台检测工具
//!
//! 对应 TypeScript `utils/platform.ts`。
//! 检测运行平台、WSL版本、Linux发行版信息和VCS工具。

use once_cell::sync::Lazy;
use std::path::Path;
use tokio::fs;

/// 支持的平台枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Macos,
    Windows,
    Wsl,
    Linux,
    Unknown,
}

pub const SUPPORTED_PLATFORMS: &[Platform] = &[Platform::Macos, Platform::Wsl];

/// 检测当前平台（缓存结果）
static PLATFORM: Lazy<Platform> = Lazy::new(|| detect_platform_impl());

fn detect_platform_impl() -> Platform {
    if cfg!(target_os = "macos") {
        return Platform::Macos;
    }

    if cfg!(target_os = "windows") {
        return Platform::Windows;
    }

    if cfg!(target_os = "linux") {
        // Check if running in WSL
        if let Ok(proc_version) = std::fs::read_to_string("/proc/version") {
            let lower = proc_version.to_lowercase();
            if lower.contains("microsoft") || lower.contains("wsl") {
                return Platform::Wsl;
            }
        }
        return Platform::Linux;
    }

    Platform::Unknown
}

pub fn get_platform() -> Platform {
    *PLATFORM
}

/// 获取 WSL 版本
static WSL_VERSION: Lazy<Option<String>> = Lazy::new(|| detect_wsl_version_impl());

fn detect_wsl_version_impl() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }

    let proc_version = std::fs::read_to_string("/proc/version").ok()?;

    // First check for explicit WSL version markers (e.g., "WSL2", "WSL3", etc.)
    let re = regex::Regex::new(r"(?i)WSL(\d+)").unwrap();
    if let Some(caps) = re.captures(&proc_version) {
        return caps.get(1).map(|m| m.as_str().to_string());
    }

    // If no explicit WSL version but contains Microsoft, assume WSL1
    if proc_version.to_lowercase().contains("microsoft") {
        return Some("1".to_string());
    }

    None
}

pub fn get_wsl_version() -> Option<&'static str> {
    WSL_VERSION.as_deref()
}

/// Linux 发行版信息
#[derive(Debug, Clone)]
pub struct LinuxDistroInfo {
    pub linux_distro_id: Option<String>,
    pub linux_distro_version: Option<String>,
    pub linux_kernel: Option<String>,
}

/// 获取 Linux 发行版信息
pub async fn get_linux_distro_info() -> Option<LinuxDistroInfo> {
    if !cfg!(target_os = "linux") {
        return None;
    }

    let mut result = LinuxDistroInfo {
        linux_distro_id: None,
        linux_distro_version: None,
        linux_kernel: None,
    };

    // Get kernel version
    if let Ok(uname) = nix::sys::utsname::uname() {
        result.linux_kernel = Some(uname.release().to_string_lossy().to_string());
    }

    // Parse /etc/os-release
    if let Ok(content) = fs::read_to_string("/etc/os-release").await {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("ID=") {
                result.linux_distro_id = Some(rest.trim_matches('"').to_string());
            } else if let Some(rest) = line.strip_prefix("VERSION_ID=") {
                result.linux_distro_version = Some(rest.trim_matches('"').to_string());
            }
        }
    }

    Some(result)
}

/// VCS 标记目录与对应名称
const VCS_MARKERS: &[(&str, &str)] = &[
    (".git", "git"),
    (".hg", "mercurial"),
    (".svn", "svn"),
    (".p4config", "perforce"),
    ("$tf", "tfs"),
    (".tfvc", "tfs"),
    (".jj", "jujutsu"),
    (".sl", "sapling"),
];

/// 检测指定目录中的版本控制系统
pub async fn detect_vcs(dir: Option<&Path>) -> Vec<String> {
    let mut detected = std::collections::HashSet::new();

    // Check for Perforce via env var
    if std::env::var("P4PORT").is_ok() {
        detected.insert("perforce".to_string());
    }

    let target_dir = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_dir().unwrap_or_default(),
    };

    if let Ok(mut entries) = fs::read_dir(&target_dir).await {
        let mut entry_names = std::collections::HashSet::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(name) = entry.file_name().to_str() {
                entry_names.insert(name.to_string());
            }
        }
        for (marker, vcs) in VCS_MARKERS {
            if entry_names.contains(*marker) {
                detected.insert(vcs.to_string());
            }
        }
    }

    detected.into_iter().collect()
}
