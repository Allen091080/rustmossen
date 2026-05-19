//! Assistant history hook (useAssistantHistory.ts).
//!
//! Manages session history loading for the assistant mode, including
//! pagination, sentinel messages, and scroll anchoring.

use std::time::Instant;

/// A page of history loaded from storage.
#[derive(Debug, Clone)]
pub struct HistoryPage {
    pub messages: Vec<HistoryMessage>,
    pub has_more: bool,
    pub first_id: Option<String>,
}

/// A message in history.
#[derive(Debug, Clone)]
pub struct HistoryMessage {
    pub uuid: String,
    pub content: String,
    pub role: MessageRole,
    pub timestamp: String,
    pub is_meta: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// State for assistant history management.
#[derive(Debug, Clone)]
pub struct AssistantHistoryState {
    pub messages: Vec<HistoryMessage>,
    pub cursor: Option<String>,
    pub is_loading: bool,
    pub has_older: bool,
    pub sentinel_uuid: String,
    pub anchor_before_height: Option<f64>,
    pub anchor_count: Option<usize>,
    pub fill_budget: u32,
    pub last_load_time: Option<Instant>,
}

impl AssistantHistoryState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            cursor: None,
            is_loading: false,
            has_older: false,
            sentinel_uuid: uuid::Uuid::new_v4().to_string(),
            anchor_before_height: None,
            anchor_count: None,
            fill_budget: 0,
            last_load_time: None,
        }
    }

    /// Prepend a page of messages at the front.
    pub fn prepend_page(&mut self, page: HistoryPage, is_initial: bool) {
        self.cursor = if page.has_more { page.first_id.clone() } else { None };
        self.has_older = page.has_more;

        let mut new_messages = page.messages;
        // Remove existing sentinel if present at index 0
        if !self.messages.is_empty() && self.messages[0].uuid == self.sentinel_uuid {
            self.messages.remove(0);
        }
        // Add sentinel at front if there is more history
        if !page.has_more {
            new_messages.insert(0, HistoryMessage {
                uuid: self.sentinel_uuid.clone(),
                content: "--- Start of conversation ---".to_string(),
                role: MessageRole::System,
                timestamp: chrono::Utc::now().to_rfc3339(),
                is_meta: true,
            });
        }
        new_messages.append(&mut self.messages);
        self.messages = new_messages;
        self.last_load_time = Some(Instant::now());
    }

    /// Start loading older messages.
    pub fn start_loading(&mut self) {
        self.is_loading = true;
    }

    /// Finish loading.
    pub fn finish_loading(&mut self) {
        self.is_loading = false;
    }

    /// Check if we should attempt to load more (for viewport filling).
    pub fn should_fill_viewport(&self) -> bool {
        self.fill_budget > 0 && self.has_older && !self.is_loading
    }

    /// Decrement fill budget.
    pub fn decrement_fill_budget(&mut self) {
        if self.fill_budget > 0 {
            self.fill_budget -= 1;
        }
    }
}

impl Default for AssistantHistoryState {
    fn default() -> Self {
        Self::new()
    }
}
