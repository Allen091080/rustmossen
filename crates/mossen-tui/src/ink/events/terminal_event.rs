//! Terminal event base (terminal-event.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase { None, Capturing, AtTarget, Bubbling }

#[derive(Debug, Clone)]
pub struct TerminalEvent {
    pub event_type: String,
    pub timestamp: f64,
    pub bubbles: bool,
    pub cancelable: bool,
    pub target: Option<usize>,
    pub current_target: Option<usize>,
    pub phase: EventPhase,
    propagation_stopped: bool,
    immediate_propagation_stopped: bool,
    default_prevented: bool,
}
impl TerminalEvent {
    pub fn new(event_type: &str, bubbles: bool, cancelable: bool) -> Self {
        Self { event_type: event_type.to_string(), timestamp: 0.0, bubbles, cancelable, target: None, current_target: None, phase: EventPhase::None, propagation_stopped: false, immediate_propagation_stopped: false, default_prevented: false }
    }
    pub fn stop_propagation(&mut self) { self.propagation_stopped = true; }
    pub fn stop_immediate_propagation(&mut self) { self.propagation_stopped = true; self.immediate_propagation_stopped = true; }
    pub fn prevent_default(&mut self) { if self.cancelable { self.default_prevented = true; } }
    pub fn is_propagation_stopped(&self) -> bool { self.propagation_stopped }
    pub fn is_immediate_propagation_stopped(&self) -> bool { self.immediate_propagation_stopped }
    pub fn set_target(&mut self, target: usize) { self.target = Some(target); }
    pub fn set_current_target(&mut self, target: Option<usize>) { self.current_target = target; }
    pub fn set_phase(&mut self, phase: EventPhase) { self.phase = phase; }
}

/// EventTarget identifier — a node id that can receive events.
pub type EventTarget = usize;
