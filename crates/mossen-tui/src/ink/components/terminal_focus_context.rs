//! TerminalFocusContext component (terminal_focus_context.ts/tsx).
//! Provides terminal focus state to children.

#[derive(Debug, Clone)]
pub struct TerminalFocusContextState {
    pub active: bool,
}
impl TerminalFocusContextState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for TerminalFocusContextState { fn default() -> Self { Self::new() } }

/// Props passed to the TerminalFocusProvider.
#[derive(Debug, Clone, Default)]
pub struct TerminalFocusContextProps {
    pub initial_focused: bool,
}

/// Provider state container.
#[derive(Debug, Clone, Default)]
pub struct TerminalFocusProvider {
    pub focused: bool,
}

impl TerminalFocusProvider {
    pub fn new(props: TerminalFocusContextProps) -> Self {
        Self {
            focused: props.initial_focused,
        }
    }
    pub fn set(&mut self, focused: bool) {
        self.focused = focused;
    }
}
