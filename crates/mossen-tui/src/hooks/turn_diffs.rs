//! Turn Diffs hook (useTurnDiffs.ts).
//! Collects and displays diffs made during a single turn.

#[derive(Debug, Clone)]
pub struct TurnDiffsState {
    pub active: bool,
    pub initialized: bool,
}

impl TurnDiffsState {
    pub fn new() -> Self {
        Self {
            active: false,
            initialized: false,
        }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
    pub fn activate(&mut self) {
        self.active = true;
    }
    pub fn deactivate(&mut self) {
        self.active = false;
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
}
impl Default for TurnDiffsState {
    fn default() -> Self {
        Self::new()
    }
}

/// One hunk in a structured patch — a contiguous range of changed lines
/// between two file revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredPatchHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    /// Raw patch lines, each prefixed with " ", "+", or "-".
    pub lines: Vec<String>,
}

/// All edits touching a single file inside one turn. Translated from
/// `TurnFileDiff` in TS — same shape: path + hunks + line stats + is-new
/// flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnFileDiff {
    pub file_path: String,
    pub hunks: Vec<StructuredPatchHunk>,
    pub is_new_file: bool,
    pub lines_added: u32,
    pub lines_removed: u32,
}

impl TurnFileDiff {
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            hunks: Vec::new(),
            is_new_file: false,
            lines_added: 0,
            lines_removed: 0,
        }
    }
}
