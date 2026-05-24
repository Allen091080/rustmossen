//! Search input hook (useSearchInput.ts).
//! Manages search/filter input with cursor, kill-ring, and keybindings.

use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct SearchInputState {
    pub query: String,
    pub cursor_offset: usize,
    pub is_active: bool,
    pub kill_ring: Vec<String>,
    pub kill_ring_index: Option<usize>,
}

const UNHANDLED_SPECIAL_KEYS: &[&str] = &[
    "ArrowUp",
    "ArrowDown",
    "PageUp",
    "PageDown",
    "F1",
    "F2",
    "F3",
    "F4",
];

impl SearchInputState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            cursor_offset: 0,
            is_active: false,
            kill_ring: Vec::new(),
            kill_ring_index: None,
        }
    }
    pub fn activate(&mut self) {
        self.is_active = true;
    }
    pub fn deactivate(&mut self) {
        self.is_active = false;
    }
    pub fn set_query(&mut self, query: String) {
        self.cursor_offset = query.len();
        self.query = query;
    }
    pub fn insert(&mut self, text: &str) {
        self.cursor_offset = clamp_char_boundary(&self.query, self.cursor_offset);
        self.query.insert_str(self.cursor_offset, text);
        self.cursor_offset += text.len();
    }
    pub fn backspace(&mut self) {
        self.cursor_offset = clamp_char_boundary(&self.query, self.cursor_offset);
        let previous = previous_char_boundary(&self.query, self.cursor_offset);
        if previous < self.cursor_offset {
            self.query.drain(previous..self.cursor_offset);
            self.cursor_offset = previous;
        }
    }
    pub fn delete(&mut self) {
        self.cursor_offset = clamp_char_boundary(&self.query, self.cursor_offset);
        let next = next_char_boundary(&self.query, self.cursor_offset);
        if next > self.cursor_offset {
            self.query.drain(self.cursor_offset..next);
        }
    }
    pub fn move_left(&mut self) {
        self.cursor_offset = previous_char_boundary(&self.query, self.cursor_offset);
    }
    pub fn move_right(&mut self) {
        self.cursor_offset = next_char_boundary(&self.query, self.cursor_offset);
    }
    pub fn move_start(&mut self) {
        self.cursor_offset = 0;
    }
    pub fn move_end(&mut self) {
        self.cursor_offset = self.query.len();
    }
    pub fn kill_to_end(&mut self) {
        self.cursor_offset = clamp_char_boundary(&self.query, self.cursor_offset);
        let killed: String = self.query.drain(self.cursor_offset..).collect();
        if !killed.is_empty() {
            self.kill_ring.push(killed);
        }
    }
    pub fn kill_to_start(&mut self) {
        self.cursor_offset = clamp_char_boundary(&self.query, self.cursor_offset);
        let killed: String = self.query.drain(..self.cursor_offset).collect();
        self.cursor_offset = 0;
        if !killed.is_empty() {
            self.kill_ring.push(killed);
        }
    }
    pub fn yank(&mut self) {
        if let Some(text) = self.kill_ring.last().cloned() {
            self.insert(&text);
        }
    }
    pub fn clear(&mut self) {
        self.query.clear();
        self.cursor_offset = 0;
    }
    pub fn is_special_key(key: &str) -> bool {
        UNHANDLED_SPECIAL_KEYS.contains(&key)
    }
}
impl Default for SearchInputState {
    fn default() -> Self {
        Self::new()
    }
}

fn clamp_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn previous_char_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_char_boundary(text, offset);
    text[..offset]
        .char_indices()
        .next_back()
        .map_or(0, |(idx, _)| idx)
}

fn next_char_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_char_boundary(text, offset);
    if offset >= text.len() {
        return text.len();
    }
    offset + text[offset..].chars().next().map_or(0, char::len_utf8)
}

#[cfg(test)]
mod tests {
    use super::SearchInputState;

    #[test]
    fn search_input_edits_multibyte_text_on_char_boundaries() {
        let mut input = SearchInputState::new();
        input.set_query("逐行阅读代码".to_string());

        input.move_left();
        input.backspace();
        assert_eq!(input.query, "逐行阅读码");

        input.insert("代");
        assert_eq!(input.query, "逐行阅读代码");

        input.cursor_offset = 1;
        input.delete();
        assert_eq!(input.query, "行阅读代码");

        input.move_end();
        input.kill_to_start();
        assert_eq!(
            input.kill_ring.last().map(String::as_str),
            Some("行阅读代码")
        );
        assert!(input.query.is_empty());
    }
}
