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

const UNHANDLED_SPECIAL_KEYS: &[&str] = &["ArrowUp", "ArrowDown", "PageUp", "PageDown", "F1", "F2", "F3", "F4"];

impl SearchInputState {
    pub fn new() -> Self {
        Self { query: String::new(), cursor_offset: 0, is_active: false, kill_ring: Vec::new(), kill_ring_index: None }
    }
    pub fn activate(&mut self) { self.is_active = true; }
    pub fn deactivate(&mut self) { self.is_active = false; }
    pub fn set_query(&mut self, query: String) { self.cursor_offset = query.len(); self.query = query; }
    pub fn insert(&mut self, text: &str) {
        self.query.insert_str(self.cursor_offset, text);
        self.cursor_offset += text.len();
    }
    pub fn backspace(&mut self) {
        if self.cursor_offset > 0 {
            self.cursor_offset -= 1;
            self.query.remove(self.cursor_offset);
        }
    }
    pub fn delete(&mut self) {
        if self.cursor_offset < self.query.len() { self.query.remove(self.cursor_offset); }
    }
    pub fn move_left(&mut self) { if self.cursor_offset > 0 { self.cursor_offset -= 1; } }
    pub fn move_right(&mut self) { if self.cursor_offset < self.query.len() { self.cursor_offset += 1; } }
    pub fn move_start(&mut self) { self.cursor_offset = 0; }
    pub fn move_end(&mut self) { self.cursor_offset = self.query.len(); }
    pub fn kill_to_end(&mut self) {
        let killed: String = self.query.drain(self.cursor_offset..).collect();
        if !killed.is_empty() { self.kill_ring.push(killed); }
    }
    pub fn kill_to_start(&mut self) {
        let killed: String = self.query.drain(..self.cursor_offset).collect();
        self.cursor_offset = 0;
        if !killed.is_empty() { self.kill_ring.push(killed); }
    }
    pub fn yank(&mut self) {
        if let Some(text) = self.kill_ring.last().cloned() { self.insert(&text); }
    }
    pub fn clear(&mut self) { self.query.clear(); self.cursor_offset = 0; }
    pub fn is_special_key(key: &str) -> bool {
        UNHANDLED_SPECIAL_KEYS.contains(&key)
    }
}
impl Default for SearchInputState { fn default() -> Self { Self::new() } }
