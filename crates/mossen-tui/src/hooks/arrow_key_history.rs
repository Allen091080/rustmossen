//! Arrow key history navigation (useArrowKeyHistory.tsx).
//!
//! Manages history navigation using up/down arrow keys in the input.

/// State for arrow key history navigation.
#[derive(Debug, Clone)]
pub struct ArrowKeyHistoryState {
    pub history: Vec<String>,
    pub current_index: Option<usize>,
    pub draft: String,
    pub max_history: usize,
}

impl ArrowKeyHistoryState {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: Vec::new(),
            current_index: None,
            draft: String::new(),
            max_history,
        }
    }

    /// Navigate up (older) in history.
    pub fn up(&mut self, current_value: &str) -> Option<&str> {
        match self.current_index {
            None => {
                if self.history.is_empty() {
                    return None;
                }
                self.draft = current_value.to_string();
                self.current_index = Some(self.history.len() - 1);
                Some(&self.history[self.history.len() - 1])
            }
            Some(idx) => {
                if idx > 0 {
                    self.current_index = Some(idx - 1);
                    Some(&self.history[idx - 1])
                } else {
                    Some(&self.history[0])
                }
            }
        }
    }

    /// Navigate down (newer) in history.
    pub fn down(&mut self) -> Option<&str> {
        match self.current_index {
            None => None,
            Some(idx) => {
                if idx < self.history.len() - 1 {
                    self.current_index = Some(idx + 1);
                    Some(&self.history[idx + 1])
                } else {
                    self.current_index = None;
                    Some(&self.draft)
                }
            }
        }
    }

    /// Reset navigation state (e.g., after submission).
    pub fn reset(&mut self) {
        self.current_index = None;
        self.draft.clear();
    }

    /// Add an entry to history.
    pub fn push(&mut self, entry: String) {
        if entry.trim().is_empty() {
            return;
        }
        // Deduplicate: remove if already present at the end
        if self.history.last().map(|s| s.as_str()) == Some(entry.as_str()) {
            return;
        }
        self.history.push(entry);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
        self.reset();
    }

    pub fn is_navigating(&self) -> bool {
        self.current_index.is_some()
    }
}

impl Default for ArrowKeyHistoryState {
    fn default() -> Self {
        Self::new(100)
    }
}

/// History filter mode. Mirrors TS `HistoryMode = PromptInputMode` — the
/// prompt's current input mode, used to scope history lookups so a bash
/// session only shows bash history and a prompt session only shows prompt
/// history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HistoryMode {
    Prompt,
    Bash,
    Memory,
    Plan,
}

impl HistoryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            HistoryMode::Prompt => "prompt",
            HistoryMode::Bash => "bash",
            HistoryMode::Memory => "memory",
            HistoryMode::Plan => "plan",
        }
    }
}
