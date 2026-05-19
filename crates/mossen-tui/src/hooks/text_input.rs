//! Text input hook (useTextInput.ts).
//! Full text editing state with cursor movement, kill-ring, history, multiline.

use super::double_press::DoublePressState;

/// Key modifiers for input handling.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub meta: bool,
    pub shift: bool,
    pub fn_key: bool,
}

/// Key event representation.
#[derive(Debug, Clone)]
pub struct Key {
    pub input: String,
    pub modifiers: KeyModifiers,
    pub escape: bool,
    pub return_key: bool,
    pub backspace: bool,
    pub delete: bool,
    pub tab: bool,
    pub up_arrow: bool,
    pub down_arrow: bool,
    pub left_arrow: bool,
    pub right_arrow: bool,
    pub home: bool,
    pub end: bool,
    pub page_up: bool,
    pub page_down: bool,
}

impl Key {
    pub fn char(ch: char) -> Self {
        Self {
            input: ch.to_string(), modifiers: KeyModifiers::default(),
            escape: false, return_key: false, backspace: false, delete: false, tab: false,
            up_arrow: false, down_arrow: false, left_arrow: false, right_arrow: false,
            home: false, end: false, page_up: false, page_down: false,
        }
    }
}

/// Cursor position in text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    pub line: usize,
    pub column: usize,
}

/// Kill ring for cut/paste operations.
#[derive(Debug, Clone)]
pub struct KillRing {
    pub entries: Vec<String>,
    pub current_index: Option<usize>,
    pub accumulating: bool,
    pub yank_start: Option<usize>,
    pub yank_length: Option<usize>,
}

impl KillRing {
    pub fn new() -> Self { Self { entries: Vec::new(), current_index: None, accumulating: false, yank_start: None, yank_length: None } }
    pub fn push(&mut self, text: String, prepend: bool) {
        if self.accumulating && !self.entries.is_empty() {
            let last = self.entries.last_mut().unwrap();
            if prepend { *last = format!("{}{}", text, last); } else { last.push_str(&text); }
        } else {
            self.entries.push(text);
            if self.entries.len() > 60 { self.entries.remove(0); }
        }
        self.accumulating = true;
    }
    pub fn last(&self) -> &str { self.entries.last().map(|s| s.as_str()).unwrap_or("") }
    pub fn yank_pop(&mut self) -> Option<&str> {
        if self.entries.len() < 2 { return None; }
        let idx = match self.current_index {
            Some(i) if i > 0 => i - 1,
            Some(_) => self.entries.len() - 1,
            None => self.entries.len().saturating_sub(2),
        };
        self.current_index = Some(idx);
        self.entries.get(idx).map(|s| s.as_str())
    }
    pub fn reset_accumulation(&mut self) { self.accumulating = false; }
    pub fn reset_yank(&mut self) { self.yank_start = None; self.yank_length = None; self.current_index = None; }
    pub fn record_yank(&mut self, start: usize, length: usize) { self.yank_start = Some(start); self.yank_length = Some(length); }
}
impl Default for KillRing { fn default() -> Self { Self::new() } }

/// Text cursor with editing operations.
#[derive(Debug, Clone)]
pub struct TextCursor {
    pub text: String,
    pub offset: usize,
    pub columns: usize,
}

impl TextCursor {
    pub fn new(text: &str, columns: usize, offset: usize) -> Self {
        Self { text: text.to_string(), offset: offset.min(text.len()), columns }
    }
    pub fn left(&mut self) { if self.offset > 0 { self.offset -= 1; } }
    pub fn right(&mut self) { if self.offset < self.text.len() { self.offset += 1; } }
    pub fn start_of_line(&mut self) {
        let line_start = self.text[..self.offset].rfind('\n').map_or(0, |p| p + 1);
        self.offset = line_start;
    }
    pub fn end_of_line(&mut self) {
        let line_end = self.text[self.offset..].find('\n').map_or(self.text.len(), |p| self.offset + p);
        self.offset = line_end;
    }
    pub fn backspace(&mut self) {
        if self.offset > 0 { self.offset -= 1; self.text.remove(self.offset); }
    }
    pub fn delete_forward(&mut self) {
        if self.offset < self.text.len() { self.text.remove(self.offset); }
    }
    pub fn insert(&mut self, text: &str) { self.text.insert_str(self.offset, text); self.offset += text.len(); }
    pub fn delete_to_line_end(&mut self) -> String {
        let end = self.text[self.offset..].find('\n').map_or(self.text.len(), |p| self.offset + p);
        let killed: String = self.text.drain(self.offset..end).collect();
        killed
    }
    pub fn delete_to_line_start(&mut self) -> String {
        let start = self.text[..self.offset].rfind('\n').map_or(0, |p| p + 1);
        let killed: String = self.text.drain(start..self.offset).collect();
        self.offset = start;
        killed
    }
    pub fn delete_word_before(&mut self) -> String {
        let new_offset = self.prev_word_boundary();
        let killed: String = self.text.drain(new_offset..self.offset).collect();
        self.offset = new_offset;
        killed
    }
    pub fn delete_word_after(&mut self) -> String {
        let end = self.next_word_boundary();
        let killed: String = self.text.drain(self.offset..end).collect();
        killed
    }
    pub fn prev_word(&mut self) { self.offset = self.prev_word_boundary(); }
    pub fn next_word(&mut self) { self.offset = self.next_word_boundary(); }
    fn prev_word_boundary(&self) -> usize {
        if self.offset == 0 { return 0; }
        let bytes = self.text.as_bytes();
        let mut i = self.offset - 1;
        while i > 0 && !bytes[i].is_ascii_alphanumeric() { i -= 1; }
        while i > 0 && bytes[i - 1].is_ascii_alphanumeric() { i -= 1; }
        i
    }
    fn next_word_boundary(&self) -> usize {
        let bytes = self.text.as_bytes();
        let mut i = self.offset;
        while i < bytes.len() && !bytes[i].is_ascii_alphanumeric() { i += 1; }
        while i < bytes.len() && bytes[i].is_ascii_alphanumeric() { i += 1; }
        i
    }
    pub fn position(&self) -> CursorPosition {
        let before = &self.text[..self.offset];
        let line = before.matches('\n').count();
        let col = before.rfind('\n').map_or(self.offset, |p| self.offset - p - 1);
        CursorPosition { line, column: col }
    }
    pub fn is_at_start(&self) -> bool { self.offset == 0 }
    pub fn is_at_end(&self) -> bool { self.offset >= self.text.len() }
}

/// Full text input state.
#[derive(Debug, Clone)]
pub struct TextInputState {
    pub cursor: TextCursor,
    pub kill_ring: KillRing,
    pub ctrl_c_handler: DoublePressState,
    pub escape_handler: DoublePressState,
    pub multiline: bool,
    pub offset: usize,
    pub rendered_value: String,
}

impl TextInputState {
    pub fn new(columns: usize, multiline: bool) -> Self {
        Self {
            cursor: TextCursor::new("", columns, 0),
            kill_ring: KillRing::new(),
            ctrl_c_handler: DoublePressState::new(),
            escape_handler: DoublePressState::new(),
            multiline, offset: 0, rendered_value: String::new(),
        }
    }
    pub fn set_value(&mut self, value: &str) {
        self.cursor = TextCursor::new(value, self.cursor.columns, self.cursor.offset.min(value.len()));
    }
    pub fn value(&self) -> &str { &self.cursor.text }
    pub fn handle_key(&mut self, key: &Key) {
        if key.modifiers.ctrl {
            match key.input.as_str() {
                "a" => self.cursor.start_of_line(),
                "e" => self.cursor.end_of_line(),
                "b" => self.cursor.left(),
                "f" => self.cursor.right(),
                "k" => { let k = self.cursor.delete_to_line_end(); self.kill_ring.push(k, false); }
                "u" => { let k = self.cursor.delete_to_line_start(); self.kill_ring.push(k, true); }
                "w" => { let k = self.cursor.delete_word_before(); self.kill_ring.push(k, true); }
                "y" => { let t = self.kill_ring.last().to_string(); let start = self.cursor.offset; self.cursor.insert(&t); self.kill_ring.record_yank(start, t.len()); }
                "d" => self.cursor.delete_forward(),
                "h" => self.cursor.backspace(),
                _ => {}
            }
            if !matches!(key.input.as_str(), "k" | "u" | "w") { self.kill_ring.reset_accumulation(); }
            if key.input != "y" { self.kill_ring.reset_yank(); }
        } else if key.modifiers.meta {
            match key.input.as_str() {
                "b" => self.cursor.prev_word(),
                "f" => self.cursor.next_word(),
                "d" => { let k = self.cursor.delete_word_after(); self.kill_ring.push(k, false); }
                _ => {}
            }
        } else if key.backspace {
            self.cursor.backspace(); self.kill_ring.reset_accumulation(); self.kill_ring.reset_yank();
        } else if key.delete {
            self.cursor.delete_forward();
        } else if key.left_arrow { self.cursor.left();
        } else if key.right_arrow { self.cursor.right();
        } else if key.home { self.cursor.start_of_line();
        } else if key.end { self.cursor.end_of_line();
        } else if !key.input.is_empty() && !key.escape && !key.return_key && !key.tab {
            self.cursor.insert(&key.input); self.kill_ring.reset_accumulation(); self.kill_ring.reset_yank();
        }
        self.offset = self.cursor.offset;
    }
}
impl Default for TextInputState { fn default() -> Self { Self::new(80, false) } }

/// Configuration props passed to a `useTextInput`-equivalent text input
/// hook. Translated from TS `UseTextInputProps` — Rust uses owned data
/// plus a few config flags. Callback closures live on the consumer side
/// (the Rust port wires events via event loops, not React callbacks).
#[derive(Debug, Clone)]
pub struct UseTextInputProps {
    /// Initial text value.
    pub value: String,
    /// Disables in-input cursor movement for arrow up/down — useful when
    /// the parent owns history navigation.
    pub disable_cursor_movement_for_up_down_keys: bool,
    /// Disables the double-press-Esc handler.
    pub disable_escape_double_press: bool,
    /// Mask character used for password-style inputs; empty when no mask.
    pub mask: String,
    /// Single-char rendered for the cursor.
    pub cursor_char: String,
    /// Highlight pasted text segments.
    pub highlight_pasted_text: bool,
    /// True if focus is on the input.
    pub focus: bool,
    /// True for multi-line editing.
    pub multiline: bool,
    /// Maximum visible lines in the rendered output (vertical clamp).
    pub max_visible_lines: Option<usize>,
    /// Number of columns available for layout.
    pub columns: usize,
    /// Offset (rows) into the rendered output, controlled by parent.
    pub external_offset: usize,
}

impl Default for UseTextInputProps {
    fn default() -> Self {
        Self {
            value: String::new(),
            disable_cursor_movement_for_up_down_keys: false,
            disable_escape_double_press: false,
            mask: String::new(),
            cursor_char: "█".to_string(),
            highlight_pasted_text: false,
            focus: true,
            multiline: false,
            max_visible_lines: None,
            columns: 80,
            external_offset: 0,
        }
    }
}
