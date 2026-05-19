//! TerminalViewport hook (use-terminal-viewport.ts).
//! Manages the terminal viewport/scrollback.

#[derive(Debug, Clone)]
pub struct TerminalViewportHookState {
    pub active: bool,
}
impl TerminalViewportHookState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for TerminalViewportHookState { fn default() -> Self { Self::new() } }

/// Hook-equivalent useTerminalViewport.
pub fn use_terminal_viewport(state: &mut TerminalViewportHookState) -> &mut TerminalViewportHookState {
    state
}
