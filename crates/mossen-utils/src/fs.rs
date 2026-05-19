//! File system operations.
//!
//! Provides async and sync file reading, path existence checks, file
//! modification time, and encoding-aware writing.

use std::path::Path;

use tokio::fs;

// ---------------------------------------------------------------------------
// Existence and metadata
// ---------------------------------------------------------------------------

/// Check if a path exists asynchronously.
pub async fn path_exists(path: &Path) -> bool {
    fs::metadata(path).await.is_ok()
}

/// Check if a path exists synchronously.
pub fn path_exists_sync(path: &Path) -> bool {
    path.exists()
}

/// Get normalized modification time in milliseconds (floored for consistency).
pub async fn get_file_modification_time(path: &Path) -> anyhow::Result<u64> {
    let metadata = fs::metadata(path).await?;
    let modified = metadata.modified()?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_millis() as u64)
}

/// Synchronous variant of `get_file_modification_time`.
pub fn get_file_modification_time_sync(path: &Path) -> anyhow::Result<u64> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata.modified()?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_millis() as u64)
}

// ---------------------------------------------------------------------------
// File reading
// ---------------------------------------------------------------------------

/// Read a file to string, returning `None` on any error.
pub fn read_file_safe(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Read a file to string asynchronously, returning `None` on any error.
pub async fn read_file_safe_async(path: &Path) -> Option<String> {
    fs::read_to_string(path).await.ok()
}

/// Read a file to string asynchronously, returning an error on failure.
pub async fn read_file(path: &Path) -> anyhow::Result<String> {
    Ok(fs::read_to_string(path).await?)
}

/// Read a file to bytes asynchronously.
pub async fn read_file_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
    Ok(fs::read(path).await?)
}

// ---------------------------------------------------------------------------
// File writing
// ---------------------------------------------------------------------------

/// Line ending style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

/// Write text content to a file, converting line endings as needed.
pub async fn write_text_content(
    path: &Path,
    content: &str,
    line_ending: LineEnding,
) -> anyhow::Result<()> {
    let to_write = match line_ending {
        LineEnding::Lf => content.to_string(),
        LineEnding::CrLf => {
            // Normalize to LF first, then convert to CRLF
            content.replace("\r\n", "\n").replace('\n', "\r\n")
        }
    };
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(path, to_write.as_bytes()).await?;
    Ok(())
}

/// Write bytes to a file, ensuring parent directory exists.
pub async fn write_file(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(path, content).await?;
    Ok(())
}

/// Write a string to a file synchronously.
pub fn write_file_sync(path: &Path, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Directory operations
// ---------------------------------------------------------------------------

/// Ensure a directory exists, creating it and all parents if needed.
pub async fn ensure_dir(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path).await?;
    Ok(())
}

/// Ensure a directory exists (synchronous).
pub fn ensure_dir_sync(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// File size formatting
// ---------------------------------------------------------------------------

/// Maximum output size constant (0.25 MB).
pub const MAX_OUTPUT_SIZE: usize = 256 * 1024;
