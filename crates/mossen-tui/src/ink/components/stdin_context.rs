//! StdinContext component (stdin_context.ts/tsx).
//! Provides stdin stream access to children.

#[derive(Debug, Clone)]
pub struct StdinContextState {
    pub active: bool,
}
impl StdinContextState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for StdinContextState { fn default() -> Self { Self::new() } }

/// TS `StdinContext` exports `type Props`.
pub type Props = StdinContextState;
