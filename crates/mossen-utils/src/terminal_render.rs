/// Text rendering utilities for terminal display
const MAX_LINES_TO_SHOW: usize = 3;
/// Account for MessageResponse prefix ("  ⎿ " = 5 chars) + parent width
/// reduction (columns - 5 in tool result rendering)
const PADDING_TO_PREVENT_OVERFLOW: usize = 10;

/// Result of wrapping text
struct WrapResult {
    above_the_fold: String,
    remaining_lines: usize,
}

/// Compute the visible width of a string (approximation: byte length for ASCII,
/// but counts chars for Unicode). In real terminal usage this would use a proper
/// Unicode width library. Here we use char count as approximation.
fn string_width(s: &str) -> usize {
    // Simple approximation: count characters
    // A proper implementation would use unicode-width crate
    s.chars().count()
}

/// Slice a string by visible character positions (ANSI-unaware for simplicity)
fn slice_str(s: &str, start: usize, end: usize) -> String {
    s.chars().skip(start).take(end - start).collect()
}

/// Inserts newlines in a string to wrap it at the specified width.
fn wrap_text(text: &str, wrap_width: usize) -> WrapResult {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut wrapped_lines: Vec<String> = Vec::new();

    for line in lines {
        let visible_width = string_width(line);
        if visible_width <= wrap_width {
            wrapped_lines.push(line.trim_end().to_string());
        } else {
            // Break long lines into chunks of wrap_width visible characters
            let mut position = 0;
            while position < visible_width {
                let chunk = slice_str(line, position, (position + wrap_width).min(visible_width));
                wrapped_lines.push(chunk.trim_end().to_string());
                position += wrap_width;
            }
        }
    }

    let remaining_lines = if wrapped_lines.len() > MAX_LINES_TO_SHOW {
        wrapped_lines.len() - MAX_LINES_TO_SHOW
    } else {
        0
    };

    // If there's only 1 line after the fold, show it directly
    if remaining_lines == 1 {
        return WrapResult {
            above_the_fold: wrapped_lines[..MAX_LINES_TO_SHOW + 1]
                .join("\n")
                .trim_end()
                .to_string(),
            remaining_lines: 0,
        };
    }

    WrapResult {
        above_the_fold: wrapped_lines[..wrapped_lines.len().min(MAX_LINES_TO_SHOW)]
            .join("\n")
            .trim_end()
            .to_string(),
        remaining_lines,
    }
}

/// Renders content with line-based truncation for terminal display.
/// If the content exceeds the maximum number of lines, it truncates the content
/// and adds a message indicating the number of additional lines.
pub fn render_truncated_content(
    content: &str,
    terminal_width: usize,
    suppress_expand_hint: bool,
) -> String {
    let trimmed_content = content.trim_end();
    if trimmed_content.is_empty() {
        return String::new();
    }

    let wrap_width = if terminal_width > PADDING_TO_PREVENT_OVERFLOW {
        terminal_width - PADDING_TO_PREVENT_OVERFLOW
    } else {
        10
    };

    // Only process enough content for the visible lines
    let max_chars = MAX_LINES_TO_SHOW * wrap_width * 4;
    let pre_truncated = trimmed_content.len() > max_chars;
    let content_for_wrapping = if pre_truncated {
        &trimmed_content[..max_chars]
    } else {
        trimmed_content
    };

    let WrapResult {
        above_the_fold,
        remaining_lines,
    } = wrap_text(content_for_wrapping, wrap_width);

    let estimated_remaining = if pre_truncated {
        std::cmp::max(
            remaining_lines,
            trimmed_content.len() / wrap_width - MAX_LINES_TO_SHOW,
        )
    } else {
        remaining_lines
    };

    let mut parts = vec![above_the_fold];
    if estimated_remaining > 0 {
        let hint = if suppress_expand_hint {
            format!("… +{} lines", estimated_remaining)
        } else {
            format!("… +{} lines (ctrl+o to expand)", estimated_remaining)
        };
        parts.push(hint);
    }

    parts
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Fast check: would OutputLine truncate this content?
/// Counts raw newlines only (ignores terminal-width wrapping).
pub fn is_output_line_truncated(content: &str) -> bool {
    let mut pos = 0usize;
    // Need more than MAX_LINES_TO_SHOW newlines
    // The +1 accounts for wrapText showing an extra line when remainingLines==1
    for _ in 0..=MAX_LINES_TO_SHOW {
        match content[pos..].find('\n') {
            Some(idx) => {
                pos += idx + 1;
            }
            None => return false,
        }
    }
    // A trailing newline is a terminator, not a new line
    pos < content.len()
}
