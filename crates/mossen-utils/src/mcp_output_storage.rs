//! MCP output storage — persisting large/binary MCP tool results to disk.

use std::path::{Path, PathBuf};

use tokio::fs;

/// MCP result type enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpResultType {
    ToolResult,
    StructuredContent,
    ContentArray,
}

/// Generates a format description string based on the MCP result type and schema.
pub fn get_format_description(result_type: McpResultType, schema: Option<&str>) -> String {
    match result_type {
        McpResultType::ToolResult => "Plain text".to_string(),
        McpResultType::StructuredContent => match schema {
            Some(s) => format!("JSON with schema: {}", s),
            None => "JSON".to_string(),
        },
        McpResultType::ContentArray => match schema {
            Some(s) => format!("JSON array with schema: {}", s),
            None => "JSON array".to_string(),
        },
    }
}

/// Generates instruction text for Mossen to read from a saved output file.
pub fn get_large_output_instructions(
    raw_output_path: &str,
    content_length: usize,
    format_description: &str,
    max_read_length: Option<usize>,
) -> String {
    let base_instructions = format!(
        "Error: result ({} characters) exceeds maximum allowed tokens. Output has been saved to {}.\n\
         Format: {}\n\
         Use offset and limit parameters to read specific portions of the file, search within it for specific content, and jq to make structured queries.\n\
         REQUIREMENTS FOR SUMMARIZATION/ANALYSIS/REVIEW:\n\
         - You MUST read the content from the file at {} in sequential chunks until 100% of the content has been read.\n",
        content_length, raw_output_path, format_description, raw_output_path
    );

    let truncation_warning = match max_read_length {
        Some(max_len) => format!(
            "- If you receive truncation warnings when reading the file (\"[N lines truncated]\"), reduce the chunk size until you have read 100% of the content without truncation ***DO NOT PROCEED UNTIL YOU HAVE DONE THIS***. Bash output is limited to {} chars.\n",
            max_len
        ),
        None => "- If you receive truncation warnings when reading the file, reduce the chunk size until you have read 100% of the content without truncation.\n".to_string(),
    };

    let completion_requirement = "- Before producing ANY summary or analysis, you MUST explicitly describe what portion of the content you have read. ***If you did not read the entire content, you MUST explicitly state this.***\n".to_string();

    format!(
        "{}{}{}",
        base_instructions, truncation_warning, completion_requirement
    )
}

/// Map a mime type to a file extension.
pub fn extension_for_mime_type(mime_type: Option<&str>) -> &'static str {
    let mt = match mime_type {
        None => return "bin",
        Some(m) => m.split(';').next().unwrap_or("").trim().to_lowercase(),
    };

    match mt.as_str() {
        "application/pdf" => "pdf",
        "application/json" => "json",
        "text/csv" => "csv",
        "text/plain" => "txt",
        "text/html" => "html",
        "text/markdown" => "md",
        "application/zip" => "zip",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => "pptx",
        "application/msword" => "doc",
        "application/vnd.ms-excel" => "xls",
        "audio/mpeg" => "mp3",
        "audio/wav" => "wav",
        "audio/ogg" => "ogg",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}

/// Heuristic for whether a content-type header indicates binary content.
pub fn is_binary_content_type(content_type: &str) -> bool {
    if content_type.is_empty() {
        return false;
    }
    let mt = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();

    if mt.starts_with("text/") {
        return false;
    }
    if mt.ends_with("+json") || mt == "application/json" {
        return false;
    }
    if mt.ends_with("+xml") || mt == "application/xml" {
        return false;
    }
    if mt.starts_with("application/javascript") {
        return false;
    }
    if mt == "application/x-www-form-urlencoded" {
        return false;
    }
    true
}

/// Result of persisting binary content.
pub enum PersistBinaryResult {
    Success {
        filepath: PathBuf,
        size: usize,
        ext: String,
    },
    Error {
        error: String,
    },
}

/// Write raw binary bytes to the tool-results directory with a mime-derived extension.
pub async fn persist_binary_content(
    bytes: &[u8],
    mime_type: Option<&str>,
    persist_id: &str,
    tool_results_dir: &Path,
) -> PersistBinaryResult {
    if let Err(e) = fs::create_dir_all(tool_results_dir).await {
        return PersistBinaryResult::Error {
            error: format!("Failed to create tool results dir: {}", e),
        };
    }

    let ext = extension_for_mime_type(mime_type);
    let filepath = tool_results_dir.join(format!("{}.{}", persist_id, ext));

    match fs::write(&filepath, bytes).await {
        Ok(()) => PersistBinaryResult::Success {
            filepath,
            size: bytes.len(),
            ext: ext.to_string(),
        },
        Err(e) => PersistBinaryResult::Error {
            error: format!("Failed to write binary content: {}", e),
        },
    }
}

/// Build a short message telling Mossen where binary content was saved.
pub fn get_binary_blob_saved_message(
    filepath: &str,
    mime_type: Option<&str>,
    size: usize,
    source_description: &str,
) -> String {
    let mt = mime_type.unwrap_or("unknown type");
    let size_str = format_file_size(size);
    format!(
        "{}Binary content ({}, {}) saved to {}",
        source_description, mt, size_str, filepath
    )
}

/// Format a byte size into a human-readable string.
fn format_file_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
