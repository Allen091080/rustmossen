#!/usr/bin/env python3
"""Generate all hooks/*.rs files for mossen-tui crate."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/hooks"

# Each tuple: (filename, content)
files = []

files.append(("after_first_render.rs", '''//! After first render hook (useAfterFirstRender.ts).
//!
//! Fires a one-shot callback after the first render cycle completes.
//! In the TS version this checked env vars and called process.exit for
//! startup-time measurement.

use std::sync::atomic::{AtomicBool, Ordering};

/// State for the after-first-render hook.
#[derive(Debug)]
pub struct AfterFirstRenderState {
    fired: AtomicBool,
}

impl AfterFirstRenderState {
    pub fn new() -> Self {
        Self {
            fired: AtomicBool::new(false),
        }
    }

    /// Call once after the first render. Returns true if this was the first call.
    pub fn mark_rendered(&self) -> bool {
        !self.fired.swap(true, Ordering::SeqCst)
    }

    /// Check if the first render has occurred.
    pub fn has_rendered(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }

    /// Check if we should exit after first render (ant user type + env flag).
    pub fn should_exit_after_render() -> bool {
        let user_type = std::env::var("USER_TYPE").unwrap_or_default();
        let exit_flag = std::env::var("MOSSEN_CODE_EXIT_AFTER_FIRST_RENDER").unwrap_or_default();
        user_type == "ant" && (exit_flag == "1" || exit_flag.eq_ignore_ascii_case("true"))
    }

    /// Get startup time in milliseconds (from process start).
    pub fn startup_time_ms() -> u64 {
        // Use std::time::SystemTime as approximation
        // In production this would use process uptime
        0
    }
}

impl Default for AfterFirstRenderState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("api_key_verification.rs", '''//! API key verification hook (useApiKeyVerification.ts).
//!
//! Manages the flow for verifying an API key: tracks the key value,
//! loading state, verification result, and error messages.

use std::time::Instant;

/// Verification status for an API key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyVerificationStatus {
    Idle,
    Verifying,
    Valid,
    Invalid { reason: String },
    Error { message: String },
}

/// State for API key verification.
#[derive(Debug, Clone)]
pub struct ApiKeyVerificationState {
    pub api_key: String,
    pub status: ApiKeyVerificationStatus,
    pub last_verified_at: Option<Instant>,
    pub attempts: u32,
}

impl ApiKeyVerificationState {
    pub fn new() -> Self {
        Self {
            api_key: String::new(),
            status: ApiKeyVerificationStatus::Idle,
            last_verified_at: None,
            attempts: 0,
        }
    }

    pub fn set_key(&mut self, key: String) {
        self.api_key = key;
        self.status = ApiKeyVerificationStatus::Idle;
    }

    pub fn start_verification(&mut self) {
        self.status = ApiKeyVerificationStatus::Verifying;
        self.attempts += 1;
    }

    pub fn mark_valid(&mut self) {
        self.status = ApiKeyVerificationStatus::Valid;
        self.last_verified_at = Some(Instant::now());
    }

    pub fn mark_invalid(&mut self, reason: String) {
        self.status = ApiKeyVerificationStatus::Invalid { reason };
        self.last_verified_at = Some(Instant::now());
    }

    pub fn mark_error(&mut self, message: String) {
        self.status = ApiKeyVerificationStatus::Error { message };
    }

    pub fn is_verifying(&self) -> bool {
        self.status == ApiKeyVerificationStatus::Verifying
    }

    pub fn is_valid(&self) -> bool {
        self.status == ApiKeyVerificationStatus::Valid
    }

    pub fn needs_verification(&self) -> bool {
        !self.api_key.is_empty() && self.status == ApiKeyVerificationStatus::Idle
    }
}

impl Default for ApiKeyVerificationState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("arrow_key_history.rs", '''//! Arrow key history navigation (useArrowKeyHistory.tsx).
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
'''))

files.append(("assistant_history.rs", '''//! Assistant history hook (useAssistantHistory.ts).
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
'''))

files.append(("away_summary.rs", '''//! Away summary hook (useAwaySummary.ts).
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
'''))

files.append(("background_task_navigation.rs", '''//! Background task navigation hook (useBackgroundTaskNavigation.ts).
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
'''))

files.append(("blink.rs", '''//! Blink animation hook (useBlink.ts).
//!
//! Synchronized blinking animation that pauses when offscreen or
//! when the terminal is blurred.

use std::time::{Duration, Instant};

/// Default blink interval in milliseconds.
pub const BLINK_INTERVAL_MS: u64 = 600;

/// State for blink animation.
#[derive(Debug, Clone)]
pub struct BlinkState {
    pub enabled: bool,
    pub interval: Duration,
    pub start_time: Instant,
    pub terminal_focused: bool,
    pub element_visible: bool,
}

impl BlinkState {
    pub fn new() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_millis(BLINK_INTERVAL_MS),
            start_time: Instant::now(),
            terminal_focused: true,
            element_visible: true,
        }
    }

    pub fn with_interval(mut self, ms: u64) -> Self {
        self.interval = Duration::from_millis(ms);
        self
    }

    /// Compute whether the blink is currently visible.
    pub fn is_visible(&self) -> bool {
        if !self.enabled || !self.terminal_focused {
            return true;
        }
        if !self.element_visible {
            return true;
        }
        let elapsed = self.start_time.elapsed();
        let cycle = elapsed.as_millis() / self.interval.as_millis().max(1);
        cycle % 2 == 0
    }

    /// Set whether the animation is enabled.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Update terminal focus state.
    pub fn set_terminal_focused(&mut self, focused: bool) {
        self.terminal_focused = focused;
    }

    /// Update element visibility (in viewport).
    pub fn set_element_visible(&mut self, visible: bool) {
        self.element_visible = visible;
    }

    /// Reset the blink clock.
    pub fn reset(&mut self) {
        self.start_time = Instant::now();
    }
}

impl Default for BlinkState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

for fname, content in files:
    path = os.path.join(BASE, fname)
    with open(path, 'w') as f:
        f.write(content)
    print(f"Created {fname}")

print(f"\nTotal: {len(files)} files created")
