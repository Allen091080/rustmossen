//! Wrap Ansi (wrap-ansi.ts).

#[derive(Debug, Clone, Default)]
pub struct WrapAnsiState {
    pub initialized: bool,
}

impl WrapAnsiState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
