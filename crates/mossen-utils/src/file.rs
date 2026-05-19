use std::collections::HashMap;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use tokio::fs;

/// Represents a file with name and content.
#[derive(Debug, Clone)]
pub struct File {
    pub filename: String,
    pub content: String,
}

/// Maximum output size: 0.25MB in bytes.
pub const MAX_OUTPUT_SIZE: u64 = 256 * 1024;

/// Check if a path exists asynchronously.
pub async fn path_exists(path: &Path) -> bool {
    fs::metadata(path).await.is_ok()
}

/// Read a file safely, returning None on error.
pub fn read_file_safe(filepath: &Path) -> Option<String> {
    std::fs::read_to_string(filepath).ok()
}

/// Get the normalized modification time of a file in milliseconds.
pub fn get_file_modification_time(file_path: &Path) -> std::io::Result<u64> {
    let metadata = std::fs::metadata(file_path)?;
    let mtime = metadata.modified()?;
    let duration = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_millis() as u64)
}

/// Async variant of get_file_modification_time.
pub async fn get_file_modification_time_async(file_path: &Path) -> std::io::Result<u64> {
    let metadata = fs::metadata(file_path).await?;
    let mtime = metadata.modified()?;
    let duration = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_millis() as u64)
}

/// Line ending type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEndingType {
    LF,
    CRLF,
}

/// Write text content with specified encoding and line endings.
pub fn write_text_content(
    file_path: &Path,
    content: &str,
    endings: LineEndingType,
) -> std::io::Result<()> {
    let to_write = match endings {
        LineEndingType::LF => content.to_string(),
        LineEndingType::CRLF => {
            // Normalize to LF first, then convert to CRLF
            content.replace("\r\n", "\n").replace('\n', "\r\n")
        }
    };
    write_file_sync_and_flush(file_path, &to_write)
}

/// Detect line endings in a file.
pub fn detect_line_endings(file_path: &Path) -> LineEndingType {
    match std::fs::read_to_string(file_path) {
        Ok(content) => detect_line_endings_for_string(&content),
        Err(_) => LineEndingType::LF,
    }
}

/// Detect line endings from a string.
pub fn detect_line_endings_for_string(content: &str) -> LineEndingType {
    // Check first 4096 chars
    let sample = if content.len() > 4096 {
        &content[..4096]
    } else {
        content
    };

    if sample.contains("\r\n") {
        LineEndingType::CRLF
    } else {
        LineEndingType::LF
    }
}

/// Convert leading tabs to spaces (2 spaces per tab).
pub fn convert_leading_tabs_to_spaces(content: &str) -> String {
    if !content.contains('\t') {
        return content.to_string();
    }

    static TAB_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\t+").unwrap());

    TAB_REGEX
        .replace_all(content, |caps: &regex::Captures| "  ".repeat(caps[0].len()))
        .to_string()
}

/// Get absolute and relative paths.
pub fn get_absolute_and_relative_paths(
    path: Option<&str>,
    cwd: &Path,
) -> (Option<PathBuf>, Option<String>) {
    let absolute_path = path.map(|p| expand_path(p));
    let relative_path = absolute_path
        .as_ref()
        .and_then(|abs| pathdiff::diff_paths(abs, cwd))
        .map(|p| p.to_string_lossy().to_string());
    (absolute_path, relative_path)
}

/// Get display path (relative if under cwd, tilde if under home, otherwise absolute).
pub fn get_display_path(file_path: &Path, cwd: &Path) -> String {
    // Try relative path
    if let Some(rel) = pathdiff::diff_paths(file_path, cwd) {
        let rel_str = rel.to_string_lossy();
        if !rel_str.starts_with("..") {
            return rel_str.to_string();
        }
    }

    // Try tilde notation
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = file_path.strip_prefix(&home) {
            return format!("~/{}", stripped.to_string_lossy());
        }
    }

    file_path.to_string_lossy().to_string()
}

/// Find files with the same name but different extensions in the same directory.
pub fn find_similar_file(file_path: &Path) -> Option<String> {
    let dir = file_path.parent()?;
    let stem = file_path.file_stem()?.to_string_lossy().to_string();

    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path == file_path {
            continue;
        }
        if let Some(entry_stem) = entry_path.file_stem() {
            if entry_stem.to_string_lossy() == stem {
                return entry_path.file_name().map(|n| n.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Marker included in file-not-found error messages.
pub const FILE_NOT_FOUND_CWD_NOTE: &str = "Note: your current working directory is";

/// Suggests a corrected path under the current working directory.
pub async fn suggest_path_under_cwd(
    requested_path: &Path,
    cwd: &Path,
) -> Option<PathBuf> {
    let cwd_parent = cwd.parent()?;

    // Resolve symlinks in the requested path's parent directory
    let resolved_path = if let Some(parent) = requested_path.parent() {
        match fs::canonicalize(parent).await {
            Ok(resolved_dir) => {
                if let Some(file_name) = requested_path.file_name() {
                    resolved_dir.join(file_name)
                } else {
                    requested_path.to_path_buf()
                }
            }
            Err(_) => requested_path.to_path_buf(),
        }
    } else {
        requested_path.to_path_buf()
    };

    // Only check if the requested path is under cwd's parent but not under cwd itself
    if !resolved_path.starts_with(cwd_parent)
        || resolved_path.starts_with(cwd)
        || resolved_path == cwd
    {
        return None;
    }

    // Get the relative path from the parent directory
    let rel_from_parent = pathdiff::diff_paths(&resolved_path, cwd_parent)?;

    // Check if the same relative path exists under cwd
    let corrected_path = cwd.join(&rel_from_parent);
    if fs::metadata(&corrected_path).await.is_ok() {
        Some(corrected_path)
    } else {
        None
    }
}

/// Whether compact line prefix format is enabled.
/// Returns true by default (killswitch pattern).
pub fn is_compact_line_prefix_enabled() -> bool {
    // Default: compact format enabled unless killswitch is set
    !std::env::var("TENGU_COMPACT_LINE_PREFIX_KILLSWITCH")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// Adds cat -n style line numbers to the content.
pub fn add_line_numbers(content: &str, start_line: usize) -> String {
    if content.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = content.split('\n').collect();

    if is_compact_line_prefix_enabled() {
        lines
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}\t{}", i + start_line, line))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let num_str = format!("{}", i + start_line);
                if num_str.len() >= 6 {
                    format!("{}\u{2192}{}", num_str, line)
                } else {
                    format!("{:>6}\u{2192}{}", num_str, line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

static LINE_NUMBER_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*\d+[\u{2192}\t](.*)$").unwrap());

/// Strips the line number prefix from a single line.
pub fn strip_line_number_prefix(line: &str) -> &str {
    if let Some(caps) = LINE_NUMBER_REGEX.captures(line) {
        if let Some(m) = caps.get(1) {
            return m.as_str();
        }
    }
    line
}

/// Checks if a directory is empty.
pub fn is_dir_empty(dir_path: &Path) -> bool {
    match std::fs::read_dir(dir_path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

/// Write to a file with atomic rename (write to temp then rename).
pub fn write_file_sync_and_flush(file_path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;

    // Check if the target is a symlink
    let target_path = match std::fs::read_link(file_path) {
        Ok(link_target) => {
            if link_target.is_absolute() {
                link_target
            } else if let Some(parent) = file_path.parent() {
                parent.join(&link_target)
            } else {
                link_target
            }
        }
        Err(_) => file_path.to_path_buf(),
    };

    // Get existing permissions
    let target_mode = std::fs::metadata(&target_path).ok().map(|m| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            m.permissions().mode()
        }
        #[cfg(not(unix))]
        {
            let _ = m;
            0o644u32
        }
    });

    // Try atomic write
    let temp_path = target_path.with_extension(format!(
        "tmp.{}.{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));

    match (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);

        // Apply permissions
        #[cfg(unix)]
        if let Some(mode) = target_mode {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(mode))?;
        }

        std::fs::rename(&temp_path, &target_path)?;
        Ok(())
    })() {
        Ok(()) => Ok(()),
        Err(_atomic_err) => {
            // Clean up temp file
            let _ = std::fs::remove_file(&temp_path);
            // Fallback to direct write
            let mut file = std::fs::File::create(&target_path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;

            #[cfg(unix)]
            if let Some(mode) = target_mode {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    &target_path,
                    std::fs::Permissions::from_mode(mode),
                )?;
            }

            Ok(())
        }
    }
}

/// Get the desktop path for the current platform.
pub fn get_desktop_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));

    #[cfg(target_os = "macos")]
    {
        return home.join("Desktop");
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let wsl_path = profile.replace('\\', "/");
            let wsl_path = wsl_path.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == ':');
            let desktop = PathBuf::from(format!("/mnt/c{}/Desktop", wsl_path));
            if desktop.exists() {
                return desktop;
            }
        }
        // Fallback
        let desktop = home.join("Desktop");
        if desktop.exists() {
            return desktop;
        }
        return home;
    }

    #[cfg(target_os = "linux")]
    {
        let desktop = home.join("Desktop");
        if desktop.exists() {
            return desktop;
        }
        return home;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let desktop = home.join("Desktop");
        if desktop.exists() {
            desktop
        } else {
            home
        }
    }
}

/// Validates that a file size is within the specified limit.
pub fn is_file_within_read_size_limit(file_path: &Path, max_size_bytes: u64) -> bool {
    match std::fs::metadata(file_path) {
        Ok(stats) => stats.len() <= max_size_bytes,
        Err(_) => false,
    }
}

/// Normalize a file path for comparison.
pub fn normalize_path_for_comparison(file_path: &Path) -> String {
    let normalized = dunce::canonicalize(file_path)
        .unwrap_or_else(|_| file_path.to_path_buf());

    #[cfg(target_os = "windows")]
    {
        normalized
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase()
    }

    #[cfg(not(target_os = "windows"))]
    {
        normalized.to_string_lossy().to_string()
    }
}

/// Compare two file paths for equality.
pub fn paths_equal(path1: &Path, path2: &Path) -> bool {
    normalize_path_for_comparison(path1) == normalize_path_for_comparison(path2)
}

/// Expand path (resolve ~ and env vars).
fn expand_path(path: &str) -> PathBuf {
    let expanded = shellexpand::tilde(path);
    PathBuf::from(expanded.as_ref())
}

/// 对应 TS `detectFileEncoding`：识别文件编码。
///
/// 简单实现：检测 BOM 标记，否则返回 `"utf-8"`。
pub fn detect_file_encoding(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return "utf-8";
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return "utf-16le";
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return "utf-16be";
    }
    "utf-8"
}

static FILE_READ_CACHE: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

/// 对应 TS `readFileSyncCached`：带进程级 LRU 风格缓存的同步读取。
pub fn read_file_sync_cached(path: &str) -> std::io::Result<String> {
    if let Some(content) = FILE_READ_CACHE.lock().unwrap().get(path).cloned() {
        return Ok(content);
    }
    let content = std::fs::read_to_string(path)?;
    FILE_READ_CACHE
        .lock()
        .unwrap()
        .insert(path.to_string(), content.clone());
    Ok(content)
}
