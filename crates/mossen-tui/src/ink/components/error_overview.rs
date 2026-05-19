//! ErrorOverview component (error_overview.ts/tsx).
//! Displays error information with stack trace.

#[derive(Debug, Clone)]
pub struct ErrorOverviewState {
    pub active: bool,
}
impl ErrorOverviewState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for ErrorOverviewState { fn default() -> Self { Self::new() } }
