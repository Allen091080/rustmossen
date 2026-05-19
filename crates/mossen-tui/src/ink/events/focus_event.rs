//! Focus event (focus-event.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusEventType { Focus, Blur }

#[derive(Debug, Clone)]
pub struct FocusEvent {
    pub event_type: FocusEventType,
    pub related_target: Option<usize>,
    stopped: bool,
}
impl FocusEvent {
    pub fn new(event_type: FocusEventType) -> Self { Self { event_type, related_target: None, stopped: false } }
    pub fn stop_propagation(&mut self) { self.stopped = true; }
    pub fn is_stopped(&self) -> bool { self.stopped }
}
