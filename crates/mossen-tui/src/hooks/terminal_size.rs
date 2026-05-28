//! Terminal Size hook (useTerminalSize.ts).
//! Provides reactive terminal dimensions (rows/columns).

/// Terminal dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
}

#[derive(Debug, Clone)]
pub struct TerminalSizeState {
    pub size: TerminalSize,
    pub min_columns: u16,
    pub min_rows: u16,
}

impl TerminalSizeState {
    pub fn new(columns: u16, rows: u16) -> Self {
        Self {
            size: TerminalSize { columns, rows },
            min_columns: 40,
            min_rows: 10,
        }
    }
    pub fn update(&mut self, columns: u16, rows: u16) {
        self.size = TerminalSize { columns, rows };
    }
    pub fn columns(&self) -> u16 {
        self.size.columns
    }
    pub fn rows(&self) -> u16 {
        self.size.rows
    }
    pub fn is_too_small(&self) -> bool {
        self.size.columns < self.min_columns || self.size.rows < self.min_rows
    }
}
impl Default for TerminalSizeState {
    fn default() -> Self {
        Self::new(80, 24)
    }
}
