//! Stdin hook (use-stdin.ts).
//! Provides access to stdin stream.

#[derive(Debug, Clone)]
pub struct StdinHookState {
    pub active: bool,
}
impl StdinHookState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for StdinHookState { fn default() -> Self { Self::new() } }
