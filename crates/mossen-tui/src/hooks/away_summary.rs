//! Away summary hook (useAwaySummary.ts).
//!
//! Manages the "while you were away" summary card that shows
//! what happened during background processing.

use std::time::Instant;

/// Summary of work done while user was away.
#[derive(Debug, Clone)]
pub struct AwaySummary {
    pub title: String,
    pub items: Vec<AwaySummaryItem>,
    pub duration_ms: u64,
    pub started_at: String,
    pub completed_at: String,
}

/// A single item in the away summary.
#[derive(Debug, Clone)]
pub struct AwaySummaryItem {
    pub description: String,
    pub item_type: AwaySummaryItemType,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwaySummaryItemType {
    FileEdit,
    FileCreate,
    Command,
    Search,
    Other,
}

/// State for the away summary hook.
#[derive(Debug, Clone)]
pub struct AwaySummaryState {
    pub summary: Option<AwaySummary>,
    pub is_visible: bool,
    pub dismissed: bool,
    pub shown_at: Option<Instant>,
}

impl AwaySummaryState {
    pub fn new() -> Self {
        Self {
            summary: None,
            is_visible: false,
            dismissed: false,
            shown_at: None,
        }
    }

    /// Set the away summary when returning from background.
    pub fn set_summary(&mut self, summary: AwaySummary) {
        self.summary = Some(summary);
        self.is_visible = true;
        self.dismissed = false;
        self.shown_at = Some(Instant::now());
    }

    /// Dismiss the summary card.
    pub fn dismiss(&mut self) {
        self.is_visible = false;
        self.dismissed = true;
    }

    /// Clear the summary entirely.
    pub fn clear(&mut self) {
        self.summary = None;
        self.is_visible = false;
        self.dismissed = false;
        self.shown_at = None;
    }

    /// Check if summary should auto-dismiss (after 30s).
    pub fn should_auto_dismiss(&self) -> bool {
        if let Some(shown_at) = self.shown_at {
            self.is_visible && shown_at.elapsed().as_secs() >= 30
        } else {
            false
        }
    }
}

impl Default for AwaySummaryState {
    fn default() -> Self {
        Self::new()
    }
}
