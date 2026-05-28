#!/usr/bin/env python3
"""Generate hooks part 3 - global_keybindings through memory_usage."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/hooks"
files = []

files.append(("global_keybindings.rs", '''//! Global keybindings hook (useGlobalKeybindings.tsx).
//!
//! Registers application-wide keybindings that are always active.

use std::collections::HashMap;

/// A global keybinding action.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GlobalAction {
    ToggleHelp,
    ToggleSettings,
    QuickSearch,
    ClearScreen,
    ToggleVim,
    ToggleFullscreen,
    CycleTheme,
    ToggleDevBar,
    ShowExport,
    Custom(String),
}

/// State for global keybindings.
#[derive(Debug, Clone)]
pub struct GlobalKeybindingsState {
    pub bindings: HashMap<String, GlobalAction>,
    pub enabled: bool,
    pub last_triggered: Option<GlobalAction>,
}

impl GlobalKeybindingsState {
    pub fn new() -> Self {
        let mut bindings = HashMap::new();
        bindings.insert("ctrl+/".to_string(), GlobalAction::ToggleHelp);
        bindings.insert("ctrl+,".to_string(), GlobalAction::ToggleSettings);
        bindings.insert("ctrl+r".to_string(), GlobalAction::QuickSearch);
        bindings.insert("ctrl+l".to_string(), GlobalAction::ClearScreen);
        Self {
            bindings,
            enabled: true,
            last_triggered: None,
        }
    }

    /// Process an input key. Returns the action if matched.
    pub fn process_key(&mut self, key: &str) -> Option<&GlobalAction> {
        if !self.enabled {
            return None;
        }
        if let Some(action) = self.bindings.get(key) {
            self.last_triggered = Some(action.clone());
            Some(action)
        } else {
            None
        }
    }

    /// Register a custom keybinding.
    pub fn register(&mut self, key: String, action: GlobalAction) {
        self.bindings.insert(key, action);
    }

    /// Unregister a keybinding.
    pub fn unregister(&mut self, key: &str) {
        self.bindings.remove(key);
    }

    /// Enable/disable global keybindings.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Default for GlobalKeybindingsState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("history_search.rs", '''//! History search hook (useHistorySearch.ts).
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
'''))

files.append(("ide_at_mentioned.rs", '''//! IDE at-mention hook (useIdeAtMentioned.ts).
//!
//! Tracks when the IDE sends an @-mention for a file or symbol.

/// State for IDE at-mention tracking.
#[derive(Debug, Clone)]
pub struct IdeAtMentionedState {
    pub mentions: Vec<AtMention>,
    pub last_mention: Option<AtMention>,
}

#[derive(Debug, Clone)]
pub struct AtMention {
    pub text: String,
    pub file_path: Option<String>,
    pub symbol: Option<String>,
    pub line_range: Option<(u32, u32)>,
    pub timestamp: u64,
}

impl IdeAtMentionedState {
    pub fn new() -> Self {
        Self {
            mentions: Vec::new(),
            last_mention: None,
        }
    }

    /// Add a new mention from the IDE.
    pub fn add_mention(&mut self, mention: AtMention) {
        self.last_mention = Some(mention.clone());
        self.mentions.push(mention);
    }

    /// Clear all mentions.
    pub fn clear(&mut self) {
        self.mentions.clear();
        self.last_mention = None;
    }

    /// Get pending mentions and clear them.
    pub fn take_mentions(&mut self) -> Vec<AtMention> {
        let taken = std::mem::take(&mut self.mentions);
        self.last_mention = None;
        taken
    }

    pub fn has_mentions(&self) -> bool {
        !self.mentions.is_empty()
    }
}

impl Default for IdeAtMentionedState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("ide_connection_status.rs", '''//! IDE connection status hook (useIdeConnectionStatus.ts).
//!
//! Monitors the connection status to an IDE (VS Code, etc.).

use std::time::Instant;

/// IDE connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error,
}

/// State for IDE connection status monitoring.
#[derive(Debug, Clone)]
pub struct IdeConnectionStatusState {
    pub status: IdeConnectionStatus,
    pub ide_name: Option<String>,
    pub connected_at: Option<Instant>,
    pub last_heartbeat: Option<Instant>,
    pub reconnect_attempts: u32,
    pub error_message: Option<String>,
}

impl IdeConnectionStatusState {
    pub fn new() -> Self {
        Self {
            status: IdeConnectionStatus::Disconnected,
            ide_name: None,
            connected_at: None,
            last_heartbeat: None,
            reconnect_attempts: 0,
            error_message: None,
        }
    }

    /// Mark as connecting.
    pub fn connecting(&mut self) {
        self.status = IdeConnectionStatus::Connecting;
        self.error_message = None;
    }

    /// Mark as connected.
    pub fn connected(&mut self, ide_name: String) {
        self.status = IdeConnectionStatus::Connected;
        self.ide_name = Some(ide_name);
        self.connected_at = Some(Instant::now());
        self.last_heartbeat = Some(Instant::now());
        self.reconnect_attempts = 0;
        self.error_message = None;
    }

    /// Process a heartbeat.
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Some(Instant::now());
    }

    /// Mark as disconnected.
    pub fn disconnected(&mut self) {
        self.status = IdeConnectionStatus::Disconnected;
        self.connected_at = None;
    }

    /// Mark as reconnecting.
    pub fn reconnecting(&mut self) {
        self.status = IdeConnectionStatus::Reconnecting;
        self.reconnect_attempts += 1;
    }

    /// Mark as error.
    pub fn error(&mut self, message: String) {
        self.status = IdeConnectionStatus::Error;
        self.error_message = Some(message);
    }

    /// Check if connection may be stale (no heartbeat for 30s).
    pub fn is_stale(&self) -> bool {
        match self.last_heartbeat {
            Some(last) => last.elapsed().as_secs() > 30,
            None => false,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.status == IdeConnectionStatus::Connected
    }
}

impl Default for IdeConnectionStatusState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("ide_integration.rs", '''//! IDE integration hook (useIDEIntegration.tsx).
//!
//! Manages the full integration with an IDE: file syncing, selection
//! tracking, and command forwarding.

use std::collections::HashMap;

/// An IDE command that can be forwarded.
#[derive(Debug, Clone)]
pub struct IdeCommand {
    pub command: String,
    pub args: HashMap<String, serde_json::Value>,
}

/// State for IDE integration.
#[derive(Debug, Clone)]
pub struct IdeIntegrationState {
    pub connected: bool,
    pub active_file: Option<String>,
    pub selection: Option<IdeSelection>,
    pub pending_commands: Vec<IdeCommand>,
    pub synced_files: Vec<String>,
    pub ide_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IdeSelection {
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub text: String,
}

impl IdeIntegrationState {
    pub fn new() -> Self {
        Self {
            connected: false,
            active_file: None,
            selection: None,
            pending_commands: Vec::new(),
            synced_files: Vec::new(),
            ide_type: None,
        }
    }

    /// Update the active file in IDE.
    pub fn set_active_file(&mut self, path: String) {
        self.active_file = Some(path);
    }

    /// Update the current selection.
    pub fn set_selection(&mut self, selection: IdeSelection) {
        self.selection = Some(selection);
    }

    /// Clear the selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Queue a command to send to the IDE.
    pub fn send_command(&mut self, command: IdeCommand) {
        self.pending_commands.push(command);
    }

    /// Take all pending commands.
    pub fn take_commands(&mut self) -> Vec<IdeCommand> {
        std::mem::take(&mut self.pending_commands)
    }

    /// Open a file in the IDE.
    pub fn open_file(&mut self, path: &str, line: Option<u32>) {
        let mut args = HashMap::new();
        args.insert("path".to_string(), serde_json::Value::String(path.to_string()));
        if let Some(l) = line {
            args.insert("line".to_string(), serde_json::Value::Number(l.into()));
        }
        self.send_command(IdeCommand {
            command: "openFile".to_string(),
            args,
        });
    }

    /// Show a diff in the IDE.
    pub fn show_diff(&mut self, old_path: &str, new_path: &str, title: &str) {
        let mut args = HashMap::new();
        args.insert("oldPath".to_string(), serde_json::Value::String(old_path.to_string()));
        args.insert("newPath".to_string(), serde_json::Value::String(new_path.to_string()));
        args.insert("title".to_string(), serde_json::Value::String(title.to_string()));
        self.send_command(IdeCommand {
            command: "showDiff".to_string(),
            args,
        });
    }

    pub fn set_connected(&mut self, connected: bool, ide_type: Option<String>) {
        self.connected = connected;
        self.ide_type = ide_type;
    }
}

impl Default for IdeIntegrationState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("ide_logging.rs", '''//! IDE logging hook (useIdeLogging.ts).
//!
//! Forwards log messages to the connected IDE for display.

/// Log level for IDE messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeLogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// State for IDE logging.
#[derive(Debug, Clone)]
pub struct IdeLoggingState {
    pub enabled: bool,
    pub buffer: Vec<IdeLogEntry>,
    pub max_buffer_size: usize,
    pub min_level: IdeLogLevel,
}

#[derive(Debug, Clone)]
pub struct IdeLogEntry {
    pub level: IdeLogLevel,
    pub message: String,
    pub source: String,
    pub timestamp: u64,
}

impl IdeLoggingState {
    pub fn new() -> Self {
        Self {
            enabled: false,
            buffer: Vec::new(),
            max_buffer_size: 1000,
            min_level: IdeLogLevel::Info,
        }
    }

    /// Log a message.
    pub fn log(&mut self, level: IdeLogLevel, message: String, source: String) {
        if !self.enabled || (level as u8) < (self.min_level as u8) {
            return;
        }
        self.buffer.push(IdeLogEntry {
            level,
            message,
            source,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        });
        if self.buffer.len() > self.max_buffer_size {
            self.buffer.remove(0);
        }
    }

    /// Take all buffered log entries.
    pub fn take_entries(&mut self) -> Vec<IdeLogEntry> {
        std::mem::take(&mut self.buffer)
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_min_level(&mut self, level: IdeLogLevel) {
        self.min_level = level;
    }
}

impl Default for IdeLoggingState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("ide_selection.rs", '''//! IDE selection hook (useIdeSelection.ts).
//!
//! Tracks the user\'s current text selection in the IDE.

/// State for IDE selection tracking.
#[derive(Debug, Clone)]
pub struct IdeSelectionState {
    pub file_path: Option<String>,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub selected_text: String,
    pub is_active: bool,
}

impl IdeSelectionState {
    pub fn new() -> Self {
        Self {
            file_path: None,
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            selected_text: String::new(),
            is_active: false,
        }
    }

    /// Update the selection.
    pub fn update(
        &mut self,
        file_path: String,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        text: String,
    ) {
        self.file_path = Some(file_path);
        self.start_line = start_line;
        self.start_col = start_col;
        self.end_line = end_line;
        self.end_col = end_col;
        self.selected_text = text;
        self.is_active = true;
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.file_path = None;
        self.start_line = 0;
        self.start_col = 0;
        self.end_line = 0;
        self.end_col = 0;
        self.selected_text.clear();
        self.is_active = false;
    }

    /// Check if there is an active selection.
    pub fn has_selection(&self) -> bool {
        self.is_active && !self.selected_text.is_empty()
    }

    /// Get selection as a context reference string.
    pub fn as_context_ref(&self) -> Option<String> {
        if !self.has_selection() {
            return None;
        }
        let path = self.file_path.as_deref().unwrap_or("unknown");
        Some(format!("{}:{}-{}", path, self.start_line, self.end_line))
    }
}

impl Default for IdeSelectionState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("inbox_poller.rs", '''//! Inbox poller hook (useInboxPoller.ts).
//!
//! Periodically polls for new messages/notifications from the server.

use std::time::{Duration, Instant};

/// State for inbox polling.
#[derive(Debug, Clone)]
pub struct InboxPollerState {
    pub is_polling: bool,
    pub last_poll: Option<Instant>,
    pub poll_interval: Duration,
    pub unread_count: u32,
    pub error_count: u32,
    pub max_errors: u32,
    pub disabled: bool,
}

impl InboxPollerState {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            is_polling: false,
            last_poll: None,
            poll_interval: Duration::from_millis(interval_ms),
            unread_count: 0,
            error_count: 0,
            max_errors: 5,
            disabled: false,
        }
    }

    /// Check if it\'s time to poll.
    pub fn should_poll(&self) -> bool {
        if self.disabled || self.is_polling {
            return false;
        }
        match self.last_poll {
            Some(last) => last.elapsed() >= self.poll_interval,
            None => true,
        }
    }

    /// Start a poll.
    pub fn start_poll(&mut self) {
        self.is_polling = true;
    }

    /// Complete a successful poll.
    pub fn complete_poll(&mut self, unread: u32) {
        self.is_polling = false;
        self.last_poll = Some(Instant::now());
        self.unread_count = unread;
        self.error_count = 0;
    }

    /// Record a poll error.
    pub fn poll_error(&mut self) {
        self.is_polling = false;
        self.error_count += 1;
        if self.error_count >= self.max_errors {
            self.disabled = true;
        }
    }

    /// Reset error state and re-enable polling.
    pub fn reset_errors(&mut self) {
        self.error_count = 0;
        self.disabled = false;
    }
}

impl Default for InboxPollerState {
    fn default() -> Self {
        Self::new(30_000)
    }
}
'''))

files.append(("input_buffer.rs", '''//! Input buffer hook (useInputBuffer.ts).
//!
//! Manages an undo buffer for text input with debounced snapshots.

use std::time::{Duration, Instant};

/// A snapshot of input state for undo.
#[derive(Debug, Clone)]
pub struct BufferEntry {
    pub text: String,
    pub cursor_offset: usize,
    pub timestamp: Instant,
}

/// State for the input undo buffer.
#[derive(Debug, Clone)]
pub struct InputBufferState {
    pub buffer: Vec<BufferEntry>,
    pub current_index: i32,
    pub max_buffer_size: usize,
    pub debounce_ms: u64,
    pub last_push_time: Option<Instant>,
}

impl InputBufferState {
    pub fn new(max_size: usize, debounce_ms: u64) -> Self {
        Self {
            buffer: Vec::new(),
            current_index: -1,
            max_buffer_size: max_size,
            debounce_ms,
            last_push_time: None,
        }
    }

    /// Push a new entry to the buffer (with debounce).
    pub fn push(&mut self, text: String, cursor_offset: usize) {
        let now = Instant::now();

        // Check debounce
        if let Some(last) = self.last_push_time {
            if now.duration_since(last) < Duration::from_millis(self.debounce_ms) {
                // Update the last entry instead of pushing new
                if let Some(entry) = self.buffer.last_mut() {
                    entry.text = text;
                    entry.cursor_offset = cursor_offset;
                    entry.timestamp = now;
                    return;
                }
            }
        }

        // Truncate any redo history
        let idx = (self.current_index + 1) as usize;
        self.buffer.truncate(idx);

        // Push new entry
        self.buffer.push(BufferEntry {
            text,
            cursor_offset,
            timestamp: now,
        });

        // Trim to max size
        if self.buffer.len() > self.max_buffer_size {
            self.buffer.remove(0);
        }

        self.current_index = self.buffer.len() as i32 - 1;
        self.last_push_time = Some(now);
    }

    /// Undo: move back one entry. Returns the restored state.
    pub fn undo(&mut self) -> Option<&BufferEntry> {
        if self.current_index <= 0 {
            return None;
        }
        self.current_index -= 1;
        self.buffer.get(self.current_index as usize)
    }

    /// Check if undo is available.
    pub fn can_undo(&self) -> bool {
        self.current_index > 0
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.current_index = -1;
        self.last_push_time = None;
    }
}

impl Default for InputBufferState {
    fn default() -> Self {
        Self::new(50, 300)
    }
}
'''))

files.append(("issue_flag_banner.rs", '''//! Issue flag banner hook (useIssueFlagBanner.ts).
//!
//! Displays a banner when a known issue affects the current session.

/// State for issue flag banner display.
#[derive(Debug, Clone)]
pub struct IssueFlagBannerState {
    pub active_issues: Vec<IssueFlagEntry>,
    pub dismissed: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct IssueFlagEntry {
    pub id: String,
    pub message: String,
    pub severity: IssueSeverity,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    Info,
    Warning,
    Critical,
}

impl IssueFlagBannerState {
    pub fn new() -> Self {
        Self {
            active_issues: Vec::new(),
            dismissed: Vec::new(),
        }
    }

    /// Add an issue flag.
    pub fn add_issue(&mut self, issue: IssueFlagEntry) {
        if !self.dismissed.contains(&issue.id) {
            self.active_issues.push(issue);
        }
    }

    /// Dismiss an issue.
    pub fn dismiss(&mut self, id: &str) {
        self.active_issues.retain(|i| i.id != id);
        self.dismissed.push(id.to_string());
    }

    /// Get visible (non-dismissed) issues.
    pub fn visible_issues(&self) -> &[IssueFlagEntry] {
        &self.active_issues
    }

    /// Check if there are any visible issues.
    pub fn has_issues(&self) -> bool {
        !self.active_issues.is_empty()
    }
}

impl Default for IssueFlagBannerState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("log_messages.rs", '''//! Log messages hook (useLogMessages.ts).
//!
//! Manages a buffer of log messages for display in the dev bar.

use std::collections::VecDeque;

/// A log message entry.
#[derive(Debug, Clone)]
pub struct LogMessage {
    pub id: u64,
    pub level: LogLevel,
    pub message: String,
    pub source: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// State for log message buffer.
#[derive(Debug, Clone)]
pub struct LogMessagesState {
    pub messages: VecDeque<LogMessage>,
    pub max_messages: usize,
    pub next_id: u64,
    pub filter_level: LogLevel,
    pub is_visible: bool,
}

impl LogMessagesState {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: VecDeque::new(),
            max_messages,
            next_id: 0,
            filter_level: LogLevel::Info,
            is_visible: false,
        }
    }

    /// Add a log message.
    pub fn push(&mut self, level: LogLevel, message: String, source: String) {
        let id = self.next_id;
        self.next_id += 1;
        self.messages.push_back(LogMessage {
            id,
            level,
            message,
            source,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        });
        if self.messages.len() > self.max_messages {
            self.messages.pop_front();
        }
    }

    /// Get messages filtered by current level.
    pub fn filtered(&self) -> Vec<&LogMessage> {
        self.messages.iter()
            .filter(|m| (m.level as u8) >= (self.filter_level as u8))
            .collect()
    }

    /// Set the minimum display level.
    pub fn set_filter(&mut self, level: LogLevel) {
        self.filter_level = level;
    }

    /// Clear all messages.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Toggle visibility.
    pub fn toggle_visible(&mut self) {
        self.is_visible = !self.is_visible;
    }
}

impl Default for LogMessagesState {
    fn default() -> Self {
        Self::new(500)
    }
}
'''))

files.append(("lsp_plugin_recommendation.rs", '''//! LSP plugin recommendation hook (useLspPluginRecommendation.tsx).
//!
//! Recommends LSP plugins based on detected file types in the workspace.

/// State for LSP plugin recommendations.
#[derive(Debug, Clone)]
pub struct LspPluginRecommendationState {
    pub recommendations: Vec<PluginRecommendation>,
    pub dismissed: Vec<String>,
    pub checked: bool,
}

#[derive(Debug, Clone)]
pub struct PluginRecommendation {
    pub plugin_id: String,
    pub plugin_name: String,
    pub language: String,
    pub reason: String,
}

impl LspPluginRecommendationState {
    pub fn new() -> Self {
        Self {
            recommendations: Vec::new(),
            dismissed: Vec::new(),
            checked: false,
        }
    }

    /// Check workspace and generate recommendations.
    pub fn check_workspace(&mut self, detected_languages: &[String], installed_plugins: &[String]) {
        self.checked = true;
        self.recommendations.clear();

        for lang in detected_languages {
            let plugin_id = match lang.as_str() {
                "typescript" | "javascript" => "typescript-lsp",
                "python" => "python-lsp",
                "rust" => "rust-analyzer",
                "go" => "gopls",
                _ => continue,
            };

            if !installed_plugins.contains(&plugin_id.to_string()) && !self.dismissed.contains(&plugin_id.to_string()) {
                self.recommendations.push(PluginRecommendation {
                    plugin_id: plugin_id.to_string(),
                    plugin_name: format!("{} Language Server", lang),
                    language: lang.clone(),
                    reason: format!("Detected {} files in workspace", lang),
                });
            }
        }
    }

    /// Dismiss a recommendation.
    pub fn dismiss(&mut self, plugin_id: &str) {
        self.recommendations.retain(|r| r.plugin_id != plugin_id);
        self.dismissed.push(plugin_id.to_string());
    }

    /// Get active recommendations.
    pub fn active(&self) -> &[PluginRecommendation] {
        &self.recommendations
    }
}

impl Default for LspPluginRecommendationState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("mailbox_bridge.rs", '''//! Mailbox bridge hook (useMailboxBridge.ts).
//!
//! Bridges the mailbox system with the React render cycle,
//! forwarding messages between the mailbox and UI state.

use std::collections::VecDeque;

/// A mailbox message.
#[derive(Debug, Clone)]
pub struct MailboxMessage {
    pub id: String,
    pub channel: String,
    pub payload: serde_json::Value,
    pub timestamp: u64,
}

/// State for the mailbox bridge.
#[derive(Debug, Clone)]
pub struct MailboxBridgeState {
    pub inbox: VecDeque<MailboxMessage>,
    pub outbox: VecDeque<MailboxMessage>,
    pub subscribed_channels: Vec<String>,
    pub connected: bool,
}

impl MailboxBridgeState {
    pub fn new() -> Self {
        Self {
            inbox: VecDeque::new(),
            outbox: VecDeque::new(),
            subscribed_channels: Vec::new(),
            connected: false,
        }
    }

    /// Subscribe to a channel.
    pub fn subscribe(&mut self, channel: String) {
        if !self.subscribed_channels.contains(&channel) {
            self.subscribed_channels.push(channel);
        }
    }

    /// Unsubscribe from a channel.
    pub fn unsubscribe(&mut self, channel: &str) {
        self.subscribed_channels.retain(|c| c != channel);
    }

    /// Receive a message into the inbox.
    pub fn receive(&mut self, message: MailboxMessage) {
        if self.subscribed_channels.contains(&message.channel) {
            self.inbox.push_back(message);
        }
    }

    /// Send a message (add to outbox).
    pub fn send(&mut self, channel: String, payload: serde_json::Value) {
        self.outbox.push_back(MailboxMessage {
            id: uuid::Uuid::new_v4().to_string(),
            channel,
            payload,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        });
    }

    /// Take all inbox messages.
    pub fn take_inbox(&mut self) -> Vec<MailboxMessage> {
        self.inbox.drain(..).collect()
    }

    /// Take all outbox messages.
    pub fn take_outbox(&mut self) -> Vec<MailboxMessage> {
        self.outbox.drain(..).collect()
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }
}

impl Default for MailboxBridgeState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("main_loop_model.rs", '''//! Main loop model hook (useMainLoopModel.ts).
//!
//! Manages the active model selection for the main conversation loop.

/// State for main loop model selection.
#[derive(Debug, Clone)]
pub struct MainLoopModelState {
    pub current_model: String,
    pub available_models: Vec<ModelInfo>,
    pub fallback_model: Option<String>,
    pub is_fast_mode: bool,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub max_tokens: u32,
    pub supports_vision: bool,
    pub supports_tools: bool,
}

impl MainLoopModelState {
    pub fn new(default_model: &str) -> Self {
        Self {
            current_model: default_model.to_string(),
            available_models: Vec::new(),
            fallback_model: None,
            is_fast_mode: false,
        }
    }

    /// Set the current model.
    pub fn set_model(&mut self, model_id: String) {
        self.current_model = model_id;
    }

    /// Toggle fast mode.
    pub fn toggle_fast_mode(&mut self) {
        self.is_fast_mode = !self.is_fast_mode;
    }

    /// Set available models.
    pub fn set_available_models(&mut self, models: Vec<ModelInfo>) {
        self.available_models = models;
    }

    /// Get current model info.
    pub fn current_model_info(&self) -> Option<&ModelInfo> {
        self.available_models.iter().find(|m| m.id == self.current_model)
    }

    /// Get the effective model (considering fast mode and fallback).
    pub fn effective_model(&self) -> &str {
        if self.is_fast_mode {
            self.fallback_model.as_deref().unwrap_or(&self.current_model)
        } else {
            &self.current_model
        }
    }
}

impl Default for MainLoopModelState {
    fn default() -> Self {
        Self::new("default")
    }
}
'''))

files.append(("manage_plugins.rs", '''//! Manage plugins hook (useManagePlugins.ts).
//!
//! Manages the installation, update, and removal of plugins.

use std::collections::HashMap;

/// Plugin installation status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginStatus {
    NotInstalled,
    Installing,
    Installed,
    Updating,
    Removing,
    Error(String),
}

/// Information about an installed plugin.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub status: PluginStatus,
    pub enabled: bool,
    pub auto_update: bool,
}

/// State for plugin management.
#[derive(Debug, Clone)]
pub struct ManagePluginsState {
    pub plugins: HashMap<String, PluginEntry>,
    pub pending_operations: Vec<String>,
}

impl ManagePluginsState {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            pending_operations: Vec::new(),
        }
    }

    /// Register a plugin.
    pub fn register(&mut self, plugin: PluginEntry) {
        self.plugins.insert(plugin.id.clone(), plugin);
    }

    /// Start installing a plugin.
    pub fn install(&mut self, id: &str) {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.status = PluginStatus::Installing;
            self.pending_operations.push(id.to_string());
        }
    }

    /// Mark installation as complete.
    pub fn install_complete(&mut self, id: &str, version: String) {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.status = PluginStatus::Installed;
            plugin.version = version;
        }
        self.pending_operations.retain(|i| i != id);
    }

    /// Start removing a plugin.
    pub fn remove(&mut self, id: &str) {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.status = PluginStatus::Removing;
            self.pending_operations.push(id.to_string());
        }
    }

    /// Mark removal as complete.
    pub fn remove_complete(&mut self, id: &str) {
        self.plugins.remove(id);
        self.pending_operations.retain(|i| i != id);
    }

    /// Toggle plugin enabled state.
    pub fn toggle_enabled(&mut self, id: &str) -> bool {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.enabled = !plugin.enabled;
            plugin.enabled
        } else {
            false
        }
    }

    /// Mark a plugin operation as failed.
    pub fn mark_error(&mut self, id: &str, error: String) {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.status = PluginStatus::Error(error);
        }
        self.pending_operations.retain(|i| i != id);
    }

    /// Get all installed plugins.
    pub fn installed(&self) -> Vec<&PluginEntry> {
        self.plugins.values()
            .filter(|p| p.status == PluginStatus::Installed)
            .collect()
    }

    /// Check if any operations are pending.
    pub fn has_pending(&self) -> bool {
        !self.pending_operations.is_empty()
    }
}

impl Default for ManagePluginsState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("memory_usage.rs", '''//! Memory usage hook (useMemoryUsage.ts).
//!
//! Monitors process memory usage and reports when thresholds are exceeded.

/// Memory usage status level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryUsageStatus {
    Normal,
    High,
    Critical,
}

/// Memory usage information.
#[derive(Debug, Clone)]
pub struct MemoryUsageInfo {
    pub heap_used: u64,
    pub status: MemoryUsageStatus,
}

/// Thresholds for memory monitoring.
const HIGH_MEMORY_THRESHOLD: u64 = 1_500_000_000; // 1.5GB
const CRITICAL_MEMORY_THRESHOLD: u64 = 2_500_000_000; // 2.5GB

/// State for memory usage monitoring.
#[derive(Debug, Clone)]
pub struct MemoryUsageState {
    pub current: Option<MemoryUsageInfo>,
    pub poll_interval_ms: u64,
    pub last_poll_ms: Option<u64>,
}

impl MemoryUsageState {
    pub fn new() -> Self {
        Self {
            current: None,
            poll_interval_ms: 10_000,
            last_poll_ms: None,
        }
    }

    /// Update with a new memory reading.
    pub fn update(&mut self, heap_used: u64) {
        let status = if heap_used >= CRITICAL_MEMORY_THRESHOLD {
            MemoryUsageStatus::Critical
        } else if heap_used >= HIGH_MEMORY_THRESHOLD {
            MemoryUsageStatus::High
        } else {
            MemoryUsageStatus::Normal
        };

        // Only store non-normal readings to avoid unnecessary re-renders
        self.current = if status == MemoryUsageStatus::Normal {
            None
        } else {
            Some(MemoryUsageInfo { heap_used, status })
        };
    }

    /// Get the current status.
    pub fn status(&self) -> MemoryUsageStatus {
        self.current.as_ref().map_or(MemoryUsageStatus::Normal, |i| i.status)
    }

    /// Check if memory is in a warning state.
    pub fn is_warning(&self) -> bool {
        matches!(self.status(), MemoryUsageStatus::High | MemoryUsageStatus::Critical)
    }
}

impl Default for MemoryUsageState {
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
