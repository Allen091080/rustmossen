//! # system_theme — 终端暗/亮模式检测
//!
//! 对应 TypeScript `utils/systemTheme.ts`。
//! 基于终端实际背景色（通过 OSC 11 查询）而非 OS 外观设置进行检测——
//! 暗色终端在亮色模式 OS 上应解析为 'dark'。

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;

use crate::theme::{ThemeName, ThemeSetting};

/// 系统主题类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemTheme {
    Dark,
    Light,
}

/// 模块级别缓存的系统主题
static CACHED_SYSTEM_THEME: Lazy<Mutex<Option<SystemTheme>>> = Lazy::new(|| Mutex::new(None));

/// 获取当前终端主题。首次检测后缓存；watcher 在实时变更时更新缓存。
pub fn get_system_theme_name() -> SystemTheme {
    let mut cached = CACHED_SYSTEM_THEME.lock();
    if let Some(theme) = *cached {
        return theme;
    }
    let detected = detect_from_color_fg_bg().unwrap_or(SystemTheme::Dark);
    *cached = Some(detected);
    detected
}

/// 更新缓存的终端主题。
/// 由 watcher 在 OSC 11 查询返回时调用，以保持非 React 调用点同步。
pub fn set_cached_system_theme(theme: SystemTheme) {
    let mut cached = CACHED_SYSTEM_THEME.lock();
    *cached = Some(theme);
}

/// 将 ThemeSetting（可能为 'auto'）解析为具体的 ThemeName。
pub fn resolve_theme_setting(setting: &ThemeSetting) -> ThemeName {
    match setting {
        ThemeSetting::Auto => match get_system_theme_name() {
            SystemTheme::Dark => ThemeName::Dark,
            SystemTheme::Light => ThemeName::Light,
        },
        ThemeSetting::Named(name) => *name,
    }
}

/// RGB 颜色值（0.0 - 1.0）
struct Rgb {
    r: f64,
    g: f64,
    b: f64,
}

/// 从 OSC 颜色响应数据字符串解析主题。
///
/// 接受 XParseColor 格式：
/// - `rgb:R/G/B` 其中每个分量是 1-4 个十六进制数字
/// - `#RRGGBB` / `#RRRRGGGGBBBB`
///
/// 对无法识别的格式返回 None。
pub fn theme_from_osc_color(data: &str) -> Option<SystemTheme> {
    let rgb = parse_osc_rgb(data)?;
    // ITU-R BT.709 relative luminance. Midpoint split: > 0.5 is light.
    let luminance = 0.2126 * rgb.r + 0.7152 * rgb.g + 0.0722 * rgb.b;
    if luminance > 0.5 {
        Some(SystemTheme::Light)
    } else {
        Some(SystemTheme::Dark)
    }
}

fn parse_osc_rgb(data: &str) -> Option<Rgb> {
    // rgb:RRRR/GGGG/BBBB — each component is 1-4 hex digits.
    // Some terminals append alpha (rgba:…/…/…/…); ignore it.
    let rgb_re = Regex::new(r"(?i)^rgba?:([0-9a-f]{1,4})/([0-9a-f]{1,4})/([0-9a-f]{1,4})").ok()?;
    if let Some(caps) = rgb_re.captures(data) {
        return Some(Rgb {
            r: hex_component(&caps[1]),
            g: hex_component(&caps[2]),
            b: hex_component(&caps[3]),
        });
    }

    // #RRGGBB or #RRRRGGGGBBBB — split into three equal hex runs.
    let hash_re = Regex::new(r"(?i)^#([0-9a-f]+)$").ok()?;
    if let Some(caps) = hash_re.captures(data) {
        let hex = &caps[1];
        if hex.len() % 3 == 0 {
            let n = hex.len() / 3;
            return Some(Rgb {
                r: hex_component(&hex[..n]),
                g: hex_component(&hex[n..2 * n]),
                b: hex_component(&hex[2 * n..]),
            });
        }
    }

    None
}

/// 将 1-4 位十六进制分量归一化到 [0, 1]。
fn hex_component(hex: &str) -> f64 {
    let max = 16u64.pow(hex.len() as u32) - 1;
    let value = u64::from_str_radix(hex, 16).unwrap_or(0);
    if max == 0 {
        0.0
    } else {
        value as f64 / max as f64
    }
}

/// 从 $COLORFGBG 环境变量同步检测初始主题猜测。
///
/// 格式为 `fg;bg`（或 `fg;other;bg`），值为 ANSI 颜色索引。
/// rxvt 约定：bg 0-6 或 8 为暗色；bg 7 和 9-15 为亮色。
/// 仅某些终端设置此变量（rxvt 系列、Konsole、开启选项的 iTerm2），
/// 所以这只是尽力而为的提示。
fn detect_from_color_fg_bg() -> Option<SystemTheme> {
    let colorfgbg = std::env::var("COLORFGBG").ok()?;
    let parts: Vec<&str> = colorfgbg.split(';').collect();
    let bg = parts.last()?;
    if bg.is_empty() {
        return None;
    }
    let bg_num: i32 = bg.parse().ok()?;
    if bg_num < 0 || bg_num > 15 {
        return None;
    }
    // 0-6 and 8 are dark ANSI colors; 7 (white) and 9-15 (bright) are light.
    if bg_num <= 6 || bg_num == 8 {
        Some(SystemTheme::Dark)
    } else {
        Some(SystemTheme::Light)
    }
}
