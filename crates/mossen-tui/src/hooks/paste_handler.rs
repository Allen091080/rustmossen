//! Paste handler hook (usePasteHandler.ts).
//! Handles clipboard paste events including bracketed paste detection.

#[derive(Debug, Clone)]
pub struct PasteHandlerState {
    pub is_pasting: bool,
    pub paste_buffer: String,
    pub bracketed_paste_enabled: bool,
    pub last_paste_length: usize,
}

impl PasteHandlerState {
    pub fn new() -> Self {
        Self { is_pasting: false, paste_buffer: String::new(), bracketed_paste_enabled: true, last_paste_length: 0 }
    }
    pub fn start_paste(&mut self) { self.is_pasting = true; self.paste_buffer.clear(); }
    pub fn append(&mut self, text: &str) { self.paste_buffer.push_str(text); }
    pub fn end_paste(&mut self) -> String {
        self.is_pasting = false;
        self.last_paste_length = self.paste_buffer.len();
        std::mem::take(&mut self.paste_buffer)
    }
    pub fn handle_unbracket_paste(&mut self, text: &str) -> String {
        self.last_paste_length = text.len();
        text.to_string()
    }
    pub fn is_in_paste(&self) -> bool { self.is_pasting }
}
impl Default for PasteHandlerState { fn default() -> Self { Self::new() } }
