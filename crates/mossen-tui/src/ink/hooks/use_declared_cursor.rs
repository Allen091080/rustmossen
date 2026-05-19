//! DeclaredCursor hook (use-declared-cursor.ts).
//! Manages declared cursor position for rendering.

#[derive(Debug, Clone)]
pub struct DeclaredCursorHookState {
    pub active: bool,
}
impl DeclaredCursorHookState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for DeclaredCursorHookState { fn default() -> Self { Self::new() } }

/// Hook-equivalent useDeclaredCursor.
pub fn use_declared_cursor(state: &mut DeclaredCursorHookState) -> &mut DeclaredCursorHookState {
    state
}
