//! Keyboard event (keyboard-event.ts).
#[derive(Debug, Clone)]
pub struct KeyboardEvent {
    pub key: String,
    pub code: String,
    pub ctrl: bool, pub meta: bool, pub shift: bool, pub alt: bool,
    pub repeat: bool,
    stopped: bool,
    default_prevented: bool,
}
impl KeyboardEvent {
    pub fn new(key: &str) -> Self {
        Self { key: key.to_string(), code: key.to_string(), ctrl: false, meta: false, shift: false, alt: false, repeat: false, stopped: false, default_prevented: false }
    }
    pub fn with_modifiers(mut self, ctrl: bool, meta: bool, shift: bool, alt: bool) -> Self {
        self.ctrl = ctrl; self.meta = meta; self.shift = shift; self.alt = alt; self
    }
    pub fn stop_propagation(&mut self) { self.stopped = true; }
    pub fn stop_immediate_propagation(&mut self) { self.stopped = true; }
    pub fn prevent_default(&mut self) { self.default_prevented = true; }
    pub fn is_stopped(&self) -> bool { self.stopped }
    pub fn is_default_prevented(&self) -> bool { self.default_prevented }
}
