//! Base event class (event.ts).
#[derive(Debug, Clone)]
pub struct Event {
    stop_immediate_propagation: bool,
}
impl Event {
    pub fn new() -> Self { Self { stop_immediate_propagation: false } }
    pub fn stop_immediate_propagation(&mut self) { self.stop_immediate_propagation = true; }
    pub fn did_stop_immediate_propagation(&self) -> bool { self.stop_immediate_propagation }
}
impl Default for Event { fn default() -> Self { Self::new() } }
