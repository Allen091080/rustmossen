//! Terminal Io (terminal-io.ts).

#[derive(Debug, Clone, Default)]
pub struct TerminalIoState {
    pub initialized: bool,
}

impl TerminalIoState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
