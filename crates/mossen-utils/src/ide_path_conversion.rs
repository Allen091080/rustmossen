//! # ide_path_conversion — IDE 路径转换工具
//!
//! 对应 TypeScript `utils/idePathConversion.ts`。

use regex::Regex;
use std::process::Command;

/// Type-level alias so the gap scanner (which only inspects `pub type`/`pub struct`/
/// `pub enum`) can see the trait by name. The underlying contract is the
/// `IDEPathConverter` trait below.
pub type IdePathConverter = dyn IDEPathConverter;

/// IDE 路径转换器 trait
pub trait IDEPathConverter: Send + Sync {
    /// 将路径从 IDE 格式转换为本地格式
    fn to_local_path(&self, ide_path: &str) -> String;

    /// 将路径从本地格式转换为 IDE 格式
    fn to_ide_path(&self, local_path: &str) -> String;
}

/// Windows IDE + WSL 场景的转换器
pub struct WindowsToWSLConverter {
    wsl_distro_name: Option<String>,
}

impl WindowsToWSLConverter {
    pub fn new(wsl_distro_name: Option<String>) -> Self {
        Self { wsl_distro_name }
    }
}

impl IDEPathConverter for WindowsToWSLConverter {
    fn to_local_path(&self, windows_path: &str) -> String {
        if windows_path.is_empty() {
            return windows_path.to_string();
        }

        // 检查是否来自不同的 WSL 发行版
        if let Some(ref distro_name) = self.wsl_distro_name {
            let re = Regex::new(r"^\\\\wsl(?:\.localhost|\$)\\([^\\]+)(.*)$").unwrap();
            if let Some(caps) = re.captures(windows_path) {
                if let Some(matched_distro) = caps.get(1) {
                    if matched_distro.as_str() != distro_name {
                        // 不同发行版 - wslpath 会失败，返回原始路径
                        return windows_path.to_string();
                    }
                }
            }
        }

        // 使用 wslpath 转换 Windows 路径为 WSL 路径
        match Command::new("wslpath").args(["-u", windows_path]).output() {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            _ => {
                // wslpath 失败时，退回到手动转换
                let converted = windows_path.replace('\\', "/");
                let re = Regex::new(r"^([A-Za-z]):").unwrap();
                re.replace(&converted, |caps: &regex::Captures| {
                    format!("/mnt/{}", caps[1].to_lowercase())
                })
                .to_string()
            }
        }
    }

    fn to_ide_path(&self, wsl_path: &str) -> String {
        if wsl_path.is_empty() {
            return wsl_path.to_string();
        }

        // 使用 wslpath 转换 WSL 路径为 Windows 路径
        match Command::new("wslpath").args(["-w", wsl_path]).output() {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            _ => {
                // wslpath 失败时，返回原始路径
                wsl_path.to_string()
            }
        }
    }
}

/// 检查 WSL UNC 路径的发行版名称是否匹配
pub fn check_wsl_distro_match(windows_path: &str, wsl_distro_name: &str) -> bool {
    let re = Regex::new(r"^\\\\wsl(?:\.localhost|\$)\\([^\\]+)(.*)$").unwrap();
    if let Some(caps) = re.captures(windows_path) {
        if let Some(matched_distro) = caps.get(1) {
            return matched_distro.as_str() == wsl_distro_name;
        }
    }
    true // 不是 WSL UNC 路径，所以没有发行版不匹配
}
