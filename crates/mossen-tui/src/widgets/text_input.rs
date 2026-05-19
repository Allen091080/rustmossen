//! Text input widget — base component for line editing.
//!
//! Translates useTextInput.ts + BaseTextInput.tsx into a Rust struct
//! with cursor management, history navigation, and input handling.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Text input state — manages buffer, cursor, and selection.
///
/// Translates: useTextInput.ts (18.3KB) hook state.
#[derive(Debug, Clone)]
pub struct TextInputState {
    /// The text content
    pub value: String,
    /// Cursor position (grapheme cluster index)
    pub cursor: usize,
    /// Placeholder text shown when empty
    pub placeholder: String,
    /// Whether the input is focused
    pub focused: bool,
    /// Optional selection range (start, end) in graphemes
    pub selection: Option<(usize, usize)>,
    /// History of previous inputs
    history: Vec<String>,
    /// Current history navigation index (None = editing current)
    history_index: Option<usize>,
    /// Saved current input when navigating history
    saved_current: String,
}

impl TextInputState {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            placeholder: String::new(),
            focused: true,
            selection: None,
            history: Vec::new(),
            history_index: None,
            saved_current: String::new(),
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Number of grapheme clusters in value.
    fn grapheme_count(&self) -> usize {
        self.value.graphemes(true).count()
    }

    /// Insert a character at cursor position.
    pub fn insert_char(&mut self, c: char) {
        let byte_pos = self.cursor_byte_offset();
        self.value.insert(byte_pos, c);
        self.cursor += 1;
        self.selection = None;
    }

    /// Insert a string at cursor position.
    pub fn insert_str(&mut self, s: &str) {
        let byte_pos = self.cursor_byte_offset();
        self.value.insert_str(byte_pos, s);
        self.cursor += s.graphemes(true).count();
        self.selection = None;
    }

    /// Delete the character before the cursor (backspace).
    pub fn delete_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let graphemes: Vec<&str> = self.value.graphemes(true).collect();
        self.cursor -= 1;
        let byte_start = graphemes[..self.cursor]
            .iter()
            .map(|g| g.len())
            .sum::<usize>();
        let byte_end = byte_start + graphemes[self.cursor].len();
        self.value.drain(byte_start..byte_end);
        self.selection = None;
    }

    /// Delete the character at the cursor (delete forward).
    pub fn delete_forward(&mut self) {
        let count = self.grapheme_count();
        if self.cursor >= count {
            return;
        }
        let graphemes: Vec<&str> = self.value.graphemes(true).collect();
        let byte_start = graphemes[..self.cursor]
            .iter()
            .map(|g| g.len())
            .sum::<usize>();
        let byte_end = byte_start + graphemes[self.cursor].len();
        self.value.drain(byte_start..byte_end);
        self.selection = None;
    }

    /// Move cursor left.
    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
        self.selection = None;
    }

    /// Move cursor right.
    pub fn move_right(&mut self) {
        let count = self.grapheme_count();
        if self.cursor < count {
            self.cursor += 1;
        }
        self.selection = None;
    }

    /// Move cursor to start.
    pub fn move_home(&mut self) {
        self.cursor = 0;
        self.selection = None;
    }

    /// Move cursor to end.
    pub fn move_end(&mut self) {
        self.cursor = self.grapheme_count();
        self.selection = None;
    }

    /// Navigate history up (previous entry).
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.saved_current = self.value.clone();
                self.history_index = Some(self.history.len() - 1);
            }
            Some(0) => return,
            Some(idx) => {
                self.history_index = Some(idx - 1);
            }
        }
        if let Some(idx) = self.history_index {
            self.value = self.history[idx].clone();
            self.cursor = self.grapheme_count();
        }
    }

    /// Navigate history down (next entry).
    pub fn history_down(&mut self) {
        match self.history_index {
            None => return,
            Some(idx) if idx >= self.history.len() - 1 => {
                self.history_index = None;
                self.value = self.saved_current.clone();
                self.cursor = self.grapheme_count();
            }
            Some(idx) => {
                self.history_index = Some(idx + 1);
                self.value = self.history[idx + 1].clone();
                self.cursor = self.grapheme_count();
            }
        }
    }

    /// Submit current value: add to history and clear.
    pub fn submit(&mut self) -> String {
        let val = std::mem::take(&mut self.value);
        if !val.trim().is_empty() {
            self.history.push(val.clone());
        }
        self.cursor = 0;
        self.history_index = None;
        self.selection = None;
        val
    }

    /// Clear current input without submitting.
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.selection = None;
        self.history_index = None;
    }

    /// Get byte offset for current cursor position.
    fn cursor_byte_offset(&self) -> usize {
        self.value
            .graphemes(true)
            .take(self.cursor)
            .map(|g| g.len())
            .sum()
    }

    /// Get display width up to cursor.
    pub fn cursor_display_width(&self) -> usize {
        self.value
            .graphemes(true)
            .take(self.cursor)
            .map(|g| UnicodeWidthStr::width(g))
            .sum()
    }
}

impl Default for TextInputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Text input widget for rendering.
pub struct TextInputWidget<'a> {
    pub state: &'a TextInputState,
    pub style: Style,
    pub cursor_style: Style,
    pub placeholder_style: Style,
}

impl<'a> TextInputWidget<'a> {
    pub fn new(state: &'a TextInputState) -> Self {
        Self {
            state,
            style: Style::default(),
            cursor_style: Style::default().add_modifier(Modifier::REVERSED),
            placeholder_style: Style::default().fg(Color::DarkGray),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl<'a> Widget for TextInputWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        if self.state.value.is_empty() && !self.state.placeholder.is_empty() {
            // Render placeholder
            let placeholder = &self.state.placeholder;
            let truncated: String = placeholder
                .graphemes(true)
                .take(area.width as usize)
                .collect();
            buf.set_string(area.x, area.y, &truncated, self.placeholder_style);
            // Show cursor at start if focused
            if self.state.focused {
                buf.set_string(area.x, area.y, " ", self.cursor_style);
            }
            return;
        }

        // Render value with cursor
        let graphemes: Vec<&str> = self.state.value.graphemes(true).collect();
        let mut x = area.x;
        let max_x = area.x + area.width;

        for (i, g) in graphemes.iter().enumerate() {
            let w = UnicodeWidthStr::width(*g) as u16;
            if x + w > max_x {
                break;
            }
            let style = if self.state.focused && i == self.state.cursor {
                self.cursor_style
            } else {
                self.style
            };
            buf.set_string(x, area.y, g, style);
            x += w;
        }

        // Cursor at end
        if self.state.focused && self.state.cursor >= graphemes.len() && x < max_x {
            buf.set_string(x, area.y, " ", self.cursor_style);
        }
    }
}
