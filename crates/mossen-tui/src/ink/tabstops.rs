//! Tabstops (tabstops.ts).

/// Expand tab characters to the appropriate number of spaces.
pub fn expand_tabs(input: &str, tab_width: usize) -> String {
    let mut out = String::with_capacity(input.len());
    let mut col = 0usize;
    for c in input.chars() {
        if c == '\t' {
            let spaces = tab_width - (col % tab_width.max(1));
            out.extend(std::iter::repeat(' ').take(spaces));
            col += spaces;
        } else if c == '\n' {
            out.push(c);
            col = 0;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct TabstopsState {
    pub initialized: bool,
}

impl TabstopsState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
