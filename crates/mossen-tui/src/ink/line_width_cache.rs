//! Line Width Cache (line-width-cache.ts).

/// Compute the printable width of one terminal line, accounting for wide
/// chars. Uses `unicode-width` rules.
pub fn line_width(line: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(line)
}

#[derive(Debug, Clone, Default)]
pub struct LineWidthCacheState {
    pub initialized: bool,
}

impl LineWidthCacheState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
