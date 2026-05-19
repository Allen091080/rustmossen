//! Terminal focus/blur event (terminal-focus-event.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalFocusEventType { Focus, Blur }

#[derive(Debug, Clone)]
pub struct TerminalFocusEvent {
    pub event_type: TerminalFocusEventType,
}
impl TerminalFocusEvent {
    pub fn new(event_type: TerminalFocusEventType) -> Self { Self { event_type } }
    pub fn is_focus(&self) -> bool { self.event_type == TerminalFocusEventType::Focus }
    pub fn is_blur(&self) -> bool { self.event_type == TerminalFocusEventType::Blur }
}
