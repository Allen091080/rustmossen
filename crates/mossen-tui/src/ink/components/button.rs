//! Button component (button.ts/tsx).
//! Interactive button component with focus support.

#[derive(Debug, Clone)]
pub struct ButtonState {
    pub active: bool,
}
impl ButtonState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for ButtonState { fn default() -> Self { Self::new() } }

/// TS `Button` exports `type Props`. Alias to existing state struct.
pub type Props = ButtonState;
