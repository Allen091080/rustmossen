//! Input hook (use-input.ts).
//! Subscribes to raw terminal input events.

#[derive(Debug, Clone)]
pub struct InputHookState {
    pub active: bool,
}
impl InputHookState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for InputHookState { fn default() -> Self { Self::new() } }
