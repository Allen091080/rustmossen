//! App hook (use-app.ts).
//! Provides access to the ink app instance.

#[derive(Debug, Clone)]
pub struct AppHookState {
    pub active: bool,
}
impl AppHookState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for AppHookState { fn default() -> Self { Self::new() } }
