//! Background task navigation hook (useBackgroundTaskNavigation.ts).
//!
//! Manages navigation between background tasks in the task panel.

/// State for background task navigation.
#[derive(Debug, Clone)]
pub struct BackgroundTaskNavigationState {
    pub task_ids: Vec<String>,
    pub selected_index: Option<usize>,
    pub is_expanded: bool,
}

impl BackgroundTaskNavigationState {
    pub fn new() -> Self {
        Self {
            task_ids: Vec::new(),
            selected_index: None,
            is_expanded: false,
        }
    }

    /// Set the list of available task IDs.
    pub fn set_tasks(&mut self, ids: Vec<String>) {
        self.task_ids = ids;
        // Reset selection if out of bounds
        if let Some(idx) = self.selected_index {
            if idx >= self.task_ids.len() {
                self.selected_index = if self.task_ids.is_empty() {
                    None
                } else {
                    Some(self.task_ids.len() - 1)
                };
            }
        }
    }

    /// Navigate to the next task.
    pub fn next(&mut self) -> Option<&str> {
        if self.task_ids.is_empty() {
            return None;
        }
        let idx = match self.selected_index {
            Some(i) => (i + 1) % self.task_ids.len(),
            None => 0,
        };
        self.selected_index = Some(idx);
        Some(&self.task_ids[idx])
    }

    /// Navigate to the previous task.
    pub fn prev(&mut self) -> Option<&str> {
        if self.task_ids.is_empty() {
            return None;
        }
        let idx = match self.selected_index {
            Some(0) => self.task_ids.len() - 1,
            Some(i) => i - 1,
            None => self.task_ids.len() - 1,
        };
        self.selected_index = Some(idx);
        Some(&self.task_ids[idx])
    }

    /// Get the currently selected task ID.
    pub fn selected(&self) -> Option<&str> {
        self.selected_index.map(|i| self.task_ids[i].as_str())
    }

    /// Toggle expanded view.
    pub fn toggle_expand(&mut self) {
        self.is_expanded = !self.is_expanded;
    }
}

impl Default for BackgroundTaskNavigationState {
    fn default() -> Self {
        Self::new()
    }
}
