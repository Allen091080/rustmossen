//! Image Paste
//!
//! Handles clipboard image detection, extraction, and image file path parsing.
//! Supports macOS, Linux, and Windows platforms.

use std::path::{Path, PathBuf};

use regex::Regex;
use once_cell::sync::Lazy;
use tracing::{debug, warn};

/// Threshold in characters for large paste detection.
pub const PASTE_THRESHOLD: usize = 800;

/// Maximum image dimensions and target raw size for API limits.
pub const IMAGE_MAX_WIDTH: u32 = 2048;
pub const IMAGE_MAX_HEIGHT: u32 = 2048;
pub const IMAGE_TARGET_RAW_SIZE: usize = 3_750_000;

/// Image with dimensions information.
#[derive(Debug, Clone)]
pub struct ImageDimensions {
    pub original_width: u32,
    pub original_height: u32,
    pub display_width: u32,
    pub display_height: u32,
}

/// Image data with optional dimensions.
#[derive(Debug, Clone)]
pub struct ImageWithDimensions {
    pub base64: String,
    pub media_type: String,
    pub dimensions: Option<ImageDimensions>,
}

/// Image data with source path.
#[derive(Debug, Clone)]
pub struct ImageWithPath {
    pub path: String,
    pub base64: String,
    pub media_type: String,
    pub dimensions: Option<ImageDimensions>,
}

/// Supported platform enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedPlatform {
    Darwin,
    Linux,
    Win32,
}

impl SupportedPlatform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Darwin
        } else if cfg!(target_os = "windows") {
            Self::Win32
        } else {
            Self::Linux
        }
    }
}

/// Clipboard commands for a given platform.
struct ClipboardCommands {
    check_image: String,
    save_image: String,
    get_path: String,
    delete_file: String,
}

fn get_clipboard_commands() -> (ClipboardCommands, PathBuf) {
    let platform = SupportedPlatform::current();
    let base_tmp_dir = std::env::var("MOSSEN_CODE_TMPDIR").unwrap_or_else(|_| {
        if platform == SupportedPlatform::Win32 {
            std::env::var("TEMP").unwrap_or_else(|_| "C:\\Temp".to_string())
        } else {
            "/tmp".to_string()
        }
    });
    let screenshot_filename = "mossen_cli_latest_screenshot.png";
    let screenshot_path = PathBuf::from(&base_tmp_dir).join(screenshot_filename);
    let sp = screenshot_path.to_string_lossy().to_string();

    let commands = match platform {
        SupportedPlatform::Darwin => ClipboardCommands {
            check_image: "osascript -e 'the clipboard as «class PNGf»'".to_string(),
            save_image: format!(
                "osascript -e 'set png_data to (the clipboard as «class PNGf»)' -e 'set fp to open for access POSIX file \"{}\" with write permission' -e 'write png_data to fp' -e 'close access fp'",
                sp
            ),
            get_path: "osascript -e 'get POSIX path of (the clipboard as «class furl»)'".to_string(),
            delete_file: format!("rm -f \"{}\"", sp),
        },
        SupportedPlatform::Linux => ClipboardCommands {
            check_image: "xclip -selection clipboard -t TARGETS -o 2>/dev/null | grep -E \"image/(png|jpeg|jpg|gif|webp|bmp)\" || wl-paste -l 2>/dev/null | grep -E \"image/(png|jpeg|jpg|gif|webp|bmp)\"".to_string(),
            save_image: format!(
                "xclip -selection clipboard -t image/png -o > \"{}\" 2>/dev/null || wl-paste --type image/png > \"{}\" 2>/dev/null || xclip -selection clipboard -t image/bmp -o > \"{}\" 2>/dev/null || wl-paste --type image/bmp > \"{}\"",
                sp, sp, sp, sp
            ),
            get_path: "xclip -selection clipboard -t text/plain -o 2>/dev/null || wl-paste 2>/dev/null".to_string(),
            delete_file: format!("rm -f \"{}\"", sp),
        },
        SupportedPlatform::Win32 => ClipboardCommands {
            check_image: "powershell -NoProfile -Command \"(Get-Clipboard -Format Image) -ne $null\"".to_string(),
            save_image: format!(
                "powershell -NoProfile -Command \"$img = Get-Clipboard -Format Image; if ($img) {{ $img.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png) }}\"",
                sp.replace('\\', "\\\\")
            ),
            get_path: "powershell -NoProfile -Command \"Get-Clipboard\"".to_string(),
            delete_file: format!("del /f \"{}\"", sp),
        },
    };

    (commands, screenshot_path)
}

/// Regex for supported image file extensions.
static IMAGE_EXTENSION_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\.(png|jpe?g|gif|webp)$").unwrap());

/// Remove outer single or double quotes from a string.
fn remove_outer_quotes(text: &str) -> &str {
    if (text.starts_with('"') && text.ends_with('"'))
        || (text.starts_with('\'') && text.ends_with('\''))
    {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

/// Remove shell escape backslashes from a path (for macOS/Linux/WSL).
fn strip_backslash_escapes(path: &str) -> String {
    if cfg!(target_os = "windows") {
        return path.to_string();
    }

    // Replace double backslashes with placeholder
    let salt = format!("{:016x}", rand::random::<u64>());
    let placeholder = format!("__DOUBLE_BACKSLASH_{}__", salt);
    let with_placeholder = path.replace("\\\\", &placeholder);

    // Remove single backslashes (shell escapes)
    let re = Regex::new(r"\\(.)").unwrap();
    let without_escapes = re.replace_all(&with_placeholder, "$1").to_string();

    // Replace placeholders back to single backslashes
    without_escapes.replace(&placeholder, "\\")
}

/// Check if a given text represents an image file path.
pub fn is_image_file_path(text: &str) -> bool {
    let cleaned = remove_outer_quotes(text.trim());
    let unescaped = strip_backslash_escapes(cleaned);
    IMAGE_EXTENSION_REGEX.is_match(&unescaped)
}

/// Clean and normalize a text string that might be an image file path.
/// Returns None if not an image path.
pub fn as_image_file_path(text: &str) -> Option<String> {
    let cleaned = remove_outer_quotes(text.trim());
    let unescaped = strip_backslash_escapes(cleaned);
    if IMAGE_EXTENSION_REGEX.is_match(&unescaped) {
        Some(unescaped)
    } else {
        None
    }
}

/// Check if clipboard contains an image (macOS only via osascript).
pub async fn has_image_in_clipboard() -> bool {
    if SupportedPlatform::current() != SupportedPlatform::Darwin {
        return false;
    }

    let output = tokio::process::Command::new("osascript")
        .args(["-e", "the clipboard as «class PNGf»"])
        .output()
        .await;

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Get an image from the clipboard.
pub async fn get_image_from_clipboard() -> Option<ImageWithDimensions> {
    let (commands, screenshot_path) = get_clipboard_commands();

    // Check if clipboard has image
    let check_result = tokio::process::Command::new("sh")
        .args(["-c", &commands.check_image])
        .output()
        .await
        .ok()?;

    if !check_result.status.success() {
        return None;
    }

    // Save the image
    let save_result = tokio::process::Command::new("sh")
        .args(["-c", &commands.save_image])
        .output()
        .await
        .ok()?;

    if !save_result.status.success() {
        return None;
    }

    // Read the saved image
    let image_buffer = tokio::fs::read(&screenshot_path).await.ok()?;
    if image_buffer.is_empty() {
        return None;
    }

    let base64_image = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_buffer);
    let media_type = detect_image_format_from_bytes(&image_buffer);

    // Cleanup (fire-and-forget)
    let _ = tokio::process::Command::new("sh")
        .args(["-c", &commands.delete_file])
        .spawn();

    Some(ImageWithDimensions {
        base64: base64_image,
        media_type,
        dimensions: None,
    })
}

/// Get image path from clipboard text content.
pub async fn get_image_path_from_clipboard() -> Option<String> {
    let (commands, _) = get_clipboard_commands();

    let result = tokio::process::Command::new("sh")
        .args(["-c", &commands.get_path])
        .output()
        .await
        .ok()?;

    if !result.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

/// Detect image format from raw bytes using magic bytes.
fn detect_image_format_from_bytes(data: &[u8]) -> String {
    if data.len() < 4 {
        return "image/png".to_string();
    }
    if data[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        "image/png".to_string()
    } else if data[0..2] == [0xFF, 0xD8] {
        "image/jpeg".to_string()
    } else if data[0..4] == [0x47, 0x49, 0x46, 0x38] {
        "image/gif".to_string()
    } else if data[0..4] == [0x52, 0x49, 0x46, 0x46] {
        "image/webp".to_string()
    } else {
        "image/png".to_string()
    }
}

/// Try to find and read an image file from a path string.
pub async fn try_read_image_from_path(text: &str) -> Option<ImageWithPath> {
    let cleaned_path = as_image_file_path(text)?;

    let image_buffer = if Path::new(&cleaned_path).is_absolute() {
        tokio::fs::read(&cleaned_path).await.ok()?
    } else {
        // Try clipboard path fallback
        let clipboard_path = get_image_path_from_clipboard().await?;
        let basename = Path::new(&cleaned_path)
            .file_name()?
            .to_string_lossy()
            .to_string();
        let clip_basename = Path::new(&clipboard_path)
            .file_name()?
            .to_string_lossy()
            .to_string();
        if basename == clip_basename {
            tokio::fs::read(&clipboard_path).await.ok()?
        } else {
            return None;
        }
    };

    if image_buffer.is_empty() {
        warn!("Image file is empty: {}", cleaned_path);
        return None;
    }

    let base64_image = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_buffer);
    let media_type = detect_image_format_from_bytes(&image_buffer);

    Some(ImageWithPath {
        path: cleaned_path,
        base64: base64_image,
        media_type,
        dimensions: None,
    })
}
