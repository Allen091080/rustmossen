//! History search hook (useHistorySearch.ts).
//!
//! Implements Ctrl+R reverse history search functionality.

/// State for history search.
#[derive(Debug, Clone)]
pub struct HistorySearchState {
    pub is_active: bool,
    pub query: String,
    pub results: Vec<HistorySearchResult>,
    pub selected_index: usize,
    pub history: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HistorySearchResult {
    pub text: String,
    pub index: usize,
    pub match_start: usize,
    pub match_end: usize,
}

impl HistorySearchState {
    pub fn new(history: Vec<String>) -> Self {
        Self {
            is_active: false,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            history,
        }
    }

    /// Activate history search mode.
    pub fn activate(&mut self) {
        self.is_active = true;
        self.query.clear();
        self.results.clear();
        self.selected_index = 0;
    }

    /// Deactivate history search.
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.query.clear();
        self.results.clear();
    }

    /// Update the search query and recompute results.
    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.selected_index = 0;
        self.search();
    }

    /// Append a character to the query.
    pub fn push_char(&mut self, ch: char) {
        self.query.push(ch);
        self.search();
    }

    /// Remove the last character from the query.
    pub fn pop_char(&mut self) {
        self.query.pop();
        self.search();
    }

    /// Perform the search.
    fn search(&mut self) {
        if self.query.is_empty() {
            self.results.clear();
            return;
        }
        let query_lower = self.query.to_lowercase();
        self.results = self.history.iter().enumerate().rev()
            .filter_map(|(idx, entry)| {
                let entry_lower = entry.to_lowercase();
                entry_lower.find(&query_lower).map(|pos| HistorySearchResult {
                    text: entry.clone(),
                    index: idx,
                    match_start: pos,
                    match_end: pos + self.query.len(),
                })
            })
            .collect();
        if self.selected_index >= self.results.len() {
            self.selected_index = 0;
        }
    }

    /// Select the next result (older).
    pub fn next(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.results.len();
        }
    }

    /// Select the previous result (newer).
    pub fn prev(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = if self.selected_index == 0 {
                self.results.len() - 1
            } else {
                self.selected_index - 1
            };
        }
    }

    /// Get the currently selected result.
    pub fn selected(&self) -> Option<&HistorySearchResult> {
        self.results.get(self.selected_index)
    }

    /// Accept the current selection.
    pub fn accept(&mut self) -> Option<String> {
        let result = self.selected().map(|r| r.text.clone());
        self.deactivate();
        result
    }
}

impl Default for HistorySearchState {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}
