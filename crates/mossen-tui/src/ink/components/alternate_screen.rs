//! AlternateScreen component (alternate_screen.ts/tsx).
//! Renders children in the alternate screen buffer with mouse tracking.

#[derive(Debug, Clone)]
pub struct AlternateScreenState {
    pub active: bool,
}
impl AlternateScreenState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for AlternateScreenState { fn default() -> Self { Self::new() } }
