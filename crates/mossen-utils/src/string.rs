//! String utility functions.
//!
//! Mirrors the TS `stringUtils.ts` module — regex escaping, capitalization,
//! pluralization, line counting, truncation, and safe accumulation.

/// Escapes special regex metacharacters so the string can be used as a literal
/// pattern in a `Regex`.
pub fn escape_regexp(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Uppercases the first character, leaving the rest unchanged.
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            upper + chars.as_str()
        }
    }
}

/// Returns the singular or plural form of a word based on count.
pub fn plural(n: usize, word: &str, plural_word: Option<&str>) -> String {
    if n == 1 {
        word.to_string()
    } else {
        match plural_word {
            Some(pw) => pw.to_string(),
            None => format!("{word}s"),
        }
    }
}

/// Returns the first line of a string without allocating a split array.
pub fn first_line_of(s: &str) -> &str {
    match s.find('\n') {
        Some(idx) => &s[..idx],
        None => s,
    }
}

/// Counts occurrences of `needle` (single char) in `haystack` starting from `start`.
pub fn count_char(haystack: &str, needle: char, start: usize) -> usize {
    haystack[start..].chars().filter(|&c| c == needle).count()
}

/// Normalize full-width (zenkaku) digits to half-width digits.
pub fn normalize_full_width_digits(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ('\u{FF10}'..='\u{FF19}').contains(&ch) {
                // U+FF10 ('０') maps to '0', etc.
                char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch)
            } else {
                ch
            }
        })
        .collect()
}

/// Normalize full-width space (U+3000) to ASCII space.
pub fn normalize_full_width_space(input: &str) -> String {
    input.replace('\u{3000}', " ")
}

/// Default max string accumulation size (32 MiB).
const MAX_STRING_LENGTH: usize = 1 << 25;

/// Safely joins lines with a delimiter, truncating if the result exceeds
/// `max_size`.
pub fn safe_join_lines(lines: &[&str], delimiter: &str, max_size: usize) -> String {
    let truncation_marker = "...[truncated]";
    let mut result = String::new();

    for line in lines {
        let delim = if result.is_empty() { "" } else { delimiter };
        let full_addition_len = delim.len() + line.len();

        if result.len() + full_addition_len <= max_size {
            result.push_str(delim);
            result.push_str(line);
        } else {
            let remaining_space =
                max_size.saturating_sub(result.len() + delim.len() + truncation_marker.len());
            if remaining_space > 0 {
                result.push_str(delim);
                // Careful: slice on char boundary
                let take = line
                    .char_indices()
                    .take_while(|(i, ch)| i + ch.len_utf8() <= remaining_space)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0);
                result.push_str(&line[..take]);
                result.push_str(truncation_marker);
            } else {
                result.push_str(truncation_marker);
            }
            return result;
        }
    }
    result
}

/// Convenience overload using defaults.
pub fn safe_join_lines_default(lines: &[&str]) -> String {
    safe_join_lines(lines, ",", MAX_STRING_LENGTH)
}

/// A string accumulator that safely truncates from the end when size limit is
/// exceeded.
pub struct EndTruncatingAccumulator {
    content: String,
    is_truncated: bool,
    total_bytes_received: usize,
    max_size: usize,
}

impl EndTruncatingAccumulator {
    pub fn new(max_size: usize) -> Self {
        Self {
            content: String::with_capacity(max_size.min(8192)),
            is_truncated: false,
            total_bytes_received: 0,
            max_size,
        }
    }

    pub fn with_default_limit() -> Self {
        Self::new(MAX_STRING_LENGTH)
    }

    /// Appends data. If total exceeds `max_size`, the tail is dropped.
    pub fn append(&mut self, data: &str) {
        self.total_bytes_received += data.len();

        if self.is_truncated && self.content.len() >= self.max_size {
            return;
        }

        if self.content.len() + data.len() > self.max_size {
            let remaining = self.max_size.saturating_sub(self.content.len());
            if remaining > 0 {
                // Only push up to a char boundary
                let take = data
                    .char_indices()
                    .take_while(|(i, ch)| i + ch.len_utf8() <= remaining)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0);
                self.content.push_str(&data[..take]);
            }
            self.is_truncated = true;
        } else {
            self.content.push_str(data);
        }
    }

    /// Returns the accumulated string with truncation notice if applicable.
    pub fn to_string_with_notice(&self) -> String {
        if !self.is_truncated {
            return self.content.clone();
        }
        let truncated_bytes = self.total_bytes_received.saturating_sub(self.max_size);
        let truncated_kb = truncated_bytes / 1024;
        format!(
            "{}\n... [output truncated - {}KB removed]",
            self.content, truncated_kb
        )
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.is_truncated = false;
        self.total_bytes_received = 0;
    }

    pub fn len(&self) -> usize {
        self.content.len()
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    pub fn is_truncated(&self) -> bool {
        self.is_truncated
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes_received
    }
}

/// Truncates text to a maximum number of lines, appending ellipsis if truncated.
pub fn truncate_to_lines(text: &str, max_lines: usize) -> String {
    let mut count = 0;
    let mut end = 0;
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            count += 1;
            if count >= max_lines {
                end = i;
                break;
            }
        }
    }
    if count < max_lines {
        return text.to_string();
    }
    let mut result = text[..end].to_string();
    result.push('…');
    result
}
