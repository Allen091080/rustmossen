//! Read-edit context: finds a needle in a file and returns surrounding context lines.
//!
//! Scans in 8KB chunks with straddle overlap so matches crossing chunk boundaries
//! are found. Capped at MAX_SCAN_BYTES. Handles CRLF normalization.

use std::io::SeekFrom;
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Chunk size for reading.
pub const CHUNK_SIZE: usize = 8 * 1024;
/// Maximum bytes to scan before giving up.
pub const MAX_SCAN_BYTES: usize = 10 * 1024 * 1024;

const NL: u8 = b'\n';

/// Result of reading edit context around a needle match.
#[derive(Debug, Clone)]
pub struct EditContext {
    /// Slice of the file: context_lines before/after the match, on line boundaries.
    pub content: String,
    /// 1-based line number of content's first line in the original file.
    pub line_offset: usize,
    /// True if MAX_SCAN_BYTES was hit without finding the needle.
    pub truncated: bool,
}

/// Finds `needle` in the file at `path` and returns a context-window slice
/// containing the match plus `context_lines` of surrounding context on each side.
///
/// Returns None on file-not-found (ENOENT). Returns `EditContext { truncated: true }`
/// if the needle isn't found within MAX_SCAN_BYTES.
pub async fn read_edit_context(
    path: &Path,
    needle: &str,
    context_lines: usize,
) -> std::io::Result<Option<EditContext>> {
    let mut file = match File::open(path).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    Ok(Some(scan_for_context(&mut file, needle, context_lines).await?))
}

/// Core scanning logic.
async fn scan_for_context(
    file: &mut File,
    needle: &str,
    context_lines: usize,
) -> std::io::Result<EditContext> {
    if needle.is_empty() {
        return Ok(EditContext {
            content: String::new(),
            line_offset: 1,
            truncated: false,
        });
    }

    let needle_lf = needle.as_bytes();
    let nl_count = needle_lf.iter().filter(|&&b| b == NL).count();
    let needle_crlf: Option<Vec<u8>> = if nl_count > 0 {
        Some(needle.replace('\n', "\r\n").into_bytes())
    } else {
        None
    };
    let overlap = needle_lf.len() + nl_count - 1;

    let mut buf = vec![0u8; CHUNK_SIZE + overlap];
    let mut pos: usize = 0;
    let mut lines_before_pos: usize = 0;
    let mut prev_tail: usize = 0;

    while pos < MAX_SCAN_BYTES {
        file.seek(SeekFrom::Start(pos as u64)).await?;
        let bytes_read = file.read(&mut buf[prev_tail..prev_tail + CHUNK_SIZE]).await?;
        if bytes_read == 0 {
            break;
        }
        let view_len = prev_tail + bytes_read;

        // Try LF needle first
        let mut match_at = index_of_within(&buf, needle_lf, view_len);
        let mut match_len = needle_lf.len();

        // If not found and has newlines, try CRLF variant
        if match_at.is_none() && nl_count > 0 {
            if let Some(ref crlf) = needle_crlf {
                match_at = index_of_within(&buf, crlf, view_len);
                match_len = crlf.len();
            }
        }

        if let Some(at) = match_at {
            let abs_match = pos - prev_tail + at;
            let lines_before_match =
                lines_before_pos + count_newlines(&buf, 0, at);
            return slice_context(
                file,
                abs_match,
                match_len,
                context_lines,
                lines_before_match,
            )
            .await;
        }

        pos += bytes_read;
        let next_tail = overlap.min(view_len);
        lines_before_pos += count_newlines(&buf, 0, view_len - next_tail);
        prev_tail = next_tail;
        buf.copy_within(view_len - prev_tail..view_len, 0);
    }

    Ok(EditContext {
        content: String::new(),
        line_offset: 1,
        truncated: pos >= MAX_SCAN_BYTES,
    })
}

/// 对应 TS `openForScan`：尝试以只读方式打开文件，失败返回 `None`。
///
/// TS 版本会吞掉 IO 错误并返回 `null`；Rust 端保持同样语义以便调用方做
/// `if let Some(f) = open_for_scan(...).await { ... }` 风格的判定。
pub async fn open_for_scan(path: &str) -> Option<File> {
    tokio::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .await
        .ok()
}

/// Reads the entire file up to MAX_SCAN_BYTES. Returns None if the file exceeds the cap.
pub async fn read_capped(file: &mut File) -> std::io::Result<Option<String>> {
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut total = 0usize;

    loop {
        if total == buf.len() {
            let new_len = (buf.len() * 2).min(MAX_SCAN_BYTES + CHUNK_SIZE);
            buf.resize(new_len, 0);
        }
        file.seek(SeekFrom::Start(total as u64)).await?;
        let bytes_read = file.read(&mut buf[total..]).await?;
        if bytes_read == 0 {
            break;
        }
        total += bytes_read;
        if total > MAX_SCAN_BYTES {
            return Ok(None);
        }
    }

    Ok(Some(normalize_crlf(&buf[..total])))
}

/// Find needle within buf[0..end). Returns None if not found or extends past end.
fn index_of_within(buf: &[u8], needle: &[u8], end: usize) -> Option<usize> {
    if needle.is_empty() || end < needle.len() {
        return None;
    }
    let search_end = end - needle.len() + 1;
    buf[..search_end]
        .windows(needle.len())
        .position(|w| w == needle)
}

fn count_newlines(buf: &[u8], start: usize, end: usize) -> usize {
    buf[start..end].iter().filter(|&&b| b == NL).count()
}

/// Decode bytes to UTF-8, normalizing CRLF only if CR is present.
fn normalize_crlf(buf: &[u8]) -> String {
    let s = String::from_utf8_lossy(buf).to_string();
    if s.contains('\r') {
        s.replace("\r\n", "\n")
    } else {
        s
    }
}

/// Given an absolute match offset, read ±context_lines around it and return
/// the decoded slice with its starting line number.
async fn slice_context(
    file: &mut File,
    match_start: usize,
    match_len: usize,
    context_lines: usize,
    lines_before_match: usize,
) -> std::io::Result<EditContext> {
    // Scan backward from match_start to find context_lines prior newlines.
    let back_chunk = match_start.min(CHUNK_SIZE);
    let mut back_buf = vec![0u8; back_chunk];
    let back_start = match_start - back_chunk;
    file.seek(SeekFrom::Start(back_start as u64)).await?;
    let back_read = file.read(&mut back_buf).await?;

    let mut ctx_start = match_start;
    let mut nl_seen = 0usize;
    for i in (0..back_read).rev() {
        if nl_seen > context_lines {
            break;
        }
        if back_buf[i] == NL {
            nl_seen += 1;
            if nl_seen > context_lines {
                break;
            }
        }
        ctx_start -= 1;
    }

    let walked_back = match_start - ctx_start;
    let line_offset = lines_before_match
        .saturating_sub(count_newlines(
            &back_buf,
            back_read.saturating_sub(walked_back),
            back_read,
        ))
        + 1;

    // Scan forward from match_end to find context_lines trailing newlines.
    let match_end = match_start + match_len;
    let mut fwd_buf = vec![0u8; CHUNK_SIZE];
    file.seek(SeekFrom::Start(match_end as u64)).await?;
    let fwd_read = file.read(&mut fwd_buf).await?;

    let mut ctx_end = match_end;
    nl_seen = 0;
    for i in 0..fwd_read {
        ctx_end += 1;
        if fwd_buf[i] == NL {
            nl_seen += 1;
            if nl_seen >= context_lines + 1 {
                break;
            }
        }
    }

    // Read the exact context range.
    let len = ctx_end - ctx_start;
    let mut out = vec![0u8; len];
    file.seek(SeekFrom::Start(ctx_start as u64)).await?;
    let out_read = file.read(&mut out).await?;

    Ok(EditContext {
        content: normalize_crlf(&out[..out_read]),
        line_offset,
        truncated: false,
    })
}
