//! Render To Screen (render-to-screen.ts).

/// Position of one regex/highlight match in the source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchPosition {
    pub start: usize,
    pub end: usize,
    pub kind: u8,
}

/// Render a snapshot of styled lines to the terminal. Returns the
/// number of bytes written.
pub fn render_to_screen(lines: &[String], out: &mut Vec<u8>) -> usize {
    let before = out.len();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push(b'\n');
        }
        out.extend_from_slice(line.as_bytes());
    }
    out.len() - before
}

/// Scan a string for all occurrences of a needle and produce match positions.
pub fn scan_positions(haystack: &str, needle: &str, kind: u8) -> Vec<MatchPosition> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut start = 0;
    while let Some(off) = haystack[start..].find(needle) {
        let s = start + off;
        let e = s + needle.len();
        out.push(MatchPosition { start: s, end: e, kind });
        start = e;
    }
    out
}

/// Wrap matched substrings in ANSI highlight escape sequences.
pub fn apply_positioned_highlight(text: &str, positions: &[MatchPosition]) -> String {
    if positions.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + positions.len() * 16);
    let mut cur = 0;
    let mut sorted = positions.to_vec();
    sorted.sort_by_key(|p| p.start);
    for p in sorted {
        if p.start < cur || p.end > text.len() {
            continue;
        }
        out.push_str(&text[cur..p.start]);
        out.push_str("\x1b[7m");
        out.push_str(&text[p.start..p.end]);
        out.push_str("\x1b[27m");
        cur = p.end;
    }
    out.push_str(&text[cur..]);
    out
}

#[derive(Debug, Clone, Default)]
pub struct RenderToScreenState {
    pub initialized: bool,
}

impl RenderToScreenState {
    pub fn new() -> Self {
        Self { initialized: false }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
}
