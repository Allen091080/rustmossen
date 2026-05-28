//! Diff data hook (useDiffData.ts).
//!
//! Fetches and caches git diff data for display in the UI.

/// A diff hunk from git.
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
    pub header: String,
}

/// A single line in a diff.
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub content: String,
    pub line_type: DiffLineType,
    pub old_line_number: Option<u32>,
    pub new_line_number: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineType {
    Context,
    Added,
    Removed,
}

/// State for diff data loading and caching.
#[derive(Debug, Clone)]
pub struct DiffDataState {
    pub file_path: String,
    pub hunks: Vec<DiffHunk>,
    pub is_loading: bool,
    pub error: Option<String>,
    pub raw_diff: Option<String>,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
}

impl DiffDataState {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            hunks: Vec::new(),
            is_loading: false,
            error: None,
            raw_diff: None,
            old_content: None,
            new_content: None,
        }
    }

    /// Start loading diff data.
    pub fn start_loading(&mut self) {
        self.is_loading = true;
        self.error = None;
    }

    /// Set the loaded diff data.
    pub fn set_data(&mut self, hunks: Vec<DiffHunk>, raw_diff: String) {
        self.hunks = hunks;
        self.raw_diff = Some(raw_diff);
        self.is_loading = false;
    }

    /// Set file contents for inline diff display.
    pub fn set_contents(&mut self, old_content: String, new_content: String) {
        self.old_content = Some(old_content);
        self.new_content = Some(new_content);
    }

    /// Mark as errored.
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.is_loading = false;
    }

    /// Check if data is available.
    pub fn has_data(&self) -> bool {
        !self.hunks.is_empty()
    }

    /// Get total lines changed.
    pub fn total_changes(&self) -> (u32, u32) {
        let mut added = 0u32;
        let mut removed = 0u32;
        for hunk in &self.hunks {
            for line in &hunk.lines {
                match line.line_type {
                    DiffLineType::Added => added += 1,
                    DiffLineType::Removed => removed += 1,
                    DiffLineType::Context => {}
                }
            }
        }
        (added, removed)
    }
}
