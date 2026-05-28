//! # browser — 浏览器和路径打开工具
//!
//! 对应 TypeScript `utils/browser.ts`。
//! 使用系统默认处理器打开 URL 和文件路径。

use std::process::Command;

/// 验证 URL 格式和协议（仅允许 http/https）。
fn validate_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| format!("Invalid URL format: {}", url))?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "Invalid URL protocol: must use http:// or https://, got {}:",
            scheme
        ));
    }

    Ok(())
}

/// 使用系统默认处理器打开文件或文件夹路径。
/// macOS 使用 `open`，Windows 使用 `explorer`，Linux 使用 `xdg-open`。
pub async fn open_path(path: &str) -> bool {
    let result = if cfg!(target_os = "windows") {
        Command::new("explorer").arg(path).status()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(path).status()
    } else {
        Command::new("xdg-open").arg(path).status()
    };

    match result {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

/// 使用系统默认浏览器打开 URL。
/// 支持 BROWSER 环境变量覆盖。
pub async fn open_browser(url: &str) -> bool {
    if validate_url(url).is_err() {
        return false;
    }

    let browser_env = std::env::var("BROWSER").ok();

    let result = if cfg!(target_os = "windows") {
        if let Some(ref browser) = browser_env {
            Command::new(browser).arg(format!("\"{}\"", url)).status()
        } else {
            Command::new("rundll32").args(["url,OpenURL", url]).status()
        }
    } else {
        let command = browser_env
            .as_deref()
            .unwrap_or(if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            });
        Command::new(command).arg(url).status()
    };

    match result {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}
