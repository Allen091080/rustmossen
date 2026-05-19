//! String width calculation (stringWidth.ts).
use unicode_width::UnicodeWidthStr;
use unicode_width::UnicodeWidthChar;

/// Calculate display width of a string (handling ANSI, CJK, emoji).
pub fn string_width(s: &str) -> usize {
    // Strip ANSI first
    let stripped = strip_ansi(s);
    UnicodeWidthStr::width(stripped.as_str())
}

/// Strip ANSI escape sequences from a string.
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    let mut in_osc = false;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1B {
            in_escape = true;
            if i + 1 < bytes.len() && bytes[i + 1] == b']' { in_osc = true; }
            i += 1; continue;
        }
        if in_osc {
            if bytes[i] == 0x07 || (bytes[i] == b'\\' && i > 0 && bytes[i-1] == 0x1B) { in_escape = false; in_osc = false; }
            i += 1; continue;
        }
        if in_escape {
            if (0x40..=0x7E).contains(&bytes[i]) { in_escape = false; }
            i += 1; continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

/// Calculate width of a single character.
pub fn char_width(c: char) -> usize { UnicodeWidthChar::width(c).unwrap_or(0) }
