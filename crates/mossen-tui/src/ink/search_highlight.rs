//! Search Highlight (search-highlight.ts).

/// Highlight all matches of needle in text using ANSI inverse styling.
pub fn apply_search_highlight(text: &str, needle: &str) -> String {
    if needle.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut start = 0;
    while let Some(off) = text[start..].find(needle) {
        let s = start + off;
        out.push_str(&text[start..s]);
        out.push_str("\x1b[7m");
        out.push_str(&text[s..s + needle.len()]);
        out.push_str("\x1b[27m");
        start = s + needle.len();
    }
    out.push_str(&text[start..]);
    out
}

#[derive(Debug, Clone, Default)]
pub struct SearchHighlightState {
    pub initialized: bool,
}

impl SearchHighlightState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
