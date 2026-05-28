//! Line-oriented file reader with two code paths:
//! - Fast path (regular files < 10 MB): read all, split in memory.
//! - Streaming path (large files): streaming with line counting.

use std::path::Path;
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};

const FAST_PATH_MAX_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

/// Result of reading a file range.
#[derive(Debug, Clone)]
pub struct ReadFileRangeResult {
    pub content: String,
    pub line_count: usize,
    pub total_lines: usize,
    pub total_bytes: usize,
    pub read_bytes: usize,
    pub mtime_ms: u64,
    /// True when output was clipped to max_bytes under truncate mode.
    pub truncated_by_bytes: bool,
}

/// Error when a file exceeds the maximum allowed size.
#[derive(Debug, Error)]
#[error("File content ({size_in_bytes} bytes) exceeds maximum allowed size ({max_size_bytes} bytes). Use offset and limit parameters to read specific portions of the file, or search for specific content instead of reading the whole file.")]
pub struct FileTooLargeError {
    pub size_in_bytes: u64,
    pub max_size_bytes: u64,
}

/// Options for reading a file range.
#[derive(Debug, Clone, Default)]
pub struct ReadFileInRangeOptions {
    pub truncate_on_byte_limit: bool,
}

/// Read lines [offset, offset + max_lines) from a file.
pub async fn read_file_in_range(
    file_path: &Path,
    offset: usize,
    max_lines: Option<usize>,
    max_bytes: Option<u64>,
    options: Option<ReadFileInRangeOptions>,
) -> Result<ReadFileRangeResult, Box<dyn std::error::Error + Send + Sync>> {
    let truncate_on_byte_limit = options
        .as_ref()
        .map(|o| o.truncate_on_byte_limit)
        .unwrap_or(false);

    let metadata = fs::metadata(file_path).await?;

    if metadata.is_dir() {
        return Err(format!(
            "EISDIR: illegal operation on a directory, read '{}'",
            file_path.display()
        )
        .into());
    }

    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    if metadata.is_file() && metadata.len() < FAST_PATH_MAX_SIZE {
        if !truncate_on_byte_limit {
            if let Some(max) = max_bytes {
                if metadata.len() > max {
                    return Err(Box::new(FileTooLargeError {
                        size_in_bytes: metadata.len(),
                        max_size_bytes: max,
                    }));
                }
            }
        }

        let text = fs::read_to_string(file_path).await?;
        return Ok(read_file_in_range_fast(
            &text,
            mtime_ms,
            offset,
            max_lines,
            if truncate_on_byte_limit {
                max_bytes
            } else {
                None
            },
        ));
    }

    read_file_in_range_streaming(
        file_path,
        mtime_ms,
        offset,
        max_lines,
        max_bytes,
        truncate_on_byte_limit,
    )
    .await
}

/// Fast path — in-memory split.
fn read_file_in_range_fast(
    raw: &str,
    mtime_ms: u64,
    offset: usize,
    max_lines: Option<usize>,
    truncate_at_bytes: Option<u64>,
) -> ReadFileRangeResult {
    let end_line = max_lines.map(|m| offset + m).unwrap_or(usize::MAX);

    // Strip BOM
    let text = if raw.starts_with('\u{feff}') {
        &raw[3..]
    } else {
        raw
    };

    let mut selected_lines: Vec<&str> = Vec::new();
    let mut line_index: usize = 0;
    let mut selected_bytes: usize = 0;
    let mut truncated_by_bytes = false;

    let truncate_limit = truncate_at_bytes.map(|b| b as usize);

    for line_raw in text.split('\n') {
        let line = line_raw.strip_suffix('\r').unwrap_or(line_raw);

        if line_index >= offset && line_index < end_line && !truncated_by_bytes {
            if let Some(limit) = truncate_limit {
                let sep = if selected_lines.is_empty() { 0 } else { 1 };
                let next_bytes = selected_bytes + sep + line.len();
                if next_bytes > limit {
                    truncated_by_bytes = true;
                } else {
                    selected_bytes = next_bytes;
                    selected_lines.push(line);
                }
            } else {
                selected_lines.push(line);
            }
        }
        line_index += 1;
    }

    // The split iterator counts the last segment even if empty after final \n.
    // Adjust: if text ends with \n, the last empty element from split is already counted.
    let total_lines = line_index;

    let content = selected_lines.join("\n");
    let total_bytes = text.len();
    let read_bytes = content.len();

    ReadFileRangeResult {
        content,
        line_count: selected_lines.len(),
        total_lines,
        total_bytes,
        read_bytes,
        mtime_ms,
        truncated_by_bytes,
    }
}

/// Streaming path for large files.
async fn read_file_in_range_streaming(
    file_path: &Path,
    mtime_ms: u64,
    offset: usize,
    max_lines: Option<usize>,
    max_bytes: Option<u64>,
    truncate_on_byte_limit: bool,
) -> Result<ReadFileRangeResult, Box<dyn std::error::Error + Send + Sync>> {
    let file = fs::File::open(file_path).await?;
    let reader = BufReader::with_capacity(512 * 1024, file);
    let mut lines_reader = reader.lines();

    let end_line = max_lines.map(|m| offset + m).unwrap_or(usize::MAX);
    let mut current_line_index: usize = 0;
    let mut selected_lines: Vec<String> = Vec::new();
    let mut total_bytes_read: usize = 0;
    let mut selected_bytes: usize = 0;
    let mut truncated_by_bytes = false;
    let mut is_first_line = true;
    let mut effective_end_line = end_line;

    while let Some(line_result) = lines_reader.next_line().await? {
        let mut line = line_result;

        if is_first_line {
            is_first_line = false;
            if line.starts_with('\u{feff}') {
                line = line[3..].to_string();
            }
        }

        // Strip \r
        if line.ends_with('\r') {
            line.pop();
        }

        total_bytes_read += line.len() + 1; // +1 for newline

        if !truncate_on_byte_limit {
            if let Some(max) = max_bytes {
                if total_bytes_read as u64 > max {
                    return Err(Box::new(FileTooLargeError {
                        size_in_bytes: total_bytes_read as u64,
                        max_size_bytes: max,
                    }));
                }
            }
        }

        if current_line_index >= offset
            && current_line_index < effective_end_line
            && !truncated_by_bytes
        {
            if truncate_on_byte_limit {
                if let Some(max) = max_bytes {
                    let sep = if selected_lines.is_empty() { 0 } else { 1 };
                    let next_bytes = selected_bytes + sep + line.len();
                    if next_bytes > max as usize {
                        truncated_by_bytes = true;
                        effective_end_line = current_line_index;
                    } else {
                        selected_bytes = next_bytes;
                        selected_lines.push(line);
                    }
                } else {
                    selected_lines.push(line);
                }
            } else {
                selected_lines.push(line);
            }
        }

        current_line_index += 1;
    }

    let content = selected_lines.join("\n");
    let read_bytes = content.len();

    Ok(ReadFileRangeResult {
        content,
        line_count: selected_lines.len(),
        total_lines: current_line_index,
        total_bytes: total_bytes_read,
        read_bytes,
        mtime_ms,
        truncated_by_bytes,
    })
}
