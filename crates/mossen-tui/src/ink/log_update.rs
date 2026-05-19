//! Log Update (log-update.ts).

/// In-place log updater — rewrites the previous output by emitting cursor-up
/// escapes followed by the new lines.
#[derive(Debug, Clone, Default)]
pub struct LogUpdate {
    pub previous_line_count: usize,
}

impl LogUpdate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the escape-prefixed payload that replaces the previous lines.
    pub fn update_payload(&mut self, content: &str) -> String {
        let new_count = content.lines().count();
        let mut out = String::new();
        if self.previous_line_count > 0 {
            out.push_str(&format!("\x1b[{}A", self.previous_line_count));
            out.push_str("\r\x1b[J");
        }
        out.push_str(content);
        self.previous_line_count = new_count;
        out
    }

    /// Reset — next call will write fresh.
    pub fn clear(&mut self) {
        self.previous_line_count = 0;
    }
}

#[derive(Debug, Clone, Default)]
pub struct LogUpdateState {
    pub initialized: bool,
}

impl LogUpdateState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
