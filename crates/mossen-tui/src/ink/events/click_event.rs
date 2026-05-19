//! Click event (click-event.ts).
#[derive(Debug, Clone)]
pub struct ClickEvent {
    pub col: u16, pub row: u16,
    pub local_col: u16, pub local_row: u16,
    pub cell_is_blank: bool,
    stopped: bool,
}
impl ClickEvent {
    pub fn new(col: u16, row: u16, cell_is_blank: bool) -> Self {
        Self { col, row, local_col: 0, local_row: 0, cell_is_blank, stopped: false }
    }
    pub fn stop_immediate_propagation(&mut self) { self.stopped = true; }
    pub fn did_stop(&self) -> bool { self.stopped }
    pub fn set_local(&mut self, col: u16, row: u16) { self.local_col = col; self.local_row = row; }
}
