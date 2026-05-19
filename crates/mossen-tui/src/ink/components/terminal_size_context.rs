//! TerminalSizeContext component (terminal_size_context.ts/tsx).
//! Provides terminal dimensions to children.

#[derive(Debug, Clone)]
pub struct TerminalSizeContextState {
    pub active: bool,
}
impl TerminalSizeContextState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for TerminalSizeContextState { fn default() -> Self { Self::new() } }

/// Context holding the live terminal size.
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

/// Active context handle (alias used by readers).
#[allow(non_upper_case_globals)]
pub static TerminalSizeContext: Option<&'static TerminalSize> = None;
