//! # system_directories — 系统目录
//!
//! 对应 TypeScript `utils/systemDirectories.ts`。
//! 获取跨平台系统目录。

use std::path::PathBuf;

/// 系统目录
#[derive(Debug, Clone)]
pub struct SystemDirectories {
    pub home: String,
    pub desktop: String,
    pub documents: String,
    pub downloads: String,
}

/// 平台类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Macos,
    Linux,
    Wsl,
    Unknown,
}

/// 系统目录选项
pub struct SystemDirectoriesOptions {
    pub env: Option<std::collections::HashMap<String, String>>,
    pub homedir: Option<String>,
    pub platform: Option<Platform>,
}

/// 获取当前平台
fn get_platform() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::Macos
    } else if cfg!(target_os = "linux") {
        // 检查 WSL
        if std::env::var("WSL_DISTRO_NAME").is_ok() {
            Platform::Wsl
        } else {
            Platform::Linux
        }
    } else {
        Platform::Unknown
    }
}

/// 获取跨平台系统目录。
///
/// 处理 Windows、macOS、Linux 和 WSL 之间的差异。
pub fn get_system_directories(options: Option<&SystemDirectoriesOptions>) -> SystemDirectories {
    let platform = options
        .and_then(|o| o.platform)
        .unwrap_or_else(get_platform);

    let home_dir = options.and_then(|o| o.homedir.clone()).unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .to_string_lossy()
            .to_string()
    });

    let env_get = |key: &str| -> Option<String> {
        if let Some(opts) = options {
            if let Some(ref env) = opts.env {
                return env.get(key).cloned();
            }
        }
        std::env::var(key).ok()
    };

    let join = |base: &str, child: &str| -> String {
        let mut p = PathBuf::from(base);
        p.push(child);
        p.to_string_lossy().to_string()
    };

    // 默认路径
    let defaults = SystemDirectories {
        home: home_dir.clone(),
        desktop: join(&home_dir, "Desktop"),
        documents: join(&home_dir, "Documents"),
        downloads: join(&home_dir, "Downloads"),
    };

    match platform {
        Platform::Windows => {
            let user_profile = env_get("USERPROFILE").unwrap_or_else(|| home_dir.clone());
            SystemDirectories {
                home: home_dir,
                desktop: join(&user_profile, "Desktop"),
                documents: join(&user_profile, "Documents"),
                downloads: join(&user_profile, "Downloads"),
            }
        }
        Platform::Linux | Platform::Wsl => SystemDirectories {
            home: home_dir,
            desktop: env_get("XDG_DESKTOP_DIR").unwrap_or(defaults.desktop),
            documents: env_get("XDG_DOCUMENTS_DIR").unwrap_or(defaults.documents),
            downloads: env_get("XDG_DOWNLOAD_DIR").unwrap_or(defaults.downloads),
        },
        Platform::Macos | Platform::Unknown => defaults,
    }
}
