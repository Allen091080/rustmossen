//! # symbol_context — Extract a symbol/word at a position in a file.
//!
//! Translates `tools/LSPTool/symbolContext.ts`.

use regex::Regex;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

const MAX_READ_BYTES: usize = 64 * 1024;
const MAX_SYMBOL_LEN: usize = 30;

static SYMBOL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\w$'!]+|[+\-*/%&|^~<>=]+").unwrap()
});

/// Expand `~` to the user's home directory at the start of a path.
fn expand_path(p: &str) -> String {
    if let Some(stripped) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(stripped).to_string_lossy().into_owned();
        }
    } else if p == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return home.to_string_lossy().into_owned();
        }
    }
    p.to_string()
}

/// Truncate a symbol with an ellipsis if it exceeds `max`.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// `symbolContext.ts` `getSymbolAtPosition` — extract the symbol/word at a
/// specific position in a file. Used to show context in tool use messages.
///
/// Reads only the first 64KB of the file (most LSP hover/goto targets are
/// near recent edits; 64KB covers ~1000 lines of typical code). If the
/// target line is past that window, returns None.
///
/// * `file_path` — file path (absolute or `~`-relative)
/// * `line` — 0-indexed line number
/// * `character` — 0-indexed character position on the line
pub fn get_symbol_at_position(
    file_path: &str,
    line: usize,
    character: usize,
) -> Option<String> {
    let abs = expand_path(file_path);
    let mut file = File::open(&abs).ok()?;
    let mut buf = vec![0u8; MAX_READ_BYTES];
    let bytes_read = file.read(&mut buf).ok()?;
    let content = std::str::from_utf8(&buf[..bytes_read]).ok()?;
    let lines: Vec<&str> = content.split('\n').collect();

    if line >= lines.len() {
        return None;
    }

    // If the buffer was filled to the max, the last split element may be
    // truncated mid-line — skip it.
    if bytes_read == MAX_READ_BYTES && line == lines.len().saturating_sub(1) {
        return None;
    }

    let line_content = lines[line];
    if character >= line_content.chars().count() {
        return None;
    }

    // Walk regex matches looking for one that brackets `character`.
    for m in SYMBOL_PATTERN.find_iter(line_content) {
        // Use char-based offsets so multi-byte sequences don't skew positions.
        let pre = &line_content[..m.start()];
        let start = pre.chars().count();
        let end = start + m.as_str().chars().count();
        if character >= start && character < end {
            return Some(truncate(m.as_str(), MAX_SYMBOL_LEN));
        }
    }
    None
}

/// Alias matching the TS export name.
#[allow(non_snake_case)]
pub fn getSymbolAtPosition(
    file_path: &str,
    line: usize,
    character: usize,
) -> Option<String> {
    get_symbol_at_position(file_path, line, character)
}
