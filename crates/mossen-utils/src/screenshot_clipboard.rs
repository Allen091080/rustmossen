//! # screenshot_clipboard — 截图到剪贴板
//!
//! 对应 TypeScript `utils/screenshotClipboard.ts`。
//! 将 ANSI 文本渲染为 PNG 并复制到系统剪贴板。
//! 支持 macOS、Linux（xclip/xsel）和 Windows。

use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::error;

use crate::exec_file_no_throw::{exec_file_no_throw_with_cwd, ExecFileOptions};
use crate::platform::get_platform;

/// 剪贴板操作结果
#[derive(Debug, Clone)]
pub struct ClipboardResult {
    pub success: bool,
    pub message: String,
}

/// ANSI 转 PNG 选项
#[derive(Debug, Clone, Default)]
pub struct AnsiToPngOptions {
    pub font_size: Option<u32>,
    pub padding: Option<u32>,
}

/// 将 ANSI 文本复制到系统剪贴板（作为 PNG 图片）。
///
/// 纯 Rust 管线：ANSI text → 位图字体渲染 → PNG 编码。
/// 无 WASM、无系统字体，在所有构建（native 和 JS）中均可工作。
pub async fn copy_ansi_to_clipboard(
    ansi_text: &str,
    options: Option<&AnsiToPngOptions>,
) -> ClipboardResult {
    let result = copy_ansi_to_clipboard_inner(ansi_text, options).await;
    match result {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to copy screenshot: {}", e);
            ClipboardResult {
                success: false,
                message: format!("Failed to copy screenshot: {}", e),
            }
        }
    }
}

async fn copy_ansi_to_clipboard_inner(
    ansi_text: &str,
    _options: Option<&AnsiToPngOptions>,
) -> anyhow::Result<ClipboardResult> {
    let temp_dir = std::env::temp_dir().join("mossen-code-screenshots");
    fs::create_dir_all(&temp_dir).await?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let png_path = temp_dir.join(format!("screenshot-{}.png", timestamp));

    // Render ANSI to PNG using our ansi_to_png module
    let png_buffer = crate::ansi_to_png::ansi_to_png(ansi_text, Default::default());
    fs::write(&png_path, &png_buffer).await?;

    let result = copy_png_to_clipboard(&png_path).await;

    // Cleanup temp file (ignore errors)
    let _ = fs::remove_file(&png_path).await;

    Ok(result)
}

async fn copy_png_to_clipboard(png_path: &Path) -> ClipboardResult {
    let platform = get_platform();
    let png_path_str = png_path.to_string_lossy().to_string();
    let timeout = std::time::Duration::from_secs(5);

    match platform {
        crate::platform::Platform::Macos => {
            // macOS: Use osascript to copy PNG to clipboard
            let escaped_path = png_path_str.replace('\\', "\\\\").replace('"', "\\\"");
            let script = format!(
                "set the clipboard to (read (POSIX file \"{}\") as «class PNGf»)",
                escaped_path
            );
            let result = exec_file_no_throw_with_cwd(
                "osascript",
                &["-e", &script],
                ExecFileOptions {
                    timeout,
                    ..Default::default()
                },
            )
            .await;

            if result.code == 0 {
                ClipboardResult {
                    success: true,
                    message: "Screenshot copied to clipboard".to_string(),
                }
            } else {
                ClipboardResult {
                    success: false,
                    message: format!("Failed to copy to clipboard: {}", result.stderr),
                }
            }
        }
        crate::platform::Platform::Linux => {
            // Linux: Try xclip first, then xsel
            let xclip_result = exec_file_no_throw_with_cwd(
                "xclip",
                &["-selection", "clipboard", "-t", "image/png", "-i", &png_path_str],
                ExecFileOptions {
                    timeout,
                    ..Default::default()
                },
            )
            .await;

            if xclip_result.code == 0 {
                return ClipboardResult {
                    success: true,
                    message: "Screenshot copied to clipboard".to_string(),
                };
            }

            // Try xsel as fallback
            let xsel_result = exec_file_no_throw_with_cwd(
                "xsel",
                &["--clipboard", "--input", "--type", "image/png"],
                ExecFileOptions {
                    timeout,
                    ..Default::default()
                },
            )
            .await;

            if xsel_result.code == 0 {
                return ClipboardResult {
                    success: true,
                    message: "Screenshot copied to clipboard".to_string(),
                };
            }

            ClipboardResult {
                success: false,
                message: "Failed to copy to clipboard. Please install xclip or xsel: sudo apt install xclip".to_string(),
            }
        }
        crate::platform::Platform::Windows => {
            // Windows: Use PowerShell to copy image to clipboard
            let escaped_path = png_path_str.replace('\'', "''");
            let ps_script = format!(
                "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Clipboard]::SetImage([System.Drawing.Image]::FromFile('{}'))",
                escaped_path
            );
            let result = exec_file_no_throw_with_cwd(
                "powershell",
                &["-NoProfile", "-Command", &ps_script],
                ExecFileOptions {
                    timeout,
                    ..Default::default()
                },
            )
            .await;

            if result.code == 0 {
                ClipboardResult {
                    success: true,
                    message: "Screenshot copied to clipboard".to_string(),
                }
            } else {
                ClipboardResult {
                    success: false,
                    message: format!("Failed to copy to clipboard: {}", result.stderr),
                }
            }
        }
        other => ClipboardResult {
            success: false,
            message: format!("Screenshot to clipboard is not supported on {:?}", other),
        },
    }
}
