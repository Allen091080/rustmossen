//! Widest line calculation (widest-line.ts).
use super::string_width::string_width;

/// Find the width of the widest line in a multi-line string.
pub fn widest_line(text: &str) -> usize {
    text.lines().map(|line| string_width(line)).max().unwrap_or(0)
}
