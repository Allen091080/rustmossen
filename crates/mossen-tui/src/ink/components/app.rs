//! InkApp component (app.ts/tsx).
//! Root application component managing render lifecycle.

#[derive(Debug, Clone)]
pub struct InkAppState {
    pub active: bool,
}
impl InkAppState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for InkAppState { fn default() -> Self { Self::new() } }

/// Mouse event passed to the App component handler.
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    pub x: u16,
    pub y: u16,
    pub button: u8,
    pub pressed: bool,
}

/// Top-level mouse-event dispatcher — returns the in-app action name, if any.
pub fn handle_mouse_event(state: &mut InkAppState, ev: MouseEvent) -> Option<&'static str> {
    let _ = (state, ev);
    None
}
