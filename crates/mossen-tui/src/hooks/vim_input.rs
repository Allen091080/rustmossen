//! Vim input hook (useVimInput.ts).
//! Full vim modal editing on top of text input state.

use super::text_input::{Key, TextCursor, TextInputState};

/// Vim mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode { Normal, Insert, Visual, Command }

/// Vim command state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VimCommand {
    Idle,
    Count(u32),
    Operator(String),
    OperatorCount(String, u32),
    Replace,
    Find(String),
    OperatorFind(String, String),
}

/// State for vim input mode.
#[derive(Debug, Clone)]
pub struct VimInputState {
    pub text_input: TextInputState,
    pub mode: VimMode,
    pub command: VimCommand,
    pub inserted_text: String,
    pub last_change: Option<RecordedChange>,
    pub register: char,
}

#[derive(Debug, Clone)]
pub struct RecordedChange {
    pub command_keys: String,
    pub inserted_text: String,
    pub count: u32,
}

impl VimInputState {
    pub fn new(columns: usize) -> Self {
        Self {
            text_input: TextInputState::new(columns, false),
            mode: VimMode::Insert,
            command: VimCommand::Idle,
            inserted_text: String::new(),
            last_change: None,
            register: '"',
        }
    }

    /// Switch to normal mode.
    pub fn enter_normal_mode(&mut self) {
        if self.mode == VimMode::Insert && !self.inserted_text.is_empty() {
            self.last_change = Some(RecordedChange {
                command_keys: "i".to_string(),
                inserted_text: std::mem::take(&mut self.inserted_text),
                count: 1,
            });
        }
        self.mode = VimMode::Normal;
        self.command = VimCommand::Idle;
    }

    /// Switch to insert mode.
    pub fn enter_insert_mode(&mut self) {
        self.mode = VimMode::Insert;
        self.inserted_text.clear();
    }

    /// Handle a key in the current mode.
    pub fn handle_key(&mut self, key: &Key) {
        match self.mode {
            VimMode::Insert => self.handle_insert_key(key),
            VimMode::Normal => self.handle_normal_key(key),
            VimMode::Visual => self.handle_normal_key(key),
            VimMode::Command => {}
        }
    }

    fn handle_insert_key(&mut self, key: &Key) {
        if key.escape {
            self.enter_normal_mode();
            return;
        }
        if key.modifiers.ctrl {
            self.text_input.handle_key(key);
            return;
        }
        // Track inserted text for dot-repeat
        if key.backspace {
            if !self.inserted_text.is_empty() {
                self.inserted_text.pop();
            }
        } else if !key.input.is_empty() && !key.return_key && !key.tab {
            self.inserted_text.push_str(&key.input);
        }
        self.text_input.handle_key(key);
    }

    fn handle_normal_key(&mut self, key: &Key) {
        if key.escape {
            self.command = VimCommand::Idle;
            return;
        }
        if key.return_key {
            self.text_input.handle_key(key);
            return;
        }
        if key.modifiers.ctrl {
            self.text_input.handle_key(key);
            return;
        }

        let ch = if key.left_arrow { "h" }
            else if key.right_arrow { "l" }
            else if key.up_arrow { "k" }
            else if key.down_arrow { "j" }
            else { key.input.as_str() };

        match (&self.command, ch) {
            (VimCommand::Idle, "i") => self.enter_insert_mode(),
            (VimCommand::Idle, "a") => { self.text_input.cursor.right(); self.enter_insert_mode(); }
            (VimCommand::Idle, "I") => { self.text_input.cursor.start_of_line(); self.enter_insert_mode(); }
            (VimCommand::Idle, "A") => { self.text_input.cursor.end_of_line(); self.enter_insert_mode(); }
            (VimCommand::Idle, "o") => {
                self.text_input.cursor.end_of_line();
                self.text_input.cursor.insert("\n");
                self.enter_insert_mode();
            }
            (VimCommand::Idle, "O") => {
                self.text_input.cursor.start_of_line();
                self.text_input.cursor.insert("\n");
                self.text_input.cursor.left();
                self.enter_insert_mode();
            }
            (VimCommand::Idle, "h") => self.text_input.cursor.left(),
            (VimCommand::Idle, "l") => self.text_input.cursor.right(),
            (VimCommand::Idle, "0") => self.text_input.cursor.start_of_line(),
            (VimCommand::Idle, "$") => self.text_input.cursor.end_of_line(),
            (VimCommand::Idle, "w") => self.text_input.cursor.next_word(),
            (VimCommand::Idle, "b") => self.text_input.cursor.prev_word(),
            (VimCommand::Idle, "x") => self.text_input.cursor.delete_forward(),
            (VimCommand::Idle, "X") => self.text_input.cursor.backspace(),
            (VimCommand::Idle, "d") => self.command = VimCommand::Operator("d".to_string()),
            (VimCommand::Idle, "c") => self.command = VimCommand::Operator("c".to_string()),
            (VimCommand::Idle, "y") => self.command = VimCommand::Operator("y".to_string()),
            (VimCommand::Operator(op), "d") if op == "d" => {
                // dd: delete entire line
                self.text_input.cursor.start_of_line();
                self.text_input.cursor.delete_to_line_end();
                self.command = VimCommand::Idle;
            }
            (VimCommand::Operator(op), "w") => {
                let op = op.clone();
                match op.as_str() {
                    "d" => { self.text_input.cursor.delete_word_after(); }
                    "c" => { self.text_input.cursor.delete_word_after(); self.enter_insert_mode(); return; }
                    _ => {}
                }
                self.command = VimCommand::Idle;
            }
            (VimCommand::Operator(op), "$") => {
                let op = op.clone();
                match op.as_str() {
                    "d" => { self.text_input.cursor.delete_to_line_end(); }
                    "c" => { self.text_input.cursor.delete_to_line_end(); self.enter_insert_mode(); return; }
                    _ => {}
                }
                self.command = VimCommand::Idle;
            }
            (VimCommand::Idle, n) if n.chars().all(|c| c.is_ascii_digit()) && n != "0" => {
                self.command = VimCommand::Count(n.parse().unwrap_or(1));
            }
            (VimCommand::Count(count), n) if n.chars().all(|c| c.is_ascii_digit()) => {
                self.command = VimCommand::Count(count * 10 + n.parse::<u32>().unwrap_or(0));
            }
            (VimCommand::Count(count), motion) => {
                let count = *count;
                for _ in 0..count {
                    match motion {
                        "h" => self.text_input.cursor.left(),
                        "l" => self.text_input.cursor.right(),
                        "w" => self.text_input.cursor.next_word(),
                        "b" => self.text_input.cursor.prev_word(),
                        "x" => self.text_input.cursor.delete_forward(),
                        _ => break,
                    }
                }
                self.command = VimCommand::Idle;
            }
            _ => { self.command = VimCommand::Idle; }
        }
    }

    pub fn value(&self) -> &str { self.text_input.value() }
    pub fn set_value(&mut self, value: &str) { self.text_input.set_value(value); }
    pub fn set_mode(&mut self, mode: VimMode) {
        match mode {
            VimMode::Insert => self.enter_insert_mode(),
            _ => self.enter_normal_mode(),
        }
    }
}

impl Default for VimInputState { fn default() -> Self { Self::new(80) } }
