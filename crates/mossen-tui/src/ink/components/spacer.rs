//! Spacer component (spacer.ts/tsx).
//! Flexible spacer that fills available space.

#[derive(Debug, Clone)]
pub struct SpacerState {
    pub active: bool,
}
impl SpacerState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for SpacerState { fn default() -> Self { Self::new() } }
