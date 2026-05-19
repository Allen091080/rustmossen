use std::path::{Path, PathBuf};
use regex::Regex;

/// File representation.
#[derive(Debug, Clone)]
pub struct File {
    pub filename: String,
    pub content: String,
}

/// Maximum output size (0.25MB).
pub const MAX_OUTPUT_SIZE: u64 = 256 * 1024;

/// Marker included in file-not-found error messages that contain a cwd note.
pub const FILE_NOT_FOUND_CWD_NOTE: &str = "Note: your current working directory is";

/// Line ending type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEndingType {
    LF,
    CRLF,
}

/// Check if a path exists asynchronously.
pub async fn path_exists(path: &str) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

/// Read a file safely, returning None on error.
pub fn read_file_safe(filepath: &str) -> Option<String> {
    std::fs::read_to_string(filepath).ok()
}

/// Get the normalized modification time of a file in milliseconds.
pub fn get_file_modification_time(file_path: &str) -> std::io::Result<u64> {
    let metadata = std::fs::metadata(file_path)?;
    let modified = metadata.modified()?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_millis() as u64)
}

/// Async variant of get_file_modification_time.
pub async fn get_file_modification_time_async(file_path: &str) -> std::io::Result<u64> {
    let metadata = tokio::fs::metadata(file_path).await?;
    let modified = metadata.modified()?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_millis() as u64)
}

/// Write text content to a file with encoding and line ending handling.
pub fn write_text_content(
    file_path: &str,
    content: &str,
    endings: LineEndingType,
) -> std::io::Result<()> {
    let to_write = match endings {
        LineEndingType::CRLF => {
            let normalized = content.replace("\r\n", "\n");
            normalized.replace('\n', "\r\n")
        }
        LineEndingType::LF => content.to_string(),
    };

    write_file_sync_and_flush(file_path, &to_write)
}

/// Detect line endings in a file.
pub fn detect_line_endings(file_path: &str) -> LineEndingType {
    match std::fs::read_to_string(file_path) {
        Ok(content) => detect_line_endings_for_string(&content),
        Err(_) => LineEndingType::LF,
    }
}

/// Detect line endings in a string.
pub fn detect_line_endings_for_string(content: &str) -> LineEndingType {
    if content.contains("\r\n") {
        LineEndingType::CRLF
    } else {
        LineEndingType::LF
    }
}

/// Convert leading tabs to spaces.
pub fn convert_leading_tabs_to_spaces(content: &str) -> String {
    if !content.contains('\t') {
        return content.to_string();
    }
    let re = Regex::new(r"(?m)^\t+").unwrap();
    re.replace_all(content, |caps: &regex::Captures| {
        "  ".repeat(caps[0].len())
    })
    .to_string()
}

/// Get absolute and relative paths for a given path.
pub fn get_absolute_and_relative_paths(
    path: Option<&str>,
    cwd: &str,
) -> (Option<PathBuf>, Option<String>) {
    let path = match path {
        Some(p) => p,
        None => return (None, None),
    };
    let absolute_path = expand_path(path, cwd);
    let relative_path = Path::new(&absolute_path)
        .strip_prefix(cwd)
        .ok()
        .map(|p| p.to_string_lossy().to_string());
    (Some(PathBuf::from(absolute_path)), relative_path)
}

/// Get a display-friendly path.
pub fn get_display_path(file_path: &str, cwd: &str) -> String {
    let (_, relative) = get_absolute_and_relative_paths(Some(file_path), cwd);
    if let Some(ref rel) = relative {
        if !rel.starts_with("..") {
            return rel.clone();
        }
    }

    // Use tilde notation for files in home directory
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if file_path.starts_with(home_str.as_ref()) {
            return format!("~{}", &file_path[home_str.len()..]);
        }
    }

    file_path.to_string()
}

/// Find files with the same name but different extensions in the same directory.
pub fn find_similar_file(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    let dir = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy().to_string();

    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if let Some(entry_stem) = entry_path.file_stem() {
            if entry_stem.to_string_lossy() == stem
                && entry_path.to_string_lossy() != file_path
            {
                return entry_path.file_name().map(|n| n.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Suggest a corrected path under the current working directory.
pub async fn suggest_path_under_cwd(
    requested_path: &str,
    cwd: &str,
) -> Option<String> {
    let cwd_parent = Path::new(cwd).parent()?.to_string_lossy().to_string();

    // Resolve symlinks in requested path's parent
    let resolved_path = match tokio::fs::canonicalize(Path::new(requested_path).parent().unwrap_or(Path::new(""))).await {
        Ok(resolved_dir) => {
            let filename = Path::new(requested_path).file_name().unwrap_or_default();
            resolved_dir.join(filename).to_string_lossy().to_string()
        }
        Err(_) => requested_path.to_string(),
    };

    let cwd_parent_prefix = if cwd_parent == "/" {
        "/".to_string()
    } else {
        format!("{}/", cwd_parent)
    };

    if !resolved_path.starts_with(&cwd_parent_prefix)
        || resolved_path.starts_with(&format!("{}/", cwd))
        || resolved_path == cwd
    {
        return None;
    }

    // Get relative path from parent directory
    let rel_from_parent = Path::new(&resolved_path)
        .strip_prefix(&cwd_parent)
        .ok()?
        .to_string_lossy()
        .to_string();

    let corrected_path = Path::new(cwd).join(&rel_from_parent);
    if tokio::fs::metadata(&corrected_path).await.is_ok() {
        Some(corrected_path.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Whether compact line prefix format is enabled.
pub fn is_compact_line_prefix_enabled() -> bool {
    // Default: compact format enabled (killswitch off)
    true
}

/// Add line numbers to content.
pub fn add_line_numbers(content: &str, start_line: usize) -> String {
    if content.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = content.split('\n').collect();

    if is_compact_line_prefix_enabled() {
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| format!("{}\t{}", index + start_line, line))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let num_str = (index + start_line).to_string();
                if num_str.len() >= 6 {
                    format!("{}→{}", num_str, line)
                } else {
                    format!("{:>6}→{}", num_str, line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Strip line number prefix from a single line.
pub fn strip_line_number_prefix(line: &str) -> &str {
    let re = Regex::new(r"^\s*\d+[\u{2192}\t](.*)$").unwrap();
    if let Some(caps) = re.captures(line) {
        caps.get(1).map_or(line, |m| m.as_str())
    } else {
        line
    }
}

/// Check if a directory is empty.
pub fn is_dir_empty(dir_path: &str) -> bool {
    match std::fs::read_dir(dir_path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

/// Validate that a file size is within the specified limit.
pub fn is_file_within_read_size_limit(file_path: &str, max_size_bytes: u64) -> bool {
    match std::fs::metadata(file_path) {
        Ok(metadata) => metadata.len() <= max_size_bytes,
        Err(_) => false,
    }
}

/// Normalize a file path for comparison.
pub fn normalize_path_for_comparison(file_path: &str) -> String {
    let normalized = Path::new(file_path)
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .to_string();

    #[cfg(windows)]
    {
        normalized.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        normalized
    }
}

/// Compare two file paths for equality.
pub fn paths_equal(path1: &str, path2: &str) -> bool {
    normalize_path_for_comparison(path1) == normalize_path_for_comparison(path2)
}

/// Get the desktop path for the current platform.
pub fn get_desktop_path() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        let desktop = home.join("Desktop");
        if desktop.exists() {
            return desktop;
        }
        return home;
    }
    PathBuf::from(".")
}

/// Write a file synchronously with flush (atomic write with temp file).
fn write_file_sync_and_flush(file_path: &str, content: &str) -> std::io::Result<()> {
    let path = Path::new(file_path);
    let target_path = resolve_symlink(file_path);

    // Check existing permissions
    let target_mode = std::fs::metadata(&target_path).ok().map(|m| m.permissions());

    // Try atomic write
    let temp_path = format!(
        "{}.tmp.{}.{}",
        target_path,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );

    match std::fs::write(&temp_path, content) {
        Ok(_) => {
            // Preserve permissions
            if let Some(mode) = target_mode {
                let _ = std::fs::set_permissions(&temp_path, mode);
            }
            // Atomic rename
            match std::fs::rename(&temp_path, &target_path) {
                Ok(_) => Ok(()),
                Err(e) => {
                    // Clean up temp file
                    let _ = std::fs::remove_file(&temp_path);
                    // Fallback to non-atomic write
                    std::fs::write(&target_path, content)
                }
            }
        }
        Err(e) => {
            // Fallback to non-atomic write
            std::fs::write(&target_path, content)
        }
    }
}

/// Resolve a symlink to its target path.
fn resolve_symlink(path: &str) -> String {
    match std::fs::read_link(path) {
        Ok(target) => {
            if target.is_absolute() {
                target.to_string_lossy().to_string()
            } else {
                let dir = Path::new(path).parent().unwrap_or(Path::new(""));
                dir.join(target).to_string_lossy().to_string()
            }
        }
        Err(_) => path.to_string(),
    }
}

/// Expand a path (resolve ~ and relative paths).
fn expand_path(path: &str, cwd: &str) -> String {
    if Path::new(path).is_absolute() {
        path.to_string()
    } else if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            format!("{}{}", home.display(), &path[1..])
        } else {
            path.to_string()
        }
    } else {
        Path::new(cwd)
            .join(path)
            .to_string_lossy()
            .to_string()
    }
}
