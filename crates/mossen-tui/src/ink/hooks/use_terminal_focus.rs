//! TerminalFocus hook (use-terminal-focus.ts).
//! Tracks terminal window focus state.

#[derive(Debug, Clone)]
pub struct TerminalFocusHookState {
    pub active: bool,
    pub focused: bool,
}
impl TerminalFocusHookState {
    pub fn new() -> Self { Self { active: true, focused: true } }
    pub fn set_focused(&mut self, focused: bool) { self.focused = focused; }
    pub fn is_focused(&self) -> bool { self.focused }
}
impl Default for TerminalFocusHookState { fn default() -> Self { Self::new() } }

/// Hook-equivalent useTerminalFocus.
pub fn use_terminal_focus(state: &TerminalFocusHookState) -> bool {
    state.focused
}
