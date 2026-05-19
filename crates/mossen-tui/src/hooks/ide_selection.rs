//! IDE selection hook (useIdeSelection.ts).
//!
//! Tracks the user's current text selection in the IDE.

/// State for IDE selection tracking.
#[derive(Debug, Clone)]
pub struct IdeSelectionState {
    pub file_path: Option<String>,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub selected_text: String,
    pub is_active: bool,
}

impl IdeSelectionState {
    pub fn new() -> Self {
        Self {
            file_path: None,
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            selected_text: String::new(),
            is_active: false,
        }
    }

    /// Update the selection.
    pub fn update(
        &mut self,
        file_path: String,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        text: String,
    ) {
        self.file_path = Some(file_path);
        self.start_line = start_line;
        self.start_col = start_col;
        self.end_line = end_line;
        self.end_col = end_col;
        self.selected_text = text;
        self.is_active = true;
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.file_path = None;
        self.start_line = 0;
        self.start_col = 0;
        self.end_line = 0;
        self.end_col = 0;
        self.selected_text.clear();
        self.is_active = false;
    }

    /// Check if there is an active selection.
    pub fn has_selection(&self) -> bool {
        self.is_active && !self.selected_text.is_empty()
    }

    /// Get selection as a context reference string.
    pub fn as_context_ref(&self) -> Option<String> {
        if !self.has_selection() {
            return None;
        }
        let path = self.file_path.as_deref().unwrap_or("unknown");
        Some(format!("{}:{}-{}", path, self.start_line, self.end_line))
    }
}

impl Default for IdeSelectionState {
    fn default() -> Self {
        Self::new()
    }
}

/// A point in an IDE selection — line + character offset within the line.
///
/// TS source: `export type SelectionPoint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    pub line: u32,
    pub character: u32,
}

/// Raw selection data sent by the IDE MCP server.
///
/// TS source: `export type SelectionData`.
#[derive(Debug, Clone)]
pub struct SelectionData {
    pub selection: Option<(SelectionPoint, SelectionPoint)>,
    pub text: Option<String>,
    pub file_path: Option<String>,
}
