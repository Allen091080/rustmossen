//! # xdg — XDG Base Directory 工具
//!
//! 对应 TypeScript `utils/xdg.ts`。
//! 实现 XDG Base Directory 规范，用于组织原生安装器组件。
//!
//! @see https://specifications.freedesktop.org/basedir-spec/latest/

use std::path::PathBuf;

/// XDG 配置选项，可覆盖环境变量和 home 目录。
pub struct XdgOptions {
    pub home: Option<String>,
}

impl Default for XdgOptions {
    fn default() -> Self {
        Self { home: None }
    }
}

fn resolve_home(options: &XdgOptions) -> PathBuf {
    if let Some(ref home) = options.home {
        return PathBuf::from(home);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// 获取 XDG state home 目录。
/// 默认：~/.local/state
pub fn get_xdg_state_home(options: &XdgOptions) -> PathBuf {
    if let Ok(val) = std::env::var("XDG_STATE_HOME") {
        if !val.is_empty() {
            return PathBuf::from(val);
        }
    }
    resolve_home(options).join(".local").join("state")
}

/// 获取 XDG cache home 目录。
/// 默认：~/.cache
pub fn get_xdg_cache_home(options: &XdgOptions) -> PathBuf {
    if let Ok(val) = std::env::var("XDG_CACHE_HOME") {
        if !val.is_empty() {
            return PathBuf::from(val);
        }
    }
    resolve_home(options).join(".cache")
}

/// 获取 XDG data home 目录。
/// 默认：~/.local/share
pub fn get_xdg_data_home(options: &XdgOptions) -> PathBuf {
    if let Ok(val) = std::env::var("XDG_DATA_HOME") {
        if !val.is_empty() {
            return PathBuf::from(val);
        }
    }
    resolve_home(options).join(".local").join("share")
}

/// 获取用户 bin 目录（非严格 XDG 但遵循惯例）。
/// 默认：~/.local/bin
pub fn get_user_bin_dir(options: &XdgOptions) -> PathBuf {
    resolve_home(options).join(".local").join("bin")
}
