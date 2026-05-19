//! Input event (input-event.ts).
#[derive(Debug, Clone)]
pub struct InputEvent {
    pub data: String,
    pub input_type: InputType,
    stopped: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType { InsertText, DeleteContent, InsertFromPaste }
impl InputEvent {
    pub fn new(data: String, input_type: InputType) -> Self { Self { data, input_type, stopped: false } }
    pub fn stop_propagation(&mut self) { self.stopped = true; }
}
