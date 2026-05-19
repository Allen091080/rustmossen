//! Selection hook (use-selection.ts).
//! Manages text selection state in the terminal.

#[derive(Debug, Clone)]
pub struct SelectionHookState {
    pub active: bool,
    pub start: Option<(u16, u16)>,
    pub end: Option<(u16, u16)>,
    pub text: String,
}
impl SelectionHookState {
    pub fn new() -> Self { Self { active: false, start: None, end: None, text: String::new() } }
    pub fn start_selection(&mut self, col: u16, row: u16) { self.active = true; self.start = Some((col, row)); self.end = Some((col, row)); }
    pub fn update_selection(&mut self, col: u16, row: u16) { if self.active { self.end = Some((col, row)); } }
    pub fn end_selection(&mut self, text: String) { self.text = text; self.active = false; }
    pub fn clear(&mut self) { self.active = false; self.start = None; self.end = None; self.text.clear(); }
    pub fn has_selection(&self) -> bool { !self.text.is_empty() }
}
impl Default for SelectionHookState { fn default() -> Self { Self::new() } }

/// Hook-equivalent useSelection — returns a mutable handle to the selection.
pub fn use_selection(state: &mut SelectionHookState) -> &mut SelectionHookState {
    state
}

/// Hook-equivalent useHasSelection — whether a selection exists right now.
pub fn use_has_selection(state: &SelectionHookState) -> bool {
    state.has_selection()
}
