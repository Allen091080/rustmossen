//! Ink App (ink-app.ts).

/// Drain stdin buffer, returning all queued bytes as a single string.
pub fn drain_stdin(buf: &mut Vec<u8>) -> String {
    let s = String::from_utf8_lossy(buf).to_string();
    buf.clear();
    s
}

#[derive(Debug, Clone, Default)]
pub struct InkAppState {
    pub initialized: bool,
}

impl InkAppState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
