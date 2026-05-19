//! Newline component (newline.ts/tsx).
//! Renders a newline character.

#[derive(Debug, Clone)]
pub struct NewlineState {
    pub active: bool,
}
impl NewlineState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for NewlineState { fn default() -> Self { Self::new() } }

/// TS `Newline` exports `type Props`.
pub type Props = NewlineState;
