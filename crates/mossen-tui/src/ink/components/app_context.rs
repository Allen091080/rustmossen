//! AppContext component (app_context.ts/tsx).
//! Provides app-level context (exit, stdin methods).

#[derive(Debug, Clone)]
pub struct AppContextState {
    pub active: bool,
}
impl AppContextState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for AppContextState { fn default() -> Self { Self::new() } }

/// TS `AppContext` exports `type Props`.
pub type Props = AppContextState;
