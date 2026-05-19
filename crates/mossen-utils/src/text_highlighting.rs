/// Represents a text highlight range.
#[derive(Debug, Clone)]
pub struct TextHighlight {
    pub start: usize,
    pub end: usize,
    pub color: Option<String>,
    pub dim_color: bool,
    pub inverse: bool,
    pub shimmer_color: Option<String>,
    pub priority: i32,
}

/// A text segment with optional highlighting.
#[derive(Debug, Clone)]
pub struct TextSegment {
    pub text: String,
    pub start: usize,
    pub highlight: Option<TextHighlight>,
}

/// Segment text by highlights, resolving overlaps by priority.
pub fn segment_text_by_highlights(text: &str, highlights: &[TextHighlight]) -> Vec<TextSegment> {
    if highlights.is_empty() {
        return vec![TextSegment {
            text: text.to_string(),
            start: 0,
            highlight: None,
        }];
    }

    let mut sorted_highlights = highlights.to_vec();
    sorted_highlights.sort_by(|a, b| {
        if a.start != b.start {
            a.start.cmp(&b.start)
        } else {
            b.priority.cmp(&a.priority)
        }
    });

    // Resolve overlaps
    let mut resolved_highlights: Vec<TextHighlight> = Vec::new();
    let mut used_ranges: Vec<(usize, usize)> = Vec::new();

    for highlight in &sorted_highlights {
        if highlight.start == highlight.end {
            continue;
        }

        let overlaps = used_ranges.iter().any(|range| {
            (highlight.start >= range.0 && highlight.start < range.1)
                || (highlight.end > range.0 && highlight.end <= range.1)
                || (highlight.start <= range.0 && highlight.end >= range.1)
        });

        if !overlaps {
            resolved_highlights.push(highlight.clone());
            used_ranges.push((highlight.start, highlight.end));
        }
    }

    // Build segments
    let mut segments: Vec<TextSegment> = Vec::new();
    let mut current_pos = 0;

    for highlight in &resolved_highlights {
        // Add text before highlight
        if highlight.start > current_pos {
            let before_text = safe_substring(text, current_pos, highlight.start);
            if !before_text.is_empty() {
                segments.push(TextSegment {
                    text: before_text,
                    start: current_pos,
                    highlight: None,
                });
            }
        }

        // Add highlighted text
        let highlight_text = safe_substring(text, highlight.start, highlight.end);
        if !highlight_text.is_empty() {
            segments.push(TextSegment {
                text: highlight_text,
                start: highlight.start,
                highlight: Some(highlight.clone()),
            });
        }

        current_pos = highlight.end;
    }

    // Add remaining text
    if current_pos < text.len() {
        let remaining = safe_substring(text, current_pos, text.len());
        if !remaining.is_empty() {
            segments.push(TextSegment {
                text: remaining,
                start: current_pos,
                highlight: None,
            });
        }
    }

    if segments.is_empty() {
        segments.push(TextSegment {
            text: text.to_string(),
            start: 0,
            highlight: None,
        });
    }

    segments
}

/// Safe substring that handles ANSI escape codes.
/// For simplicity, this counts visible characters only.
fn safe_substring(text: &str, start: usize, end: usize) -> String {
    // Strip ANSI for position counting, but preserve them in output
    let mut visible_pos = 0;
    let mut byte_start = None;
    let mut byte_end = None;
    let mut i = 0;
    let bytes = text.as_bytes();

    while i < bytes.len() {
        // Check for ANSI escape sequence
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // Skip ANSI sequence
            let seq_start = i;
            i += 2;
            while i < bytes.len() && !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'm') {
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // Skip the final character
            }
            continue;
        }

        if visible_pos == start && byte_start.is_none() {
            byte_start = Some(i);
        }
        if visible_pos == end {
            byte_end = Some(i);
            break;
        }

        // Advance one character
        let ch = text[i..].chars().next().unwrap_or('\0');
        i += ch.len_utf8();
        visible_pos += 1;
    }

    if visible_pos == end && byte_end.is_none() {
        byte_end = Some(i);
    }

    if byte_start.is_none() {
        byte_start = Some(text.len().min(i));
    }

    let bs = byte_start.unwrap_or(0);
    let be = byte_end.unwrap_or(text.len());

    if bs >= be || bs >= text.len() {
        return String::new();
    }

    text[bs..be].to_string()
}
