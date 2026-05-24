//! Utility functions for the Bash tool.
//!
//! Includes output formatting, image detection, CWD reset logic, and content summarization.

use regex::Regex;
use std::path::Path;

use mossen_utils::string_utils::{safe_prefix_by_bytes, truncate_chars_with_suffix};

/// Maximum image file size to attempt reading (20 MB).
const MAX_IMAGE_FILE_SIZE: u64 = 20 * 1024 * 1024;

/// Default maximum output length for shell command output.
const DEFAULT_MAX_OUTPUT_LENGTH: usize = 30_000;

/// Strips leading and trailing lines that contain only whitespace/newlines.
/// Unlike trim(), this preserves whitespace within content lines and only removes
/// completely empty lines from the beginning and end.
pub fn strip_empty_lines(content: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();

    // Find the first non-empty line
    let mut start_index = 0;
    while start_index < lines.len() && lines[start_index].trim().is_empty() {
        start_index += 1;
    }

    // Find the last non-empty line
    let mut end_index = lines.len().saturating_sub(1);
    while end_index > 0 && lines[end_index].trim().is_empty() {
        end_index -= 1;
    }

    // If all lines are empty, return empty string
    if start_index > end_index {
        return String::new();
    }

    lines[start_index..=end_index].join("\n")
}

/// Check if content is a base64 encoded image data URL.
pub fn is_image_output(content: &str) -> bool {
    let re = Regex::new(r"(?i)^data:image/[a-z0-9.+_-]+;base64,").unwrap();
    re.is_match(content)
}

/// Parsed data URI components.
pub struct DataUri {
    pub media_type: String,
    pub data: String,
}

/// Parse a data-URI string into its media type and base64 payload.
pub fn parse_data_uri(s: &str) -> Option<DataUri> {
    let trimmed = s.trim();
    let re = Regex::new(r"^data:([^;]+);base64,(.+)$").unwrap();
    let captures = re.captures(trimmed)?;
    let media_type = captures.get(1)?.as_str().to_string();
    let data = captures.get(2)?.as_str().to_string();
    Some(DataUri { media_type, data })
}

/// Build an image tool_result block from shell stdout containing a data URI.
/// Returns None if parse fails so callers can fall through to text handling.
pub fn build_image_tool_result(stdout: &str, tool_use_id: &str) -> Option<serde_json::Value> {
    let parsed = parse_data_uri(stdout)?;
    Some(serde_json::json!({
        "tool_use_id": tool_use_id,
        "type": "tool_result",
        "content": [{
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": parsed.media_type,
                "data": parsed.data,
            }
        }]
    }))
}

/// Resize image output from a shell tool.
/// If the image is too large or cannot be parsed, returns None.
pub async fn resize_shell_image_output(
    stdout: &str,
    output_file_path: Option<&str>,
    output_file_size: Option<u64>,
) -> Option<String> {
    let source = if let Some(path) = output_file_path {
        let size = match output_file_size {
            Some(s) => s,
            None => tokio::fs::metadata(path).await.ok()?.len(),
        };
        if size > MAX_IMAGE_FILE_SIZE {
            return None;
        }
        tokio::fs::read_to_string(path).await.ok()?
    } else {
        stdout.to_string()
    };

    let parsed = parse_data_uri(&source)?;
    // In a full implementation, we would resize/downsample the image here.
    // For now, return the data URI as-is if it's valid.
    let ext = parsed.media_type.split('/').nth(1).unwrap_or("png");
    Some(format!("data:image/{};base64,{}", ext, parsed.data))
}

/// Output formatting result.
pub struct FormattedOutput {
    pub total_lines: usize,
    pub truncated_content: String,
    pub is_image: bool,
}

/// Count occurrences of a character in a string, optionally starting from a byte offset.
fn count_char_in_string(s: &str, ch: char, start_byte: usize) -> usize {
    let start = if start_byte >= s.len() {
        s.len()
    } else if s.is_char_boundary(start_byte) {
        start_byte
    } else {
        s.char_indices()
            .map(|(idx, _)| idx)
            .find(|idx| *idx > start_byte)
            .unwrap_or(s.len())
    };
    s[start..].chars().filter(|&c| c == ch).count()
}

/// Format output with truncation if necessary.
pub fn format_output(content: &str) -> FormattedOutput {
    let is_image = is_image_output(content);
    if is_image {
        return FormattedOutput {
            total_lines: 1,
            truncated_content: content.to_string(),
            is_image,
        };
    }

    let max_output_length = DEFAULT_MAX_OUTPUT_LENGTH;
    if content.len() <= max_output_length {
        return FormattedOutput {
            total_lines: content.chars().filter(|&c| c == '\n').count() + 1,
            truncated_content: content.to_string(),
            is_image,
        };
    }

    let truncated_part = safe_prefix_by_bytes(content, max_output_length);
    let remaining_lines = count_char_in_string(content, '\n', truncated_part.len()) + 1;
    let truncated = format!(
        "{}\n\n... [{} lines truncated] ...",
        truncated_part, remaining_lines
    );

    FormattedOutput {
        total_lines: content.chars().filter(|&c| c == '\n').count() + 1,
        truncated_content: truncated,
        is_image,
    }
}

/// Append shell reset message to stderr.
pub fn stderr_append_shell_reset_message(stderr: &str, original_cwd: &str) -> String {
    format!("{}\nShell cwd was reset to {}", stderr.trim(), original_cwd)
}

/// Reset CWD if outside the allowed project working directory.
/// Returns true if the CWD was reset.
pub fn reset_cwd_if_outside_project(
    current_cwd: &str,
    original_cwd: &str,
    should_maintain_project_dir: bool,
    is_in_allowed_path: bool,
) -> bool {
    if should_maintain_project_dir || (current_cwd != original_cwd && !is_in_allowed_path) {
        if !should_maintain_project_dir {
            return true;
        }
    }
    false
}

/// Creates a human-readable summary of structured content blocks.
/// Used to display MCP results with images and text in the UI.
pub fn create_content_summary(content: &[serde_json::Value]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut text_count = 0usize;
    let mut image_count = 0usize;

    for block in content {
        let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match block_type {
            "image" => image_count += 1,
            "text" => {
                text_count += 1;
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    let preview = truncate_chars_with_suffix(text, 200, "...");
                    parts.push(preview);
                }
            }
            _ => {}
        }
    }

    let mut summary = Vec::new();
    if image_count > 0 {
        summary.push(format!(
            "[{} {}]",
            image_count,
            if image_count == 1 { "image" } else { "images" }
        ));
    }
    if text_count > 0 {
        summary.push(format!(
            "[{} text {}]",
            text_count,
            if text_count == 1 { "block" } else { "blocks" }
        ));
    }

    let summary_str = summary.join(", ");
    if parts.is_empty() {
        format!("MCP Result: {}", summary_str)
    } else {
        format!("MCP Result: {}\n\n{}", summary_str, parts.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_empty_lines() {
        assert_eq!(strip_empty_lines("\n\nhello\nworld\n\n"), "hello\nworld");
        assert_eq!(strip_empty_lines("hello"), "hello");
        assert_eq!(strip_empty_lines("\n\n\n"), "");
        assert_eq!(strip_empty_lines("  \nhello\n  "), "hello");
    }

    #[test]
    fn test_is_image_output() {
        assert!(is_image_output("data:image/png;base64,iVBORw0KGgo="));
        assert!(is_image_output("data:image/jpeg;base64,/9j/4AAQ"));
        assert!(!is_image_output("hello world"));
        assert!(!is_image_output("data:text/plain;base64,aGVsbG8="));
    }

    #[test]
    fn test_parse_data_uri() {
        let result = parse_data_uri("data:image/png;base64,abc123").unwrap();
        assert_eq!(result.media_type, "image/png");
        assert_eq!(result.data, "abc123");

        assert!(parse_data_uri("not a data uri").is_none());
    }

    #[test]
    fn test_format_output_short() {
        let output = format_output("hello\nworld");
        assert_eq!(output.total_lines, 2);
        assert_eq!(output.truncated_content, "hello\nworld");
        assert!(!output.is_image);
    }
}
