//! Portable session storage utilities.
//!
//! Pure Rust — no internal dependencies on logging, experiments, or feature
//! flags. Shared between the CLI and VS Code extension equivalents.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Size of the head/tail buffer for lite metadata reads.
pub const LITE_READ_BUF_SIZE: usize = 65536;

/// Maximum length for a single filesystem path component.
pub const MAX_SANITIZED_LENGTH: usize = 200;

/// File size below which precompact filtering is skipped.
pub const SKIP_PRECOMPACT_THRESHOLD: u64 = 5 * 1024 * 1024;

// --------------------------------------------------------------------------
// UUID validation
// --------------------------------------------------------------------------

/// Validate a string as a UUID.
pub fn validate_uuid(maybe_uuid: &str) -> Option<Uuid> {
    Uuid::parse_str(maybe_uuid).ok()
}

// --------------------------------------------------------------------------
// JSON string field extraction — no full parse, works on truncated lines
// --------------------------------------------------------------------------

/// Unescape a JSON string value extracted as raw text.
pub fn unescape_json_string(raw: &str) -> String {
    if !raw.contains('\\') {
        return raw.to_string();
    }
    // Try parsing as a JSON string
    let quoted = format!("\"{}\"", raw);
    serde_json::from_str::<String>(&quoted).unwrap_or_else(|_| raw.to_string())
}

/// Extracts a simple JSON string field value from raw text without full parsing.
pub fn extract_json_string_field(text: &str, key: &str) -> Option<String> {
    let patterns = [
        format!("\"{}\":\"", key),
        format!("\"{}\": \"", key),
    ];

    for pattern in &patterns {
        if let Some(idx) = text.find(pattern.as_str()) {
            let value_start = idx + pattern.len();
            let mut i = value_start;
            let bytes = text.as_bytes();
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    return Some(unescape_json_string(&text[value_start..i]));
                }
                i += 1;
            }
        }
    }
    None
}

/// Like extract_json_string_field but finds the LAST occurrence.
pub fn extract_last_json_string_field(text: &str, key: &str) -> Option<String> {
    let patterns = [
        format!("\"{}\":\"", key),
        format!("\"{}\": \"", key),
    ];

    let mut last_value: Option<String> = None;

    for pattern in &patterns {
        let mut search_from = 0;
        while let Some(idx) = text[search_from..].find(pattern.as_str()) {
            let abs_idx = search_from + idx;
            let value_start = abs_idx + pattern.len();
            let mut i = value_start;
            let bytes = text.as_bytes();
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    last_value = Some(unescape_json_string(&text[value_start..i]));
                    break;
                }
                i += 1;
            }
            search_from = i + 1;
        }
    }

    last_value
}

// --------------------------------------------------------------------------
// First prompt extraction
// --------------------------------------------------------------------------

/// Pattern for auto-generated/system messages to skip.
fn is_skip_first_prompt(text: &str) -> bool {
    let trimmed = text.trim_start();
    // Starts with lowercase XML-like tag
    if trimmed.starts_with('<') {
        if let Some(next_char) = trimmed.chars().nth(1) {
            if next_char.is_ascii_lowercase() {
                return true;
            }
        }
    }
    // Interrupt marker
    if trimmed.starts_with("[Request interrupted by user") {
        return true;
    }
    false
}

/// Extract command name from XML tag.
fn extract_command_name(text: &str) -> Option<&str> {
    let start = text.find("<command-name>")?;
    let content_start = start + "<command-name>".len();
    let end = text[content_start..].find("</command-name>")?;
    Some(&text[content_start..content_start + end])
}

/// Extract bash input from XML tag.
fn extract_bash_input(text: &str) -> Option<&str> {
    let start = text.find("<bash-input>")?;
    let content_start = start + "<bash-input>".len();
    let end = text[content_start..].find("</bash-input>")?;
    Some(&text[content_start..content_start + end])
}

/// Extracts the first meaningful user prompt from a JSONL head chunk.
pub fn extract_first_prompt_from_head(head: &str) -> String {
    let mut command_fallback = String::new();

    for line in head.lines() {
        if !line.contains("\"type\":\"user\"") && !line.contains("\"type\": \"user\"") {
            continue;
        }
        if line.contains("\"tool_result\"") {
            continue;
        }
        if line.contains("\"isMeta\":true") || line.contains("\"isMeta\": true") {
            continue;
        }
        if line.contains("\"isCompactSummary\":true") || line.contains("\"isCompactSummary\": true")
        {
            continue;
        }

        let entry: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if entry.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }

        let message = match entry.get("message") {
            Some(m) => m,
            None => continue,
        };

        let content = match message.get("content") {
            Some(c) => c,
            None => continue,
        };

        let texts: Vec<String> = if let Some(s) = content.as_str() {
            vec![s.to_string()]
        } else if let Some(arr) = content.as_array() {
            arr.iter()
                .filter_map(|block| {
                    if block.get("type")?.as_str()? == "text" {
                        Some(block.get("text")?.as_str()?.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            continue;
        };

        for raw in &texts {
            let result = raw.replace('\n', " ").trim().to_string();
            if result.is_empty() {
                continue;
            }

            // Slash-command fallback
            if let Some(cmd) = extract_command_name(&result) {
                if command_fallback.is_empty() {
                    command_fallback = cmd.to_string();
                }
                continue;
            }

            // Bash input formatting
            if let Some(bash) = extract_bash_input(&result) {
                return format!("! {}", bash.trim());
            }

            if is_skip_first_prompt(&result) {
                continue;
            }

            if result.len() > 200 {
                return format!("{}\u{2026}", result[..200].trim());
            }
            return result;
        }
    }

    command_fallback
}

// --------------------------------------------------------------------------
// File I/O — read head and tail of a file
// --------------------------------------------------------------------------

/// Lite session file data.
#[derive(Debug, Clone)]
pub struct LiteSessionFile {
    pub mtime: u64,
    pub size: u64,
    pub head: String,
    pub tail: String,
}

/// Read head and tail of a file.
pub async fn read_head_and_tail(
    file_path: &str,
    file_size: u64,
) -> (String, String) {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let file = match tokio::fs::File::open(file_path).await {
        Ok(f) => f,
        Err(_) => return (String::new(), String::new()),
    };

    let mut file = file;
    let mut buf = vec![0u8; LITE_READ_BUF_SIZE];

    let bytes_read = match file.read(&mut buf).await {
        Ok(n) => n,
        Err(_) => return (String::new(), String::new()),
    };

    if bytes_read == 0 {
        return (String::new(), String::new());
    }

    let head = String::from_utf8_lossy(&buf[..bytes_read]).to_string();

    let tail_offset = file_size.saturating_sub(LITE_READ_BUF_SIZE as u64);
    let tail = if tail_offset > 0 {
        match file.seek(std::io::SeekFrom::Start(tail_offset)).await {
            Ok(_) => {
                let n = file.read(&mut buf).await.unwrap_or(0);
                String::from_utf8_lossy(&buf[..n]).to_string()
            }
            Err(_) => head.clone(),
        }
    } else {
        head.clone()
    };

    (head, tail)
}

/// Read a single session file (lite metadata).
pub async fn read_session_lite(file_path: &str) -> Option<LiteSessionFile> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(file_path).await.ok()?;
    let meta = file.metadata().await.ok()?;

    let mut buf = vec![0u8; LITE_READ_BUF_SIZE];
    let bytes_read = file.read(&mut buf).await.ok()?;
    if bytes_read == 0 {
        return None;
    }

    let head = String::from_utf8_lossy(&buf[..bytes_read]).to_string();

    let tail_offset = meta.len().saturating_sub(LITE_READ_BUF_SIZE as u64);
    let tail = if tail_offset > 0 {
        file.seek(std::io::SeekFrom::Start(tail_offset)).await.ok()?;
        let n = file.read(&mut buf).await.ok()?;
        String::from_utf8_lossy(&buf[..n]).to_string()
    } else {
        head.clone()
    };

    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;

    Some(LiteSessionFile {
        mtime,
        size: meta.len(),
        head,
        tail,
    })
}

// --------------------------------------------------------------------------
// Path sanitization
// --------------------------------------------------------------------------

/// djb2 hash for path sanitization.
fn djb2_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

/// Makes a string safe for use as a directory or file name.
pub fn sanitize_path(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    if sanitized.len() <= MAX_SANITIZED_LENGTH {
        return sanitized;
    }

    let hash = djb2_hash(name);
    let hash_str = format!("{}", radix_fmt(hash, 36));
    format!("{}-{}", &sanitized[..MAX_SANITIZED_LENGTH], hash_str)
}

/// Format a number in a given radix (base 36).
fn radix_fmt(mut value: u64, radix: u32) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut result = String::new();
    while value > 0 {
        let digit = (value % radix as u64) as u32;
        let c = char::from_digit(digit, radix).unwrap_or('0');
        result.push(c);
        value /= radix as u64;
    }
    result.chars().rev().collect()
}

// --------------------------------------------------------------------------
// Project directory discovery
// --------------------------------------------------------------------------

/// Get the projects directory path.
pub fn get_projects_dir(config_home: &str) -> PathBuf {
    Path::new(config_home).join("projects")
}

/// Get a specific project directory.
pub fn get_project_dir(config_home: &str, project_dir: &str) -> PathBuf {
    get_projects_dir(config_home).join(sanitize_path(project_dir))
}

/// Canonicalize a path (resolve symlinks + NFC normalize).
pub async fn canonicalize_path(dir: &str) -> String {
    match tokio::fs::canonicalize(dir).await {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => dir.to_string(),
    }
}

/// Find the project directory for a given path, tolerating hash mismatches.
pub async fn find_project_dir(
    config_home: &str,
    project_path: &str,
) -> Option<PathBuf> {
    let exact = get_project_dir(config_home, project_path);
    if tokio::fs::read_dir(&exact).await.is_ok() {
        return Some(exact);
    }

    // For short paths, exact miss means no sessions
    let sanitized = sanitize_path(project_path);
    if sanitized.len() <= MAX_SANITIZED_LENGTH {
        return None;
    }

    // Try prefix matching for long paths
    let prefix = &sanitized[..MAX_SANITIZED_LENGTH];
    let projects_dir = get_projects_dir(config_home);
    let mut entries = tokio::fs::read_dir(&projects_dir).await.ok()?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(ft) = entry.file_type().await {
            if ft.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&format!("{}-", prefix)) {
                    return Some(projects_dir.join(name));
                }
            }
        }
    }

    None
}

/// Resolve a sessionId to its on-disk JSONL file path.
pub async fn resolve_session_file_path(
    config_home: &str,
    session_id: &str,
    dir: Option<&str>,
) -> Option<SessionFilePath> {
    let file_name = format!("{}.jsonl", session_id);

    if let Some(dir) = dir {
        let canonical = canonicalize_path(dir).await;
        if let Some(project_dir) = find_project_dir(config_home, &canonical).await {
            let file_path = project_dir.join(&file_name);
            if let Ok(meta) = tokio::fs::metadata(&file_path).await {
                if meta.len() > 0 {
                    return Some(SessionFilePath {
                        file_path,
                        project_path: Some(canonical),
                        file_size: meta.len(),
                    });
                }
            }
        }
        return None;
    }

    // No dir — scan all project directories
    let projects_dir = get_projects_dir(config_home);
    let mut entries = match tokio::fs::read_dir(&projects_dir).await {
        Ok(e) => e,
        Err(_) => return None,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let file_path = entry.path().join(&file_name);
        if let Ok(meta) = tokio::fs::metadata(&file_path).await {
            if meta.len() > 0 {
                return Some(SessionFilePath {
                    file_path,
                    project_path: None,
                    file_size: meta.len(),
                });
            }
        }
    }

    None
}

/// Resolved session file path info.
#[derive(Debug, Clone)]
pub struct SessionFilePath {
    pub file_path: PathBuf,
    pub project_path: Option<String>,
    pub file_size: u64,
}

// --------------------------------------------------------------------------
// Compact-boundary chunked read
// --------------------------------------------------------------------------

/// Chunk size for the forward transcript reader.
pub const TRANSCRIPT_READ_CHUNK_SIZE: usize = 1024 * 1024;

/// Compact boundary marker.
const COMPACT_BOUNDARY_MARKER: &[u8] = b"\"compact_boundary\"";

/// Result of reading a transcript for load.
#[derive(Debug)]
pub struct TranscriptLoadResult {
    pub boundary_start_offset: usize,
    pub post_boundary_buf: Vec<u8>,
    pub has_preserved_segment: bool,
}

/// Parse a potential compact boundary line.
fn parse_boundary_line(line: &str) -> Option<bool> {
    let parsed: serde_json::Value = serde_json::from_str(line).ok()?;
    if parsed.get("type")?.as_str()? != "system" {
        return None;
    }
    if parsed.get("subtype")?.as_str()? != "compact_boundary" {
        return None;
    }
    let has_preserved = parsed
        .get("compactMetadata")
        .and_then(|cm| cm.get("preservedSegment"))
        .is_some();
    Some(has_preserved)
}

/// Read transcript for load with compact boundary handling.
pub async fn read_transcript_for_load(
    file_path: &str,
    file_size: u64,
) -> std::io::Result<TranscriptLoadResult> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(file_path).await?;
    let mut output = Vec::with_capacity((file_size as usize).min(8 * 1024 * 1024));
    let mut boundary_start_offset = 0usize;
    let mut has_preserved_segment = false;

    let chunk_size = TRANSCRIPT_READ_CHUNK_SIZE;
    let mut chunk = vec![0u8; chunk_size];
    let mut file_pos = 0u64;
    let mut remainder = Vec::new();

    while file_pos < file_size {
        let to_read = chunk_size.min((file_size - file_pos) as usize);
        let bytes_read = file.read(&mut chunk[..to_read]).await?;
        if bytes_read == 0 {
            break;
        }
        file_pos += bytes_read as u64;

        // Combine remainder with new chunk
        let mut combined = Vec::with_capacity(remainder.len() + bytes_read);
        combined.extend_from_slice(&remainder);
        combined.extend_from_slice(&chunk[..bytes_read]);

        // Process complete lines
        let mut line_start = 0;
        let mut last_newline = 0;

        for (i, &byte) in combined.iter().enumerate() {
            if byte == b'\n' {
                last_newline = i + 1;
                let line = &combined[line_start..i];

                // Check for attribution-snapshot (skip it)
                if line.starts_with(b"{\"type\":\"attribution-snapshot\"") {
                    line_start = i + 1;
                    continue;
                }

                // Check for compact boundary
                if line.len() < 256 {
                    // Boundary marker is within first 256 bytes of line
                    if let Some(pos) = line
                        .windows(COMPACT_BOUNDARY_MARKER.len())
                        .position(|w| w == COMPACT_BOUNDARY_MARKER)
                    {
                        let line_str = String::from_utf8_lossy(line);
                        if let Some(has_preserved) = parse_boundary_line(&line_str) {
                            if has_preserved {
                                has_preserved_segment = true;
                            } else {
                                output.clear();
                                boundary_start_offset = (file_pos as usize) - bytes_read + line_start;
                                has_preserved_segment = false;
                            }
                            line_start = i + 1;
                            continue;
                        }
                    }
                }

                // Write the line to output
                output.extend_from_slice(&combined[line_start..i + 1]);
                line_start = i + 1;
            }
        }

        // Save remainder for next iteration
        remainder = combined[last_newline..].to_vec();
    }

    // Process any remaining data
    if !remainder.is_empty() {
        if !remainder.starts_with(b"{\"type\":\"attribution-snapshot\"") {
            output.extend_from_slice(&remainder);
        }
    }

    Ok(TranscriptLoadResult {
        boundary_start_offset,
        post_boundary_buf: output,
        has_preserved_segment,
    })
}
